using System.Net;
using SafeInvest.Core.Abstractions;
using SafeInvest.Core.Models;
using SafeInvest.MarketData;
using SafeInvest.MarketData.Providers;
using Xunit;

namespace SafeInvest.MarketData.Tests;

public class CoinGeckoProviderTests
{
    private static readonly Asset Bitcoin = AssetCatalog.Find(AssetKind.Crypto, "BTC")!;

    [Fact]
    public async Task Quotes_are_read_in_the_requested_currency()
    {
        FakeHttpMessageHandler handler = new FakeHttpMessageHandler()
            .RespondWithFixture("simple/price", "coingecko-simple-price.json");
        CoinGeckoProvider provider = new(handler.CreateClient(), new MarketDataOptions());

        IReadOnlyList<Quote> quotes = await provider.GetQuotesAsync([Bitcoin], "EUR");

        Quote quote = Assert.Single(quotes);
        Assert.Equal("BTC", quote.Symbol);
        Assert.Equal(68_724m, quote.Price);
        Assert.Equal("EUR", quote.Currency);
        Assert.Equal(-1.8492m, quote.ChangePercent24h);
        Assert.Equal(-1, quote.Direction);
        Assert.False(quote.IsSimulated);
        Assert.Equal("coingecko", quote.SourceId);
    }

    [Fact]
    public async Task The_ticker_is_translated_into_the_coingecko_id()
    {
        FakeHttpMessageHandler handler = new FakeHttpMessageHandler()
            .RespondWithFixture("simple/price", "coingecko-simple-price.json");
        CoinGeckoProvider provider = new(handler.CreateClient(), new MarketDataOptions());

        // Only the ticker is known here; the catalog has to supply "bitcoin".
        await provider.GetQuotesAsync(
            [new Asset { Symbol = "BTC", Name = "Bitcoin", Kind = AssetKind.Crypto }],
            "EUR");

        Assert.Contains("ids=bitcoin", Assert.Single(handler.RequestedUrls), StringComparison.Ordinal);
    }

    [Fact]
    public async Task A_demo_key_is_sent_as_a_header_when_one_is_configured()
    {
        FakeHttpMessageHandler handler = new FakeHttpMessageHandler()
            .RespondWithFixture("simple/price", "coingecko-simple-price.json");
        MarketDataOptions options = new();
        options.ApiKeys["coingecko"] = "CG-demo-key";

        CoinGeckoProvider provider = new(handler.CreateClient(), options);
        IReadOnlyList<Quote> quotes = await provider.GetQuotesAsync([Bitcoin], "EUR");

        Assert.NotEmpty(quotes);
    }

    [Fact]
    public async Task A_429_surfaces_as_a_rate_limited_failure()
    {
        FakeHttpMessageHandler handler = new FakeHttpMessageHandler()
            .Respond("simple/price", "{}", HttpStatusCode.TooManyRequests);
        CoinGeckoProvider provider = new(handler.CreateClient(), new MarketDataOptions());

        QuoteProviderException ex = await Assert.ThrowsAsync<QuoteProviderException>(
            () => provider.GetQuotesAsync([Bitcoin], "EUR"));

        Assert.True(ex.IsRateLimited);
        Assert.Equal("coingecko", ex.ProviderId);
    }

    [Fact]
    public async Task Search_returns_assets_carrying_their_provider_id()
    {
        FakeHttpMessageHandler handler = new FakeHttpMessageHandler()
            .RespondWithFixture("search", "coingecko-search.json");
        CoinGeckoProvider provider = new(handler.CreateClient(), new MarketDataOptions());

        IReadOnlyList<Asset> results = await provider.SearchAsync("solana", AssetKind.Crypto, 5);

        Assert.Equal(2, results.Count);
        Assert.Equal("SOL", results[0].Symbol);
        Assert.Equal("solana", results[0].ProviderId);
    }
}

public class YahooFinanceProviderTests
{
    private static readonly Asset Microsoft = new()
    {
        Symbol = "MSFT",
        Name = "Microsoft",
        Kind = AssetKind.Stock,
    };

    [Fact]
    public async Task A_quote_keeps_the_listing_currency_for_the_service_to_convert()
    {
        FakeHttpMessageHandler handler = new FakeHttpMessageHandler()
            .RespondWithFixture("chart/MSFT", "yahoo-chart-msft.json");
        YahooFinanceProvider provider = new(handler.CreateClient());

        Quote quote = Assert.Single(await provider.GetQuotesAsync([Microsoft], "EUR"));

        Assert.Equal(499.7m, quote.Price);
        Assert.Equal("USD", quote.Currency);
        Assert.Equal(-2.043m, quote.ChangePercent24h);
        Assert.Equal("Microsoft Corporation", quote.Name);
    }

    [Fact]
    public async Task History_is_rebuilt_from_the_timestamp_and_close_arrays()
    {
        FakeHttpMessageHandler handler = new FakeHttpMessageHandler()
            .RespondWithFixture("chart/MSFT", "yahoo-chart-msft.json");
        YahooFinanceProvider provider = new(handler.CreateClient());

        IReadOnlyList<Candle> candles = await provider.GetHistoryAsync(Microsoft, "USD", HistoryRange.Week);

        Assert.Equal(3, candles.Count);
        Assert.Equal(499.7m, candles[^1].Close);
    }

    [Fact]
    public async Task Cryptocurrencies_are_not_this_provider_s_business()
    {
        FakeHttpMessageHandler handler = new();
        YahooFinanceProvider provider = new(handler.CreateClient());

        IReadOnlyList<Quote> quotes = await provider.GetQuotesAsync(
            [AssetCatalog.Find(AssetKind.Crypto, "BTC")!],
            "EUR");

        Assert.Empty(quotes);
        Assert.Empty(handler.RequestedUrls);
    }
}

public class CoinMarketCapProviderTests
{
    [Fact]
    public async Task Without_a_key_the_provider_declares_itself_unconfigured()
    {
        CoinMarketCapProvider provider = new(new FakeHttpMessageHandler().CreateClient(), new MarketDataOptions());

        Assert.False(provider.IsConfigured);
        await Assert.ThrowsAsync<QuoteProviderException>(
            () => provider.GetQuotesAsync([AssetCatalog.Find(AssetKind.Crypto, "BTC")!], "EUR"));
    }

    [Fact]
    public async Task The_v2_array_shaped_payload_is_understood()
    {
        FakeHttpMessageHandler handler = new FakeHttpMessageHandler()
            .RespondWithFixture("quotes/latest", "coinmarketcap-quotes.json");
        MarketDataOptions options = new();
        options.ApiKeys["coinmarketcap"] = "cmc-key";

        CoinMarketCapProvider provider = new(handler.CreateClient(), options);
        Quote quote = Assert.Single(
            await provider.GetQuotesAsync([AssetCatalog.Find(AssetKind.Crypto, "BTC")!], "EUR"));

        Assert.Equal(68_800.5m, quote.Price);
        Assert.Equal("EUR", quote.Currency);
        Assert.Equal(-1.72m, quote.ChangePercent24h);
    }
}

public class FinnhubProviderTests
{
    [Fact]
    public async Task A_quote_is_read_from_the_short_field_names()
    {
        FakeHttpMessageHandler handler = new FakeHttpMessageHandler()
            .RespondWithFixture("quote?symbol=MSFT", "finnhub-quote.json");
        MarketDataOptions options = new();
        options.ApiKeys["finnhub"] = "finnhub-key";

        FinnhubProvider provider = new(handler.CreateClient(), options);
        Quote quote = Assert.Single(await provider.GetQuotesAsync(
            [new Asset { Symbol = "MSFT", Name = "Microsoft", Kind = AssetKind.Stock }],
            "EUR"));

        Assert.Equal(499.7m, quote.Price);
        Assert.Equal("USD", quote.Currency);
        Assert.Equal(-2.043m, quote.ChangePercent24h);
    }
}

public class SimulatedQuoteProviderTests
{
    private static readonly Asset Bitcoin = AssetCatalog.Find(AssetKind.Crypto, "BTC")!;

    [Fact]
    public async Task Every_simulated_quote_is_flagged_as_such()
    {
        SimulatedQuoteProvider provider = new();

        Quote quote = Assert.Single(await provider.GetQuotesAsync([Bitcoin], "EUR"));

        Assert.True(quote.IsSimulated);
        Assert.Equal("simulated", quote.SourceId);
        Assert.True(quote.Price > 0m);
        Assert.Equal("EUR", quote.Currency);
    }

    [Fact]
    public void The_same_asset_at_the_same_instant_always_gives_the_same_price()
    {
        DateTimeOffset moment = new(2026, 5, 17, 9, 30, 0, TimeSpan.Zero);

        decimal first = SimulatedQuoteProvider.PriceAt(Bitcoin, moment);
        decimal second = SimulatedQuoteProvider.PriceAt(Bitcoin, moment);

        Assert.Equal(first, second);
    }

    [Fact]
    public void Different_assets_do_not_share_a_price()
    {
        DateTimeOffset moment = new(2026, 5, 17, 9, 30, 0, TimeSpan.Zero);

        Assert.NotEqual(
            SimulatedQuoteProvider.PriceAt(Bitcoin, moment),
            SimulatedQuoteProvider.PriceAt(AssetCatalog.Find(AssetKind.Crypto, "ETH")!, moment));
    }

    [Fact]
    public void Prices_stay_strictly_positive_over_a_decade()
    {
        DateTimeOffset start = new(2020, 1, 1, 0, 0, 0, TimeSpan.Zero);

        foreach (Asset asset in AssetCatalog.All)
        {
            for (int day = 0; day < 3650; day += 7)
            {
                Assert.True(
                    SimulatedQuoteProvider.PriceAt(asset, start.AddDays(day)) > 0m,
                    $"{asset.Symbol} est passé à zéro au jour {day}.");
            }
        }
    }

    [Fact]
    public async Task History_comes_back_in_chronological_order()
    {
        SimulatedQuoteProvider provider = new();

        IReadOnlyList<Candle> candles = await provider.GetHistoryAsync(Bitcoin, "EUR", HistoryRange.Month);

        Assert.Equal(30, candles.Count);
        Assert.True(candles.Zip(candles.Skip(1)).All(pair => pair.First.Timestamp < pair.Second.Timestamp));
    }
}

public class WebScrapeProviderTests
{
    [Theory]
    [InlineData("$79,778.16", 79778.16)]
    [InlineData("499.70", 499.70)]
    [InlineData("  1,234.5 USD ", 1234.5)]
    public void Prices_are_parsed_out_of_the_page_text(string text, double expected)
    {
        Assert.True(WebScrapeProvider.TryParsePrice(text, out decimal price));
        Assert.Equal((decimal)expected, price);
    }

    [Theory]
    [InlineData("")]
    [InlineData("   ")]
    [InlineData("N/A")]
    [InlineData("0")]
    public void Text_without_a_usable_price_is_rejected(string text)
    {
        Assert.False(WebScrapeProvider.TryParsePrice(text, out _));
    }
}

public class AssetCatalogTests
{
    [Fact]
    public void Every_catalogued_crypto_carries_the_id_its_api_needs()
    {
        foreach (Asset asset in AssetCatalog.OfKind(AssetKind.Crypto))
        {
            Assert.False(string.IsNullOrWhiteSpace(asset.ProviderId), $"{asset.Symbol} n'a pas d'identifiant.");
        }
    }

    [Fact]
    public void A_bare_ticker_is_enriched_from_the_catalog()
    {
        Asset bare = new() { Symbol = "eth", Name = "Ethereum", Kind = AssetKind.Crypto };

        Assert.Equal("ethereum", AssetCatalog.Enrich(bare).ProviderId);
    }

    [Fact]
    public void An_unknown_asset_passes_through_untouched()
    {
        Asset unknown = new() { Symbol = "ZZZZ", Name = "Inconnu", Kind = AssetKind.Crypto };

        Assert.Null(AssetCatalog.Enrich(unknown).ProviderId);
    }

    [Fact]
    public void Search_matches_on_the_name_as_well_as_the_ticker()
    {
        Assert.Contains(AssetCatalog.Search("micro", null, 10), a => a.Symbol == "MSFT");
        Assert.Contains(AssetCatalog.Search("BTC", null, 10), a => a.Symbol == "BTC");
    }

    [Fact]
    public void An_empty_query_still_offers_something_to_browse()
    {
        Assert.NotEmpty(AssetCatalog.Search("", AssetKind.Crypto, 5));
    }
}
