namespace SafeInvest.Core.Storage;

/// <summary>
/// User preferences: which market data sources to try, in what order, and with which keys.
/// Keys are stored encrypted; <see cref="ProtectedApiKeys"/> never holds clear text.
/// </summary>
public sealed class AppSettings
{
    /// <summary>Provider ids tried in order for cryptocurrencies.</summary>
    public List<string> CryptoProviderOrder { get; set; } = ["coingecko", "coinmarketcap", "scraper", "simulated"];

    /// <summary>Provider ids tried in order for shares and ETFs.</summary>
    public List<string> StockProviderOrder { get; set; } = ["yahoo", "finnhub", "scraper", "simulated"];

    /// <summary>Provider id to encrypted key. Use <c>SettingsService</c> to read or write.</summary>
    public Dictionary<string, string> ProtectedApiKeys { get; set; } = [];

    public int QuoteCacheSeconds { get; set; } = 60;

    public int RefreshIntervalSeconds { get; set; } = 60;

    public string DefaultCurrency { get; set; } = "EUR";

    public decimal DefaultFeePercent { get; set; }

    public decimal DefaultStartingCash { get; set; } = 10_000m;

    /// <summary>Skips every network provider and prices everything from the simulator.</summary>
    public bool ForceSimulatedMode { get; set; }

    /// <summary>Swaps green/red for blue/orange, which stays readable for deuteranopia.</summary>
    public bool ColorBlindPalette { get; set; }

    /// <summary>"Default", "Light" or "Dark".</summary>
    public string Theme { get; set; } = "Default";
}
