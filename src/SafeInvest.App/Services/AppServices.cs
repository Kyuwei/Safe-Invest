using Microsoft.Extensions.DependencyInjection;
using SafeInvest.Core.Engine;
using SafeInvest.Core.Storage;
using SafeInvest.MarketData;

namespace SafeInvest.App.Services;

/// <summary>
/// The application's service container. Built once at launch, so the market data chain,
/// the store and the settings are shared by every screen — and are the same objects the
/// MCP server builds on its side.
/// </summary>
internal static class AppServices
{
    private static IServiceProvider? _provider;

    public static IServiceProvider Provider =>
        _provider ?? throw new InvalidOperationException("AppServices.Initialize n'a pas été appelé.");

    public static T Get<T>() where T : notnull => Provider.GetRequiredService<T>();

    public static void Initialize()
    {
        if (_provider is not null)
        {
            return;
        }

        SafeInvestPaths.EnsureCreated();

        ServiceCollection services = new();

        SettingsService settingsService = new();
        AppSettings settings = settingsService.LoadAsync().GetAwaiter().GetResult();
        MarketDataOptions marketDataOptions = settings.ToMarketDataOptions(settingsService);

        services.AddSingleton(TimeProvider.System);
        services.AddSingleton(settingsService);
        services.AddSingleton(settings);
        services.AddSingleton<IGameStore>(_ => new JsonGameStore());
        services.AddSingleton<PortfolioEngine>();
        services.AddSafeInvestMarketData(marketDataOptions);
        services.AddSingleton<GameSessionService>();

        _provider = services.BuildServiceProvider();

        PaletteService.Apply(settings.ColorBlindPalette);
    }

    /// <summary>
    /// Rebuilds the market data chain after the user changes providers or keys, without
    /// restarting the app. The store and the open game are untouched.
    /// </summary>
    public static void ReloadMarketData()
    {
        if (_provider is null)
        {
            return;
        }

        SettingsService settingsService = Get<SettingsService>();
        settingsService.Invalidate();
        AppSettings settings = settingsService.LoadAsync().GetAwaiter().GetResult();

        GameSessionService session = Get<GameSessionService>();

        ServiceCollection services = new();
        services.AddSingleton(TimeProvider.System);
        services.AddSingleton(settingsService);
        services.AddSingleton(settings);
        services.AddSingleton(Get<IGameStore>());
        services.AddSingleton(Get<PortfolioEngine>());
        services.AddSafeInvestMarketData(settings.ToMarketDataOptions(settingsService));
        services.AddSingleton(session);

        _provider = services.BuildServiceProvider();

        session.UseMarketData(Get<IMarketDataService>());
        PaletteService.Apply(settings.ColorBlindPalette);
    }
}
