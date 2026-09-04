using System.Text.Json;
using SafeInvest.Core.Abstractions;
using SafeInvest.Core.Models;
using SafeInvest.MarketData.Internal;

namespace SafeInvest.MarketData.Providers;

/// <summary>
/// CoinGecko. Works with no sign-up at all (the keyless public tier, a handful of calls a
/// minute), and lifts to 100 calls a minute if the user pastes a free Demo key into the
/// settings. Quotes come back directly in the requested currency, so no FX step is needed.
/// </summary>
public sealed class CoinGeckoProvider(HttpClient httpClient, MarketDataOptions options) : IQuoteProvider
{
    private const string BaseUrl = "https://api.coingecko.com/api/v3";

    // The keyless tier is documented at 5-15 calls a minute; stay at the safe end.
    private readonly TokenBucket _bucket = new(capacity: 5, refillWindow: TimeSpan.FromMinutes(1));

    public string Id => "coingecko";

    public string DisplayName => "CoinGecko";

    public IReadOnlySet<AssetKind> SupportedKinds { get; } = new HashSet<AssetKind> { AssetKind.Crypto };

    /// <summary>Always usable: the public endpoints need no key.</summary>
    public bool IsConfigured => true;

    public async Task<IReadOnlyList<Quote>> GetQuotesAsync(
        IReadOnlyCollection<Asset> assets,
        string currency,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(assets);

        Dictionary<string, Asset> byId = [];
        foreach (Asset asset in assets.Where(a => a.Kind == AssetKind.Crypto))
        {
            Asset enriched = AssetCatalog.Enrich(asset);
            string? id = enriched.ProviderId;
            if (!string.IsNullOrWhiteSpace(id))
            {
                byId[id] = enriched;
            }
        }

        if (byId.Count == 0)
        {
            return [];
        }

        Throttle();

        string vs = currency.ToLowerInvariant();
        string url = $"{BaseUrl}/simple/price" +
                     $"?ids={Uri.EscapeDataString(string.Join(',', byId.Keys))}" +
                     $"&vs_currencies={vs}" +
                     "&include_24hr_change=true&include_market_cap=true" +
                     "&include_24hr_vol=true&include_last_updated_at=true";

        using JsonDocument document = await HttpJson
            .GetAsync(httpClient, Id, url, cancellationToken, Headers())
            .ConfigureAwait(false);

        List<Quote> quotes = [];

        foreach ((string id, Asset asset) in byId)
        {
            if (!document.RootElement.TryGetProperty(id, out JsonElement entry)
                || HttpJson.Decimal(entry, vs) is not { } price)
            {
                continue;
            }

            long? updatedAt = entry.TryGetProperty("last_updated_at", out JsonElement stamp)
                && stamp.TryGetInt64(out long seconds)
                    ? seconds
                    : null;

            quotes.Add(new Quote
            {
                Symbol = asset.Symbol,
                Kind = AssetKind.Crypto,
                Name = asset.Name,
                Price = price,
                Currency = currency.ToUpperInvariant(),
                ChangePercent24h = Round(HttpJson.Decimal(entry, $"{vs}_24h_change")),
                MarketCap = HttpJson.Decimal(entry, $"{vs}_market_cap"),
                Volume24h = HttpJson.Decimal(entry, $"{vs}_24h_vol"),
                AsOf = updatedAt is null
                    ? DateTimeOffset.UtcNow
                    : DateTimeOffset.FromUnixTimeSeconds(updatedAt.Value),
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

        Throttle();

        string url = $"{BaseUrl}/search?query={Uri.EscapeDataString(query.Trim())}";
        using JsonDocument document = await HttpJson
            .GetAsync(httpClient, Id, url, cancellationToken, Headers())
            .ConfigureAwait(false);

        if (!document.RootElement.TryGetProperty("coins", out JsonElement coins)
            || coins.ValueKind != JsonValueKind.Array)
        {
            return [];
        }

        List<Asset> results = [];
        foreach (JsonElement coin in coins.EnumerateArray().Take(limit))
        {
            string? symbol = HttpJson.String(coin, "symbol");
            string? name = HttpJson.String(coin, "name");
            string? id = HttpJson.String(coin, "id");

            if (symbol is null || name is null || id is null)
            {
                continue;
            }

            results.Add(new Asset
            {
                Symbol = symbol.ToUpperInvariant(),
                Name = name,
                Kind = AssetKind.Crypto,
                ProviderId = id,
                LogoUrl = HttpJson.String(coin, "thumb"),
            });
        }

        return results;
    }

    public async Task<IReadOnlyList<Candle>> GetHistoryAsync(
        Asset asset,
        string currency,
        HistoryRange range,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(asset);

        string? id = AssetCatalog.Enrich(asset).ProviderId;
        if (asset.Kind != AssetKind.Crypto || string.IsNullOrWhiteSpace(id))
        {
            return [];
        }

        Throttle();

        int days = DaysFor(range);
        string url = $"{BaseUrl}/coins/{Uri.EscapeDataString(id)}/market_chart" +
                     $"?vs_currency={currency.ToLowerInvariant()}&days={days}";

        using JsonDocument document = await HttpJson
            .GetAsync(httpClient, Id, url, cancellationToken, Headers())
            .ConfigureAwait(false);

        if (!document.RootElement.TryGetProperty("prices", out JsonElement prices)
            || prices.ValueKind != JsonValueKind.Array)
        {
            return [];
        }

        List<Candle> candles = [];
        foreach (JsonElement point in prices.EnumerateArray())
        {
            if (point.ValueKind != JsonValueKind.Array || point.GetArrayLength() < 2)
            {
                continue;
            }

            if (point[0].TryGetInt64(out long milliseconds) && point[1].TryGetDecimal(out decimal close))
            {
                candles.Add(new Candle
                {
                    Timestamp = DateTimeOffset.FromUnixTimeMilliseconds(milliseconds),
                    Close = close,
                });
            }
        }

        return candles;
    }

    private void Throttle()
    {
        if (!_bucket.TryTake())
        {
            throw new QuoteProviderException(
                Id,
                $"Limite CoinGecko atteinte, réessayez dans {_bucket.TimeUntilNextToken().TotalSeconds:N0} s.")
            {
                IsRateLimited = true,
            };
        }
    }

    private Dictionary<string, string> Headers()
    {
        Dictionary<string, string> headers = new()
        {
            ["Accept"] = "application/json",
        };

        // A free Demo key raises the limit from ~5 to 100 calls a minute.
        if (options.ApiKeyFor(Id) is { } key)
        {
            headers["x-cg-demo-api-key"] = key;
        }

        return headers;
    }

    private static int DaysFor(HistoryRange range) => range switch
    {
        HistoryRange.Day => 1,
        HistoryRange.Week => 7,
        HistoryRange.Month => 30,
        HistoryRange.Quarter => 90,
        _ => 365,
    };

    private static decimal? Round(decimal? value) =>
        value is null ? null : Math.Round(value.Value, 4, MidpointRounding.AwayFromZero);
}
