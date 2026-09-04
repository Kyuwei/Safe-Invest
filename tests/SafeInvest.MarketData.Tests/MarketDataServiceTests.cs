using System.Net;
using Microsoft.Extensions.Caching.Memory;
using SafeInvest.Core.Abstractions;
using SafeInvest.Core.Models;
using SafeInvest.MarketData;
using SafeInvest.MarketData.Fx;
using SafeInvest.MarketData.Providers;
using Xunit;

namespace SafeInvest.MarketData.Tests;

public class MarketDataServiceTests
{
    private static readonly Asset Bitcoin = AssetCatalog.Find(AssetKind.Crypto, "BTC")!;

    private static readonly Asset Microsoft = new()
    {
        Symbol = "MSFT",
        Name = "Microsoft",
        Kind = AssetKind.Stock,
    };

    private static MemoryCache NewCache() => new(new MemoryCacheOptions());

    private static MarketDataService Build(
        FakeHttpMessageHandler handler,
        MarketDataOptions options,
        params IQuoteProvider[] extraProviders)
    {
        MemoryCache cache = NewCache();
        FxRateService fx = new(handler.CreateClient(), cache, options);

        List<IQuoteProvider> providers =
        [
            new CoinGeckoProvider(handler.CreateClient(), options),
            new CoinMarketCapProvider(handler.CreateClient(), options),
            new YahooFinanceProvider(handler.CreateClient()),
            new FinnhubProvider(handler.CreateClient(), options),
            new WebScrapeProvider(handler.CreateClient()),
            .. extraProviders,
            new SimulatedQuoteProvider(),
        ];

        return new MarketDataService(providers, fx, cache, options);
    }

    [Fact]
    public async Task The_first_working_source_in_the_chain_wins()
    {
        FakeHttpMessageHandler handler = new FakeHttpMessageHandler()
            .RespondWithFixture("simple/price", "coingecko-simple-price.json");

        MarketDataService service = Build(handler, new MarketDataOptions());
        IReadOnlyDictionary<string, Quote> quotes = await service.GetQuotesAsync([Bitcoin], "EUR");

        Quote quote = quotes[Bitcoin.Key];
        Assert.Equal("coingecko", quote.SourceId);
        Assert.False(quote.IsSimulated);
        Assert.Equal(68_724m, quote.Price);
    }

    [Fact]
    public async Task A_saturated_quota_falls_through_to_the_next_source()
    {
        // CoinGecko is out of quota, CoinMarketCap has a key and answers.
        FakeHttpMessageHandler handler = new FakeHttpMessageHandler()
            .Respond("simple/price", "{}", HttpStatusCode.TooManyRequests)
            .RespondWithFixture("quotes/latest", "coinmarketcap-quotes.json");

        MarketDataOptions options = new();
        options.ApiKeys["coinmarketcap"] = "cmc-key";

        MarketDataService service = Build(handler, options);
        IReadOnlyDictionary<string, Quote> quotes = await service.GetQuotesAsync([Bitcoin], "EUR");

        Assert.Equal("coinmarketcap", quotes[Bitcoin.Key].SourceId);
    }

    [Fact]
    public async Task When_every_real_source_fails_the_simulator_keeps_the_app_alive()
    {
        // Nothing is routed: every HTTP call 404s, and no API key is configured.
        FakeHttpMessageHandler handler = new();

        MarketDataService service = Build(handler, new MarketDataOptions());
        IReadOnlyDictionary<string, Quote> quotes = await service.GetQuotesAsync([Bitcoin], "EUR");

        Quote quote = quotes[Bitcoin.Key];
        Assert.True(quote.IsSimulated);
        Assert.Equal("simulated", quote.SourceId);
        Assert.True(quote.Price > 0m);
    }

    [Fact]
    public async Task A_dollar_quote_is_converted_into_the_currency_of_the_game()
    {
        FakeHttpMessageHandler handler = new FakeHttpMessageHandler()
            .RespondWithFixture("chart/MSFT", "yahoo-chart-msft.json")
            .RespondWithFixture("frankfurter", "frankfurter-usd-eur.json");

        MarketDataService service = Build(handler, new MarketDataOptions());
        IReadOnlyDictionary<string, Quote> quotes = await service.GetQuotesAsync([Microsoft], "EUR");

        Quote quote = quotes[Microsoft.Key];
        Assert.Equal("EUR", quote.Currency);
        Assert.Equal(499.7m * 0.86044m, quote.Price);
        // The percentage move is currency-independent and must survive untouched.
        Assert.Equal(-2.043m, quote.ChangePercent24h);
    }

    [Fact]
    public async Task Forcing_the_demo_mode_bypasses_every_network_source()
    {
        FakeHttpMessageHandler handler = new FakeHttpMessageHandler()
            .RespondWithFixture("simple/price", "coingecko-simple-price.json");

        MarketDataService service = Build(handler, new MarketDataOptions { ForceSimulated = true });
        IReadOnlyDictionary<string, Quote> quotes = await service.GetQuotesAsync([Bitcoin], "EUR");

        Assert.True(quotes[Bitcoin.Key].IsSimulated);
        Assert.Empty(handler.RequestedUrls);
    }

    [Fact]
    public async Task A_second_request_inside_the_cache_window_makes_no_network_call()
    {
        FakeHttpMessageHandler handler = new FakeHttpMessageHandler()
            .RespondWithFixture("simple/price", "coingecko-simple-price.json");

        MarketDataService service = Build(handler, new MarketDataOptions());

        await service.GetQuotesAsync([Bitcoin], "EUR");
        int afterFirst = handler.RequestedUrls.Count;
        await service.GetQuotesAsync([Bitcoin], "EUR");

        Assert.Equal(afterFirst, handler.RequestedUrls.Count);
    }

    [Fact]
    public async Task An_unpriceable_asset_is_simply_missing_from_the_result()
    {
        FakeHttpMessageHandler handler = new FakeHttpMessageHandler()
            .RespondWithFixture("simple/price", "coingecko-simple-price.json");

        // A provider that answers nothing at all, standing in for a dead source.
        MarketDataService service = new(
            [new SilentProvider()],
            new FxRateService(handler.CreateClient(), NewCache(), new MarketDataOptions()),
            NewCache(),
            new MarketDataOptions { CryptoProviderOrder = ["silent"] });

        IReadOnlyDictionary<string, Quote> quotes = await service.GetQuotesAsync([Bitcoin], "EUR");

        Assert.Empty(quotes);
    }

    [Fact]
    public async Task Provider_health_is_recorded_for_the_settings_screen()
    {
        FakeHttpMessageHandler handler = new FakeHttpMessageHandler()
            .Respond("simple/price", "{}", HttpStatusCode.TooManyRequests);

        MarketDataService service = Build(handler, new MarketDataOptions());
        await service.GetQuotesAsync([Bitcoin], "EUR");

        ProviderStatus coinGecko = service.GetProviderStatuses().Single(s => s.Id == "coingecko");
        Assert.False(coinGecko.LastCallSucceeded);
        Assert.Contains("429", coinGecko.LastError, StringComparison.Ordinal);

        ProviderStatus simulator = service.GetProviderStatuses().Single(s => s.Id == "simulated");
        Assert.True(simulator.IsSimulated);
        Assert.True(simulator.LastCallSucceeded);
    }

    [Fact]
    public async Task Search_always_offers_the_catalog_even_with_no_network()
    {
        FakeHttpMessageHandler handler = new();

        MarketDataService service = Build(handler, new MarketDataOptions());
        IReadOnlyList<Asset> results = await service.SearchAsync("bitcoin", AssetKind.Crypto, 5);

        Assert.Contains(results, a => a.Symbol == "BTC");
    }

    [Fact]
    public async Task History_falls_through_to_a_source_that_actually_has_it()
    {
        // CoinMarketCap has a key but no free history: the chain must not stop there.
        FakeHttpMessageHandler handler = new();
        MarketDataOptions options = new()
        {
            CryptoProviderOrder = ["coinmarketcap", "simulated"],
        };
        options.ApiKeys["coinmarketcap"] = "cmc-key";

        MarketDataService service = Build(handler, options);
        IReadOnlyList<Candle> candles = await service.GetHistoryAsync(Bitcoin, "EUR", HistoryRange.Month);

        Assert.NotEmpty(candles);
    }

    /// <summary>A source that is reachable but knows nothing, to prove we do not invent a price.</summary>
    private sealed class SilentProvider : IQuoteProvider
    {
        public string Id => "silent";

        public string DisplayName => "Source muette";

        public IReadOnlySet<AssetKind> SupportedKinds { get; } =
            new HashSet<AssetKind> { AssetKind.Crypto, AssetKind.Stock, AssetKind.Etf };

        public bool IsConfigured => true;

        public Task<IReadOnlyList<Quote>> GetQuotesAsync(
            IReadOnlyCollection<Asset> assets,
            string currency,
            CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<Quote>>([]);

        public Task<IReadOnlyList<Asset>> SearchAsync(
            string query,
            AssetKind? kind,
            int limit,
            CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<Asset>>([]);

        public Task<IReadOnlyList<Candle>> GetHistoryAsync(
            Asset asset,
            string currency,
            HistoryRange range,
            CancellationToken cancellationToken = default) =>
            Task.FromResult<IReadOnlyList<Candle>>([]);
    }
}

public class FxRateServiceTests
{
    [Fact]
    public async Task The_same_currency_needs_no_call_at_all()
    {
        FakeHttpMessageHandler handler = new();
        FxRateService service = new(handler.CreateClient(), new MemoryCache(new MemoryCacheOptions()), new MarketDataOptions());

        Assert.Equal(1m, await service.GetRateAsync("EUR", "EUR"));
        Assert.Empty(handler.RequestedUrls);
    }

    [Fact]
    public async Task Rates_come_from_the_european_central_bank_feed()
    {
        FakeHttpMessageHandler handler = new FakeHttpMessageHandler()
            .RespondWithFixture("frankfurter", "frankfurter-usd-eur.json");
        FxRateService service = new(handler.CreateClient(), new MemoryCache(new MemoryCacheOptions()), new MarketDataOptions());

        Assert.Equal(0.86044m, await service.GetRateAsync("USD", "EUR"));
    }

    [Fact]
    public async Task A_rate_is_only_fetched_once_per_cache_window()
    {
        FakeHttpMessageHandler handler = new FakeHttpMessageHandler()
            .RespondWithFixture("frankfurter", "frankfurter-usd-eur.json");
        FxRateService service = new(handler.CreateClient(), new MemoryCache(new MemoryCacheOptions()), new MarketDataOptions());

        await service.GetRateAsync("USD", "EUR");
        await service.GetRateAsync("USD", "EUR");

        Assert.Single(handler.RequestedUrls);
    }

    [Fact]
    public async Task Percentages_are_left_alone_when_a_quote_changes_currency()
    {
        FakeHttpMessageHandler handler = new FakeHttpMessageHandler()
            .RespondWithFixture("frankfurter", "frankfurter-usd-eur.json");
        FxRateService service = new(handler.CreateClient(), new MemoryCache(new MemoryCacheOptions()), new MarketDataOptions());

        Quote dollars = new()
        {
            Symbol = "MSFT",
            Kind = AssetKind.Stock,
            Price = 100m,
            Currency = "USD",
            ChangePercent24h = -2.5m,
            Change24h = -2.56m,
            AsOf = DateTimeOffset.UtcNow,
            SourceId = "test",
        };

        Quote euros = await service.ConvertAsync(dollars, "EUR");

        Assert.Equal(86.044m, euros.Price);
        Assert.Equal("EUR", euros.Currency);
        Assert.Equal(-2.5m, euros.ChangePercent24h);
        Assert.Equal(-2.56m * 0.86044m, euros.Change24h);
    }

    [Fact]
    public async Task Yahoo_takes_over_when_the_central_bank_feed_is_down()
    {
        FakeHttpMessageHandler handler = new FakeHttpMessageHandler()
            .Respond("frankfurter", "boom", HttpStatusCode.ServiceUnavailable)
            .Respond("USDEUR=X", """
                {"chart":{"result":[{"meta":{"currency":"EUR","symbol":"USDEUR=X","regularMarketPrice":0.87}}]}}
                """);

        FxRateService service = new(handler.CreateClient(), new MemoryCache(new MemoryCacheOptions()), new MarketDataOptions());

        Assert.Equal(0.87m, await service.GetRateAsync("USD", "EUR"));
    }
}
