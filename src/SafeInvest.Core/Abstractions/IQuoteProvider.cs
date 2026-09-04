using SafeInvest.Core.Models;

namespace SafeInvest.Core.Abstractions;

/// <summary>A daily (or intraday) price bar, used to draw sparklines.</summary>
public sealed record Candle
{
    public required DateTimeOffset Timestamp { get; init; }

    public required decimal Close { get; init; }

    public decimal? Open { get; init; }

    public decimal? High { get; init; }

    public decimal? Low { get; init; }

    public decimal? Volume { get; init; }
}

/// <summary>How far back a price history request reaches.</summary>
public enum HistoryRange
{
    Day,
    Week,
    Month,
    Quarter,
    Year,
}

/// <summary>
/// One source of market data. Implementations live in SafeInvest.MarketData and are
/// chained by <c>MarketDataService</c>, which falls through to the next one on failure.
/// </summary>
public interface IQuoteProvider
{
    /// <summary>Stable lowercase id used in settings and shown in the UI ("coingecko").</summary>
    string Id { get; }

    /// <summary>Human-readable name for the settings screen.</summary>
    string DisplayName { get; }

    /// <summary>Asset families this provider can price.</summary>
    IReadOnlySet<AssetKind> SupportedKinds { get; }

    /// <summary>False when a required API key is missing, so the chain skips it silently.</summary>
    bool IsConfigured { get; }

    /// <summary>True when prices are made up rather than fetched (demo/offline mode).</summary>
    bool IsSimulated => false;

    Task<IReadOnlyList<Quote>> GetQuotesAsync(
        IReadOnlyCollection<Asset> assets,
        string currency,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<Asset>> SearchAsync(
        string query,
        AssetKind? kind,
        int limit,
        CancellationToken cancellationToken = default);

    Task<IReadOnlyList<Candle>> GetHistoryAsync(
        Asset asset,
        string currency,
        HistoryRange range,
        CancellationToken cancellationToken = default);
}

/// <summary>Raised when a provider fails; the chain catches it and tries the next source.</summary>
public sealed class QuoteProviderException : Exception
{
    public QuoteProviderException(string providerId, string message)
        : base(message) => ProviderId = providerId;

    public QuoteProviderException(string providerId, string message, Exception innerException)
        : base(message, innerException) => ProviderId = providerId;

    public string ProviderId { get; } = string.Empty;

    /// <summary>Set when the provider refused because the free quota is exhausted.</summary>
    public bool IsRateLimited { get; init; }
}
