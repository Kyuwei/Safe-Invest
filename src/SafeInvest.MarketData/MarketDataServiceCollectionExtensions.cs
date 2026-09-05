using Microsoft.Extensions.DependencyInjection;
using SafeInvest.Core.Abstractions;
using SafeInvest.Core.Storage;
using SafeInvest.MarketData.Fx;
using SafeInvest.MarketData.Providers;

namespace SafeInvest.MarketData;

/// <summary>
/// Wires the whole market data layer up in one call, so the WinUI app and the MCP server
/// get an identical chain of sources without duplicating the registration.
/// </summary>
public static class MarketDataServiceCollectionExtensions
{
    public static IServiceCollection AddSafeInvestMarketData(
        this IServiceCollection services,
        MarketDataOptions options)
    {
        ArgumentNullException.ThrowIfNull(services);
        ArgumentNullException.ThrowIfNull(options);

        services.AddSingleton(options);
        services.AddMemoryCache();

        services.AddHttpClient(nameof(CoinGeckoProvider), c => c.Timeout = options.RequestTimeout);
        services.AddHttpClient(nameof(CoinMarketCapProvider), c => c.Timeout = options.RequestTimeout);
        services.AddHttpClient(nameof(YahooFinanceProvider), c => c.Timeout = options.RequestTimeout);
        services.AddHttpClient(nameof(FinnhubProvider), c => c.Timeout = options.RequestTimeout);
        services.AddHttpClient(nameof(WebScrapeProvider), c => c.Timeout = options.RequestTimeout);
        services.AddHttpClient(nameof(FxRateService), c => c.Timeout = options.RequestTimeout);

        services.AddSingleton<IFxRateService>(sp => new FxRateService(
            sp.GetRequiredService<IHttpClientFactory>().CreateClient(nameof(FxRateService)),
            sp.GetRequiredService<Microsoft.Extensions.Caching.Memory.IMemoryCache>(),
            options));

        services.AddSingleton<IQuoteProvider>(sp => new CoinGeckoProvider(
            sp.GetRequiredService<IHttpClientFactory>().CreateClient(nameof(CoinGeckoProvider)), options));

        services.AddSingleton<IQuoteProvider>(sp => new CoinMarketCapProvider(
            sp.GetRequiredService<IHttpClientFactory>().CreateClient(nameof(CoinMarketCapProvider)), options));

        services.AddSingleton<IQuoteProvider>(sp => new YahooFinanceProvider(
            sp.GetRequiredService<IHttpClientFactory>().CreateClient(nameof(YahooFinanceProvider))));

        services.AddSingleton<IQuoteProvider>(sp => new FinnhubProvider(
            sp.GetRequiredService<IHttpClientFactory>().CreateClient(nameof(FinnhubProvider)), options));

        services.AddSingleton<IQuoteProvider>(sp => new WebScrapeProvider(
            sp.GetRequiredService<IHttpClientFactory>().CreateClient(nameof(WebScrapeProvider))));

        // Always last in the chain: the app must still work on a train with no signal.
        services.AddSingleton<IQuoteProvider>(_ => new SimulatedQuoteProvider());

        services.AddSingleton<IMarketDataService, MarketDataService>();

        return services;
    }

    /// <summary>Projects the user's saved preferences onto the market data options.</summary>
    public static MarketDataOptions ToMarketDataOptions(this AppSettings settings, SettingsService settingsService)
    {
        ArgumentNullException.ThrowIfNull(settings);
        ArgumentNullException.ThrowIfNull(settingsService);

        MarketDataOptions options = new()
        {
            CryptoProviderOrder = [.. settings.CryptoProviderOrder],
            StockProviderOrder = [.. settings.StockProviderOrder],
            QuoteCacheDuration = TimeSpan.FromSeconds(Math.Clamp(settings.QuoteCacheSeconds, 5, 3600)),
            ForceSimulated = settings.ForceSimulatedMode,
        };

        foreach (string providerId in new[] { "coingecko", "coinmarketcap", "finnhub" })
        {
            options.ApiKeys[providerId] = settingsService.GetApiKey(settings, providerId);
        }

        return options;
    }
}
