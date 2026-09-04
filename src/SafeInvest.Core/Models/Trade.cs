namespace SafeInvest.Core.Models;

/// <summary>
/// One completed buy or sell. <see cref="Rationale"/> is mandatory for AI players —
/// the point of the AI mode is that every move comes with a stated reason.
/// </summary>
public sealed record Trade
{
    public required Guid Id { get; init; }

    public required DateTimeOffset Timestamp { get; init; }

    public required TradeSide Side { get; init; }

    public required Asset Asset { get; init; }

    public required decimal Quantity { get; init; }

    public required decimal UnitPrice { get; init; }

    public required decimal Fees { get; init; }

    /// <summary>Absolute cash movement, fees included.</summary>
    public required decimal Total { get; init; }

    /// <summary>Profit or loss realised by this trade. Sells only.</summary>
    public decimal? RealizedPnL { get; init; }

    public string? Rationale { get; init; }

    public required PlayerKind ActorKind { get; init; }

    public string? QuoteSourceId { get; init; }

    public bool QuoteWasSimulated { get; init; }
}
