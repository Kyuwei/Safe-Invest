using Microsoft.Extensions.Caching.Memory;
using Microsoft.Extensions.Logging;
using SafeInvest.Core.Abstractions;
using SafeInvest.Core.Models;
using SafeInvest.MarketData.Fx;

namespace SafeInvest.MarketData;

/// <summary>The single entry point the app and the MCP server use to get prices.</summary>
public interface IMarketDataService
{
    /// <summary>Quotes keyed by <see cref="Asset.Key"/>. Assets that could not be priced are simply absent.</summary>
    Task<IReadOnlyDictionary<string, Quote>> GetQuotesAsync(
        IReadOnlyCollection<Asset> assets,
        string currency,
        CancellationToken cancellationToken = default);

    Task<Quote?> GetQuoteAsync(Asset asset, string currency, CancellationToken cancellationToken = default);

    Task<IReadOnlyList<Asset>> SearchAsync(
        string query,
        AssetKind? kind,
        int limit = 15,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<Candle>> GetHistoryAsync(
        Asset asset,
        string currency,
        HistoryRange range,
        CancellationToken cancellationToken = default);

    /// <summary>What the Réglages screen shows next to each source.</summary>
    IReadOnlyList<ProviderStatus> GetProviderStatuses();
}

/// <summary>
/// Runs the configured providers as a fall-through chain: the first source that answers
/// wins, and a failure (outage, expired quota, changed markup) simply moves to the next
/// one, ending at the simulator so the app is never dead in the water.
///
/// It also normalises currencies — Yahoo quotes Microsoft in dollars while a game is
/// played in euros — and caches quotes briefly, which is what keeps a free tier free.
/// </summary>
public sealed class MarketDataService(
    IEnumerable<IQuoteProvider> providers,
    IFxRateService fxRateService,
    IMemoryCache cache,
    MarketDataOptions options,
    ILogger<MarketDataService>? logger = null,
    TimeProvider? timeProvider = null) : IMarketDataService
{
    private readonly IReadOnlyList<IQuoteProvider> _providers = [.. providers];
    private readonly TimeProvider _clock = timeProvider ?? TimeProvider.System;
    private readonly Dictionary<string, ProviderStatus> _statuses = [];
    private readonly Lock _statusGate = new();

    public async Task<IReadOnlyDictionary<string, Quote>> GetQuotesAsync(
        IReadOnlyCollection<Asset> assets,
        string currency,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(assets);

        string target = Normalise(currency);
        Dictionary<string, Quote> found = [];
        List<Asset> outstanding = [];

        foreach (Asset asset in assets.DistinctBy(a => a.Key))
        {
            if (cache.TryGetValue(CacheKey(asset, target), out Quote? cached) && cached is not null)
            {
                found[asset.Key] = cached;
            }
            else
            {
                outstanding.Add(AssetCatalog.Enrich(asset));
            }
        }

        if (outstanding.Count == 0)
        {
            return found;
        }

        foreach (AssetKind kind in outstanding.Select(a => a.Kind).Distinct())
        {
            List<Asset> pending = [.. outstanding.Where(a => a.Kind == kind)];

            foreach (IQuoteProvider provider in ChainFor(kind))
            {
                if (pending.Count == 0)
                {
                    break;
                }

                IReadOnlyList<Quote> quotes;
                try
                {
                    quotes = await provider
                        .GetQuotesAsync(pending, target, cancellationToken)
                        .ConfigureAwait(false);

                    RecordSuccess(provider);
                }
                catch (Exception ex) when (ex is not OperationCanceledException)
                {
                    RecordFailure(provider, ex);
                    logger?.LogWarning(ex, "Source {Provider} indisponible, passage à la suivante.", provider.Id);
                    continue;
                }

                foreach (Quote quote in quotes)
                {
                    Quote normalised = await ToTargetCurrencyAsync(quote, target, cancellationToken)
                        .ConfigureAwait(false);

                    if (normalised.Price <= 0m)
                    {
                        continue;
                    }

                    found[normalised.Key] = normalised;
                    cache.Set(CacheKey(normalised, target), normalised, options.QuoteCacheDuration);
                }

                pending.RemoveAll(a => found.ContainsKey(a.Key));
            }
        }

        return found;
    }

    public async Task<Quote?> GetQuoteAsync(
        Asset asset,
        string currency,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(asset);

        IReadOnlyDictionary<string, Quote> quotes = await GetQuotesAsync([asset], currency, cancellationToken)
            .ConfigureAwait(false);

        return quotes.GetValueOrDefault(asset.Key);
    }

    public async Task<IReadOnlyList<Asset>> SearchAsync(
        string query,
        AssetKind? kind,
        int limit = 15,
        CancellationToken cancellationToken = default)
    {
        Dictionary<string, Asset> results = [];

        // The built-in catalog answers instantly and covers the assets most users want.
        foreach (Asset asset in AssetCatalog.Search(query, kind, limit))
        {
            results[asset.Key] = asset;
        }

        if (string.IsNullOrWhiteSpace(query))
        {
            return [.. results.Values.Take(limit)];
        }

        foreach (AssetKind searched in KindsToSearch(kind))
        {
            foreach (IQuoteProvider provider in ChainFor(searched))
            {
                if (results.Count >= limit)
                {
                    break;
                }

                try
                {
                    IReadOnlyList<Asset> found = await provider
                        .SearchAsync(query, searched, limit, cancellationToken)
                        .ConfigureAwait(false);

                    RecordSuccess(provider);

                    foreach (Asset asset in found)
                    {
                        results.TryAdd(asset.Key, asset);
                    }

                    if (found.Count > 0)
                    {
                        break;
                    }
                }
                catch (Exception ex) when (ex is not OperationCanceledException)
                {
                    RecordFailure(provider, ex);
                }
            }
        }

        return [.. results.Values.Take(limit)];
    }

    public async Task<IReadOnlyList<Candle>> GetHistoryAsync(
        Asset asset,
        string currency,
        HistoryRange range,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(asset);

        string target = Normalise(currency);
        string key = $"history:{asset.Key}:{target}:{range}";

        if (cache.TryGetValue(key, out IReadOnlyList<Candle>? cached) && cached is not null)
        {
            return cached;
        }

        Asset enriched = AssetCatalog.Enrich(asset);

        foreach (IQuoteProvider provider in ChainFor(asset.Kind))
        {
            try
            {
                IReadOnlyList<Candle> candles = await provider
                    .GetHistoryAsync(enriched, target, range, cancellationToken)
                    .ConfigureAwait(false);

                if (candles.Count == 0)
                {
                    continue;
                }

                RecordSuccess(provider);
                cache.Set(key, candles, options.HistoryCacheDuration);
                return candles;
            }
            catch (Exception ex) when (ex is not OperationCanceledException)
            {
                RecordFailure(provider, ex);
            }
        }

        return [];
    }

    public IReadOnlyList<ProviderStatus> GetProviderStatuses()
    {
        lock (_statusGate)
        {
            return
            [
                .. _providers.Select(p => _statuses.GetValueOrDefault(p.Id) ?? new ProviderStatus
                {
                    Id = p.Id,
                    DisplayName = p.DisplayName,
                    IsConfigured = p.IsConfigured,
                    IsSimulated = p.IsSimulated,
                }),
            ];
        }
    }

    /// <summary>
    /// The providers to try for a family, in the user's configured order. Sources missing
    /// their API key are dropped here rather than failing later, and the simulator is
    /// always appended so the chain can never come back empty-handed.
    /// </summary>
    private IEnumerable<IQuoteProvider> ChainFor(AssetKind kind)
    {
        if (options.ForceSimulated)
        {
            return _providers.Where(p => p.IsSimulated);
        }

        IReadOnlyList<string> order = kind == AssetKind.Crypto
            ? options.CryptoProviderOrder
            : options.StockProviderOrder;

        List<IQuoteProvider> chain = [];

        foreach (string id in order)
        {
            IQuoteProvider? provider = _providers.FirstOrDefault(
                p => p.Id.Equals(id, StringComparison.OrdinalIgnoreCase));

            if (provider is not null
                && provider.IsConfigured
                && provider.SupportedKinds.Contains(kind)
                && !chain.Contains(provider))
            {
                chain.Add(provider);
            }
        }

        foreach (IQuoteProvider fallback in _providers.Where(p => p.IsSimulated && p.SupportedKinds.Contains(kind)))
        {
            if (!chain.Contains(fallback))
            {
                chain.Add(fallback);
            }
        }

        return chain;
    }

    private async Task<Quote> ToTargetCurrencyAsync(Quote quote, string target, CancellationToken cancellationToken)
    {
        if (Normalise(quote.Currency) == target)
        {
            return quote;
        }

        try
        {
            return await fxRateService.ConvertAsync(quote, target, cancellationToken).ConfigureAwait(false);
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            // Without a rate we cannot honestly express this price in the game's currency.
            logger?.LogWarning(ex, "Conversion {From}→{To} impossible pour {Symbol}.", quote.Currency, target, quote.Symbol);
            return quote with { Price = 0m };
        }
    }

    private static IEnumerable<AssetKind> KindsToSearch(AssetKind? kind) =>
        kind is null ? [AssetKind.Crypto, AssetKind.Stock] : [kind.Value];

    private static string Normalise(string currency) =>
        string.IsNullOrWhiteSpace(currency) ? "EUR" : currency.Trim().ToUpperInvariant();

    private static string CacheKey(Asset asset, string currency) => $"quote:{asset.Key}:{currency}";

    private static string CacheKey(Quote quote, string currency) => $"quote:{quote.Key}:{currency}";

    private void RecordSuccess(IQuoteProvider provider) => Record(provider, succeeded: true, error: null);

    private void RecordFailure(IQuoteProvider provider, Exception exception) =>
        Record(provider, succeeded: false, error: exception.Message);

    private void Record(IQuoteProvider provider, bool succeeded, string? error)
    {
        lock (_statusGate)
        {
            _statuses[provider.Id] = new ProviderStatus
            {
                Id = provider.Id,
                DisplayName = provider.DisplayName,
                IsConfigured = provider.IsConfigured,
                IsSimulated = provider.IsSimulated,
                LastCallSucceeded = succeeded,
                LastCallAt = _clock.GetUtcNow(),
                LastError = error,
            };
        }
    }
}
