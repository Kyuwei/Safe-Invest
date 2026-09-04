using System.Globalization;
using System.Text.RegularExpressions;
using AngleSharp.Html.Dom;
using AngleSharp.Html.Parser;
using SafeInvest.Core.Abstractions;
using SafeInvest.Core.Models;
using SafeInvest.MarketData.Internal;

namespace SafeInvest.MarketData.Providers;

/// <summary>
/// Last-resort source: reads the price straight off a public web page when every API in
/// the chain has failed or run out of quota.
///
/// This is deliberately the least trusted link. Page markup changes without warning, so
/// the recipes below are kept declarative — repairing a broken source means editing one
/// selector, not rewriting the provider. Quotes it returns are real prices, not simulated
/// ones, but they carry this provider's id so the UI can say where the number came from.
/// </summary>
public sealed partial class WebScrapeProvider(HttpClient httpClient) : IQuoteProvider
{
    private static readonly HtmlParser Parser = new();

    private readonly TokenBucket _bucket = new(capacity: 10, refillWindow: TimeSpan.FromMinutes(1));

    /// <summary>Browser-shaped headers; these pages refuse anything that looks like a script.</summary>
    private static readonly Dictionary<string, string> BrowserHeaders = new()
    {
        ["User-Agent"] = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 " +
                         "(KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36",
        ["Accept"] = "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        ["Accept-Language"] = "fr-FR,fr;q=0.9,en;q=0.8",
    };

    /// <summary>
    /// CoinMarketCap publishes coin pages under a slug. It usually matches the CoinGecko
    /// id we already store, and this table covers the handful that differ.
    /// </summary>
    private static readonly Dictionary<string, string> CoinMarketCapSlugs = new(StringComparer.OrdinalIgnoreCase)
    {
        ["BTC"] = "bitcoin",
        ["ETH"] = "ethereum",
        ["SOL"] = "solana",
        ["XRP"] = "xrp",
        ["ADA"] = "cardano",
        ["DOGE"] = "dogecoin",
        ["AVAX"] = "avalanche",
        ["DOT"] = "polkadot",
        ["LINK"] = "chainlink",
        ["MATIC"] = "polygon-ecosystem-token",
        ["LTC"] = "litecoin",
        ["BNB"] = "bnb",
    };

    public string Id => "scraper";

    public string DisplayName => "Repli web (pages publiques)";

    public IReadOnlySet<AssetKind> SupportedKinds { get; } =
        new HashSet<AssetKind> { AssetKind.Crypto, AssetKind.Stock, AssetKind.Etf };

    public bool IsConfigured => true;

    public async Task<IReadOnlyList<Quote>> GetQuotesAsync(
        IReadOnlyCollection<Asset> assets,
        string currency,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(assets);

        List<Quote> quotes = [];

        foreach (Asset asset in assets)
        {
            if (BuildRecipe(asset) is not { } recipe)
            {
                continue;
            }

            Throttle();

            try
            {
                Quote? quote = await ScrapeAsync(asset, recipe, cancellationToken).ConfigureAwait(false);
                if (quote is not null)
                {
                    quotes.Add(quote);
                }
            }
            catch (QuoteProviderException)
            {
                // One unreachable page must not sink the whole batch.
            }
        }

        return quotes;
    }

    private async Task<Quote?> ScrapeAsync(Asset asset, ScrapeRecipe recipe, CancellationToken cancellationToken)
    {
        string html = await HttpJson
            .GetStringAsync(httpClient, Id, recipe.Url, cancellationToken, BrowserHeaders)
            .ConfigureAwait(false);

        using IHtmlDocument document = await Parser
            .ParseDocumentAsync(html, cancellationToken)
            .ConfigureAwait(false);

        decimal? price = null;

        foreach (string selector in recipe.Selectors)
        {
            string? text = document.QuerySelector(selector)?.TextContent;
            if (TryParsePrice(text, out decimal parsed))
            {
                price = parsed;
                break;
            }
        }

        // Some pages hydrate the visible price client-side but still embed it in the
        // server-rendered JSON payload, so try that before giving up.
        price ??= recipe.JsonFallbackPattern is { } pattern ? MatchNumber(html, pattern) : null;

        if (price is null or <= 0m)
        {
            return null;
        }

        return new Quote
        {
            Symbol = asset.Symbol,
            Kind = asset.Kind,
            Name = asset.Name,
            Price = price.Value,
            Currency = recipe.Currency,
            AsOf = DateTimeOffset.UtcNow,
            SourceId = Id,
        };
    }

    /// <summary>Searching is not something a scraper should do; the chain handles it elsewhere.</summary>
    public Task<IReadOnlyList<Asset>> SearchAsync(
        string query,
        AssetKind? kind,
        int limit,
        CancellationToken cancellationToken = default) =>
        Task.FromResult<IReadOnlyList<Asset>>(AssetCatalog.Search(query, kind, limit));

    /// <summary>Scraping a whole price series is neither reliable nor polite.</summary>
    public Task<IReadOnlyList<Candle>> GetHistoryAsync(
        Asset asset,
        string currency,
        HistoryRange range,
        CancellationToken cancellationToken = default) =>
        Task.FromResult<IReadOnlyList<Candle>>([]);

    private static ScrapeRecipe? BuildRecipe(Asset asset)
    {
        string symbol = Asset.Normalize(asset.Symbol);

        if (asset.Kind == AssetKind.Crypto)
        {
            string? slug = CoinMarketCapSlugs.GetValueOrDefault(symbol)
                           ?? AssetCatalog.Enrich(asset).ProviderId;

            return string.IsNullOrWhiteSpace(slug)
                ? null
                : new ScrapeRecipe
                {
                    Url = $"https://coinmarketcap.com/currencies/{Uri.EscapeDataString(slug)}/",
                    Selectors = ["[data-test=\"text-cdp-price-display\"]", "span.sc-65e7f566-0"],
                    JsonFallbackPattern = "\"price\"\\s*:\\s*(?<value>[0-9]+(?:\\.[0-9]+)?)",
                    Currency = "USD",
                };
        }

        // stockanalysis.com renders the quote server-side, unlike most finance portals.
        string section = asset.Kind == AssetKind.Etf ? "etf" : "stocks";
        string ticker = symbol.Split('.')[0].ToLowerInvariant();

        return new ScrapeRecipe
        {
            Url = $"https://stockanalysis.com/{section}/{Uri.EscapeDataString(ticker)}/",
            Selectors = ["div.text-4xl", "[data-test=\"quote-price\"]"],
            JsonFallbackPattern = null,
            Currency = "USD",
        };
    }

    private void Throttle()
    {
        if (!_bucket.TryTake())
        {
            throw new QuoteProviderException(Id, "Repli web volontairement ralenti pour rester poli.")
            {
                IsRateLimited = true,
            };
        }
    }

    internal static bool TryParsePrice(string? text, out decimal price)
    {
        price = 0m;

        if (string.IsNullOrWhiteSpace(text))
        {
            return false;
        }

        // Strip currency symbols, spaces and thousands separators: "$79,778.16" -> 79778.16
        string cleaned = PriceCharacters().Replace(text, string.Empty).Replace(",", string.Empty, StringComparison.Ordinal);

        return decimal.TryParse(cleaned, NumberStyles.Float, CultureInfo.InvariantCulture, out price) && price > 0m;
    }

    private static decimal? MatchNumber(string html, string pattern)
    {
        Match match = Regex.Match(html, pattern, RegexOptions.None, TimeSpan.FromSeconds(2));

        return match.Success
            && decimal.TryParse(
                match.Groups["value"].Value,
                NumberStyles.Float,
                CultureInfo.InvariantCulture,
                out decimal value)
                ? value
                : null;
    }

    [GeneratedRegex(@"[^0-9.,]")]
    private static partial Regex PriceCharacters();

    private sealed record ScrapeRecipe
    {
        public required string Url { get; init; }

        /// <summary>CSS selectors tried in order until one yields a number.</summary>
        public required IReadOnlyList<string> Selectors { get; init; }

        /// <summary>Regex with a "value" group, applied to the raw HTML as a last resort.</summary>
        public required string? JsonFallbackPattern { get; init; }

        public required string Currency { get; init; }
    }
}
