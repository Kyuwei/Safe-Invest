using System.Text.Json;
using SafeInvest.Core.Abstractions;
using SafeInvest.Core.Models;
using SafeInvest.MarketData.Internal;

namespace SafeInvest.MarketData.Providers;

/// <summary>
/// Finnhub. Free tier allows 60 calls a minute on US equities with roughly a 15-minute
/// delay. Candles moved behind the paid plan, so history falls through to Yahoo.
/// </summary>
public sealed class FinnhubProvider(HttpClient httpClient, MarketDataOptions options) : IQuoteProvider
{
    private const string BaseUrl = "https://finnhub.io/api/v1";

    private readonly TokenBucket _bucket = new(capacity: 55, refillWindow: TimeSpan.FromMinutes(1));

    public string Id => "finnhub";

    public string DisplayName => "Finnhub";

    public IReadOnlySet<AssetKind> SupportedKinds { get; } =
        new HashSet<AssetKind> { AssetKind.Stock, AssetKind.Etf };

    public bool IsConfigured => options.ApiKeyFor(Id) is not null;

    public async Task<IReadOnlyList<Quote>> GetQuotesAsync(
        IReadOnlyCollection<Asset> assets,
        string currency,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(assets);
        RequireKey();

        List<Quote> quotes = [];

        foreach (Asset asset in assets.Where(a => SupportedKinds.Contains(a.Kind)))
        {
            Throttle();

            string url = $"{BaseUrl}/quote?symbol={Uri.EscapeDataString(asset.Symbol)}&token={Key()}";
            using JsonDocument document = await HttpJson
                .GetAsync(httpClient, Id, url, cancellationToken)
                .ConfigureAwait(false);

            // "c" is the current price; Finnhub answers 0 for symbols it does not cover.
            if (HttpJson.Decimal(document.RootElement, "c") is not { } price || price <= 0m)
            {
                continue;
            }

            long? stamp = document.RootElement.TryGetProperty("t", out JsonElement time)
                && time.TryGetInt64(out long seconds) && seconds > 0
                    ? seconds
                    : null;

            quotes.Add(new Quote
            {
                Symbol = asset.Symbol,
                Kind = asset.Kind,
                Name = asset.Name,
                Price = price,
                // Finnhub reports in the instrument's listing currency, US dollars in practice.
                Currency = "USD",
                Change24h = HttpJson.Decimal(document.RootElement, "d"),
                ChangePercent24h = HttpJson.Decimal(document.RootElement, "dp"),
                AsOf = stamp is null ? DateTimeOffset.UtcNow : DateTimeOffset.FromUnixTimeSeconds(stamp.Value),
                SourceId = Id,
            });
        }

        return quotes;
    }

    public async Task<IReadOnlyList<Asset>> SearchAsync(
        string query,
        AssetKind? kind,
        int limit,
        CancellationToken cancellationToken = default)
    {
        if (kind == AssetKind.Crypto || string.IsNullOrWhiteSpace(query))
        {
            return [];
        }

        RequireKey();
        Throttle();

        string url = $"{BaseUrl}/search?q={Uri.EscapeDataString(query.Trim())}&token={Key()}";
        using JsonDocument document = await HttpJson
            .GetAsync(httpClient, Id, url, cancellationToken)
            .ConfigureAwait(false);

        if (!document.RootElement.TryGetProperty("result", out JsonElement rows)
            || rows.ValueKind != JsonValueKind.Array)
        {
            return [];
        }

        List<Asset> results = [];

        foreach (JsonElement row in rows.EnumerateArray())
        {
            string? symbol = HttpJson.String(row, "symbol");
            if (symbol is null)
            {
                continue;
            }

            AssetKind resolved = HttpJson.String(row, "type") switch
            {
                "ETP" or "ETF" => AssetKind.Etf,
                _ => AssetKind.Stock,
            };

            if (kind is not null && resolved != kind.Value)
            {
                continue;
            }

            results.Add(new Asset
            {
                Symbol = symbol.ToUpperInvariant(),
                Name = HttpJson.String(row, "description") ?? symbol,
                Kind = resolved,
            });

            if (results.Count >= limit)
            {
                break;
            }
        }

        return results;
    }

    /// <summary>Candles are a paid Finnhub feature; the chain falls through for history.</summary>
    public Task<IReadOnlyList<Candle>> GetHistoryAsync(
        Asset asset,
        string currency,
        HistoryRange range,
        CancellationToken cancellationToken = default) =>
        Task.FromResult<IReadOnlyList<Candle>>([]);

    private string Key() => options.ApiKeyFor(Id) ?? string.Empty;

    private void RequireKey()
    {
        if (!IsConfigured)
        {
            throw new QuoteProviderException(
                Id, "Aucune clé Finnhub enregistrée : renseignez-la dans les Réglages.");
        }
    }

    private void Throttle()
    {
        if (!_bucket.TryTake())
        {
            throw new QuoteProviderException(Id, "Limite d'appels Finnhub atteinte.")
            {
                IsRateLimited = true,
            };
        }
    }
}
