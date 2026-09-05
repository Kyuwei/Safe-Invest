namespace SafeInvest.MarketData;

/// <summary>Tuning knobs for the market data layer, filled from <c>AppSettings</c>.</summary>
public sealed class MarketDataOptions
{
    /// <summary>Provider ids tried in order for cryptocurrencies.</summary>
    public IReadOnlyList<string> CryptoProviderOrder { get; set; } =
        ["coingecko", "coinmarketcap", "scraper", "simulated"];

    /// <summary>Provider ids tried in order for shares and ETFs.</summary>
    public IReadOnlyList<string> StockProviderOrder { get; set; } =
        ["yahoo", "finnhub", "scraper", "simulated"];

    public TimeSpan QuoteCacheDuration { get; set; } = TimeSpan.FromSeconds(60);

    public TimeSpan HistoryCacheDuration { get; set; } = TimeSpan.FromMinutes(15);

    public TimeSpan FxCacheDuration { get; set; } = TimeSpan.FromHours(1);

    public TimeSpan RequestTimeout { get; set; } = TimeSpan.FromSeconds(12);

    /// <summary>Bypasses every network provider and prices everything from the simulator.</summary>
    public bool ForceSimulated { get; set; }

    /// <summary>Clear-text API keys by provider id. Resolved by the host from settings.</summary>
    public Dictionary<string, string?> ApiKeys { get; } = [];

    public string? ApiKeyFor(string providerId) =>
        ApiKeys.TryGetValue(providerId, out string? key) && !string.IsNullOrWhiteSpace(key) ? key : null;
}

/// <summary>What the settings screen shows for each configured source.</summary>
public sealed record ProviderStatus
{
    public required string Id { get; init; }

    public required string DisplayName { get; init; }

    public required bool IsConfigured { get; init; }

    public required bool IsSimulated { get; init; }

    /// <summary>Null while untried, true after a success, false after a failure.</summary>
    public bool? LastCallSucceeded { get; init; }

    public DateTimeOffset? LastCallAt { get; init; }

    public string? LastError { get; init; }
}
