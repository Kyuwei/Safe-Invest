using System.Text.Json;
using SafeInvest.Core.Abstractions;
using SafeInvest.Core.Models;
using SafeInvest.MarketData.Internal;

namespace SafeInvest.MarketData.Providers;

/// <summary>
/// CoinMarketCap's professional API. Needs a free Basic key (15 000 credits a month);
/// without one the provider reports itself unconfigured and the chain skips it.
/// Historical series are a paid feature there, so history falls through to another source.
/// </summary>
public sealed class CoinMarketCapProvider(HttpClient httpClient, MarketDataOptions options) : IQuoteProvider
{
    private const string BaseUrl = "https://pro-api.coinmarketcap.com";

    private readonly TokenBucket _bucket = new(capacity: 25, refillWindow: TimeSpan.FromMinutes(1));

    public string Id => "coinmarketcap";

    public string DisplayName => "CoinMarketCap";

    public IReadOnlySet<AssetKind> SupportedKinds { get; } = new HashSet<AssetKind> { AssetKind.Crypto };

    public bool IsConfigured => options.ApiKeyFor(Id) is not null;

    public async Task<IReadOnlyList<Quote>> GetQuotesAsync(
        IReadOnlyCollection<Asset> assets,
        string currency,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(assets);
        RequireKey();

        Dictionary<string, Asset> bySymbol = assets
            .Where(a => a.Kind == AssetKind.Crypto)
            .GroupBy(a => Asset.Normalize(a.Symbol), StringComparer.Ordinal)
            .ToDictionary(g => g.Key, g => g.First(), StringComparer.Ordinal);

        if (bySymbol.Count == 0)
        {
            return [];
        }

        Throttle();

        string convert = currency.ToUpperInvariant();
        string url = $"{BaseUrl}/v2/cryptocurrency/quotes/latest" +
                     $"?symbol={Uri.EscapeDataString(string.Join(',', bySymbol.Keys))}" +
                     $"&convert={convert}";

        using JsonDocument document = await HttpJson
            .GetAsync(httpClient, Id, url, cancellationToken, Headers())
            .ConfigureAwait(false);

        if (!document.RootElement.TryGetProperty("data", out JsonElement data))
        {
            return [];
        }

        List<Quote> quotes = [];

        foreach ((string symbol, Asset asset) in bySymbol)
        {
            if (!data.TryGetProperty(symbol, out JsonElement entry))
            {
                continue;
            }

            // v2 returns an array per symbol (several coins can share a ticker); v1 an object.
            JsonElement coin = entry.ValueKind == JsonValueKind.Array
                ? entry.GetArrayLength() > 0 ? entry[0] : default
                : entry;

            if (coin.ValueKind != JsonValueKind.Object
                || !coin.TryGetProperty("quote", out JsonElement quoteNode)
                || !quoteNode.TryGetProperty(convert, out JsonElement converted)
                || HttpJson.Decimal(converted, "price") is not { } price)
            {
                continue;
            }

            quotes.Add(new Quote
            {
                Symbol = asset.Symbol,
                Kind = AssetKind.Crypto,
                Name = HttpJson.String(coin, "name") ?? asset.Name,
                Price = price,
                Currency = convert,
                ChangePercent24h = HttpJson.Decimal(converted, "percent_change_24h"),
                MarketCap = HttpJson.Decimal(converted, "market_cap"),
                Volume24h = HttpJson.Decimal(converted, "volume_24h"),
                AsOf = ParseTimestamp(HttpJson.String(converted, "last_updated")),
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
        if (kind is not null and not AssetKind.Crypto || string.IsNullOrWhiteSpace(query))
        {
            return [];
        }

        RequireKey();
        Throttle();

        // The map endpoint costs zero credits, so searching is free.
        string url = $"{BaseUrl}/v1/cryptocurrency/map" +
                     $"?listing_status=active&sort=cmc_rank&limit={Math.Clamp(limit * 10, 20, 500)}";

        using JsonDocument document = await HttpJson
            .GetAsync(httpClient, Id, url, cancellationToken, Headers())
            .ConfigureAwait(false);

        if (!document.RootElement.TryGetProperty("data", out JsonElement data)
            || data.ValueKind != JsonValueKind.Array)
        {
            return [];
        }

        string needle = query.Trim();
        List<Asset> results = [];

        foreach (JsonElement coin in data.EnumerateArray())
        {
            string? symbol = HttpJson.String(coin, "symbol");
            string? name = HttpJson.String(coin, "name");

            if (symbol is null || name is null)
            {
                continue;
            }

            if (!symbol.Contains(needle, StringComparison.OrdinalIgnoreCase)
                && !name.Contains(needle, StringComparison.OrdinalIgnoreCase))
            {
                continue;
            }

            results.Add(new Asset
            {
                Symbol = symbol.ToUpperInvariant(),
                Name = name,
                Kind = AssetKind.Crypto,
                ProviderId = HttpJson.String(coin, "slug"),
            });

            if (results.Count >= limit)
            {
                break;
            }
        }

        return results;
    }

    /// <summary>Historical data is behind a paid CoinMarketCap plan, so the chain moves on.</summary>
    public Task<IReadOnlyList<Candle>> GetHistoryAsync(
        Asset asset,
        string currency,
        HistoryRange range,
        CancellationToken cancellationToken = default) =>
        Task.FromResult<IReadOnlyList<Candle>>([]);

    private void RequireKey()
    {
        if (!IsConfigured)
        {
            throw new QuoteProviderException(
                Id, "Aucune clé CoinMarketCap enregistrée : renseignez-la dans les Réglages.");
        }
    }

    private void Throttle()
    {
        if (!_bucket.TryTake())
        {
            throw new QuoteProviderException(Id, "Limite d'appels CoinMarketCap atteinte.")
            {
                IsRateLimited = true,
            };
        }
    }

    private Dictionary<string, string> Headers() => new()
    {
        ["X-CMC_PRO_API_KEY"] = options.ApiKeyFor(Id) ?? string.Empty,
        ["Accept"] = "application/json",
    };

    private static DateTimeOffset ParseTimestamp(string? value) =>
        DateTimeOffset.TryParse(
            value,
            System.Globalization.CultureInfo.InvariantCulture,
            System.Globalization.DateTimeStyles.AdjustToUniversal,
            out DateTimeOffset parsed)
            ? parsed
            : DateTimeOffset.UtcNow;
}
