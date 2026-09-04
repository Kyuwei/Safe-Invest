using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging;
using ModelContextProtocol.Protocol;
using SafeInvest.Core.Engine;
using SafeInvest.Core.Storage;
using SafeInvest.MarketData;
using SafeInvest.Mcp;
using SafeInvest.Mcp.Tools;

// Safe Invest MCP server — lets an AI play a game the desktop app displays live.
//
// Both processes read and write the same files under %LOCALAPPDATA%\SafeInvest, with the
// store serialising writes across processes, so the app picks a trade up within a moment
// of the AI making it. The app does not need to be running.

HostApplicationBuilder builder = Host.CreateApplicationBuilder(args);

// stdout is the MCP transport: one stray line there breaks the protocol, so every log
// goes to stderr instead.
builder.Logging.ClearProviders();
builder.Logging.AddConsole(options => options.LogToStandardErrorThreshold = LogLevel.Trace);
builder.Logging.SetMinimumLevel(LogLevel.Warning);

SafeInvestPaths.EnsureCreated();

// Read the same preferences the app writes: provider order, API keys, demo mode.
using SettingsService settingsService = new();
AppSettings settings = await settingsService.LoadAsync().ConfigureAwait(false);
MarketDataOptions marketDataOptions = settings.ToMarketDataOptions(settingsService);

// Lets CI and offline demos run the whole server without touching the network.
if (Environment.GetEnvironmentVariable("SAFEINVEST_SIMULATED") is "1" or "true")
{
    marketDataOptions.ForceSimulated = true;
}

builder.Services.AddSingleton(TimeProvider.System);
builder.Services.AddSingleton<IGameStore>(_ => new JsonGameStore());
builder.Services.AddSingleton<PortfolioEngine>();
builder.Services.AddSafeInvestMarketData(marketDataOptions);
builder.Services.AddSingleton<SafeInvestContext>();
builder.Services.AddSingleton<GameTools>();
builder.Services.AddSingleton<MarketTools>();
builder.Services.AddSingleton<TradingTools>();

builder.Services
    .AddMcpServer(options => options.ServerInfo = new Implementation
    {
        Name = "safe-invest",
        Version = ThisAssembly.Version,
    })
    .WithStdioServerTransport()
    .WithTools<GameTools>()
    .WithTools<MarketTools>()
    .WithTools<TradingTools>();

await builder.Build().RunAsync().ConfigureAwait(false);

/// <summary>Keeps the version reported to MCP clients in one place.</summary>
internal static class ThisAssembly
{
    public const string Version = "0.1.0";
}
