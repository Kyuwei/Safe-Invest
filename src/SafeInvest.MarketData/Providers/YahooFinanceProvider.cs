using System.Text.Json;
using SafeInvest.Core.Abstractions;
using SafeInvest.Core.Models;
using SafeInvest.MarketData.Fx;
using SafeInvest.MarketData.Internal;

namespace SafeInvest.MarketData.Providers;

/// <summary>
/// Yahoo Finance's chart endpoint. No key and no cookie dance (unlike the v7 quote
/// endpoint), which makes it the default share and ETF source. It is undocumented, so the
/// chain is expected to fall through to Finnhub or the web fallback when it changes.
/// Prices come back in the listing currency; MarketDataService converts them.
/// </summary>
public sealed class YahooFinanceProvider(HttpClient httpClient) : IQuoteProvider
{
    private const string ChartUrl = "https://query1.finance.yahoo.com/v8/finance/chart";
    private const string SearchUrl = "https://query1.finance.yahoo.com/v1/finance/search";

    private readonly TokenBucket _bucket = new(capacity: 30, refillWindow: TimeSpan.FromMinutes(1));

    public string Id => "yahoo";

    public string DisplayName => "Yahoo Finance";

    public IReadOnlySet<AssetKind> SupportedKinds { get; } =
        new HashSet<AssetKind> { AssetKind.Stock, AssetKind.Etf };

    public bool IsConfigured => true;

    public async Task<IReadOnlyList<Quote>> GetQuotesAsync(
        IReadOnlyCollection<Asset> assets,
        string currency,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(assets);

        List<Quote> quotes = [];

        // The chart endpoint takes one symbol at a time, so a portfolio of ten shares is
        // ten calls. The service caches aggressively above us to keep that bearable.
        foreach (Asset asset in assets.Where(a => SupportedKinds.Contains(a.Kind)))
        {
            Quote? quote = await FetchOneAsync(asset, cancellationToken).ConfigureAwait(false);
            if (quote is not null)
            {
                quotes.Add(quote);
            }
        }

        return quotes;
    }

    private async Task<Quote?> FetchOneAsync(Asset asset, CancellationToken cancellationToken)
    {
        Throttle();

        string url = $"{ChartUrl}/{Uri.EscapeDataString(asset.Symbol)}?range=1d&interval=1d";
        using JsonDocument document = await HttpJson
            .GetAsync(httpClient, Id, url, cancellationToken, YahooFinanceHeaders.Default)
            .ConfigureAwait(false);

        if (!TryGetMeta(document, out JsonElement meta)
            || HttpJson.Decimal(meta, "regularMarketPrice") is not { } price)
        {
            return null;
        }

        decimal? changePercent = HttpJson.Decimal(meta, "regularMarketChangePercent");
        decimal? previousClose = HttpJson.Decimal(meta, "chartPreviousClose")
                                 ?? HttpJson.Decimal(meta, "previousClose");

        // Older payloads omit the percentage but always carry the previous close.
        changePercent ??= previousClose is > 0m
            ? Math.Round((price - previousClose.Value) / previousClose.Value * 100m, 4, MidpointRounding.AwayFromZero)
            : null;

        long? marketTime = meta.TryGetProperty("regularMarketTime", out JsonElement time)
            && time.TryGetInt64(out long seconds)
                ? seconds
                : null;

        return new Quote
        {
            Symbol = asset.Symbol,
            Kind = asset.Kind,
            Name = HttpJson.String(meta, "longName") ?? HttpJson.String(meta, "shortName") ?? asset.Name,
            Price = price,
            Currency = (HttpJson.String(meta, "currency") ?? "USD").ToUpperInvariant(),
            ChangePercent24h = changePercent,
            Change24h = previousClose is null ? null : price - previousClose.Value,
            Volume24h = HttpJson.Decimal(meta, "regularMarketVolume"),
            AsOf = marketTime is null
                ? DateTimeOffset.UtcNow
                : DateTimeOffset.FromUnixTimeSeconds(marketTime.Value),
            SourceId = Id,
        };
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

        Throttle();

        string url = $"{SearchUrl}?q={Uri.EscapeDataString(query.Trim())}&quotesCount={limit}&newsCount=0";
        using JsonDocument document = await HttpJson
            .GetAsync(httpClient, Id, url, cancellationToken, YahooFinanceHeaders.Default)
            .ConfigureAwait(false);

        if (!document.RootElement.TryGetProperty("quotes", out JsonElement rows)
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

            AssetKind resolved = HttpJson.String(row, "quoteType") switch
            {
                "ETF" => AssetKind.Etf,
                "CRYPTOCURRENCY" => AssetKind.Crypto,
                _ => AssetKind.Stock,
            };

            if (resolved == AssetKind.Crypto || (kind is not null && resolved != kind.Value))
            {
                continue;
            }

            results.Add(new Asset
            {
                Symbol = symbol.ToUpperInvariant(),
                Name = HttpJson.String(row, "longname") ?? HttpJson.String(row, "shortname") ?? symbol,
                Kind = resolved,
            });

            if (results.Count >= limit)
            {
                break;
            }
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

        if (!SupportedKinds.Contains(asset.Kind))
        {
            return [];
        }

        Throttle();

        (string window, string interval) = WindowFor(range);
        string url = $"{ChartUrl}/{Uri.EscapeDataString(asset.Symbol)}?range={window}&interval={interval}";

        using JsonDocument document = await HttpJson
            .GetAsync(httpClient, Id, url, cancellationToken, YahooFinanceHeaders.Default)
            .ConfigureAwait(false);

        if (!TryGetResult(document, out JsonElement result)
            || !result.TryGetProperty("timestamp", out JsonElement stamps)
            || stamps.ValueKind != JsonValueKind.Array
            || !result.TryGetProperty("indicators", out JsonElement indicators)
            || !indicators.TryGetProperty("quote", out JsonElement quoteArray)
            || quoteArray.ValueKind != JsonValueKind.Array
            || quoteArray.GetArrayLength() == 0
            || !quoteArray[0].TryGetProperty("close", out JsonElement closes)
            || closes.ValueKind != JsonValueKind.Array)
        {
            return [];
        }

        List<Candle> candles = [];
        int count = Math.Min(stamps.GetArrayLength(), closes.GetArrayLength());

        for (int i = 0; i < count; i++)
        {
            if (closes[i].ValueKind != JsonValueKind.Number
                || !closes[i].TryGetDecimal(out decimal close)
                || !stamps[i].TryGetInt64(out long seconds))
            {
                continue;
            }

            candles.Add(new Candle
            {
                Timestamp = DateTimeOffset.FromUnixTimeSeconds(seconds),
                Close = close,
            });
        }

        return candles;
    }

    private void Throttle()
    {
        if (!_bucket.TryTake())
        {
            throw new QuoteProviderException(Id, "Trop d'appels vers Yahoo Finance d'un coup.")
            {
                IsRateLimited = true,
            };
        }
    }

    private static bool TryGetResult(JsonDocument document, out JsonElement result)
    {
        result = default;

        if (!document.RootElement.TryGetProperty("chart", out JsonElement chart)
            || !chart.TryGetProperty("result", out JsonElement results)
            || results.ValueKind != JsonValueKind.Array
            || results.GetArrayLength() == 0)
        {
            return false;
        }

        result = results[0];
        return true;
    }

    private static bool TryGetMeta(JsonDocument document, out JsonElement meta)
    {
        meta = default;
        return TryGetResult(document, out JsonElement result)
            && result.TryGetProperty("meta", out meta);
    }

    private static (string Range, string Interval) WindowFor(HistoryRange range) => range switch
    {
        HistoryRange.Day => ("1d", "5m"),
        HistoryRange.Week => ("5d", "1h"),
        HistoryRange.Month => ("1mo", "1d"),
        HistoryRange.Quarter => ("3mo", "1d"),
        _ => ("1y", "1wk"),
    };
}
