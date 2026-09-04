using System.Text.Json.Serialization;

namespace SafeInvest.Core.Models;

/// <summary>One line of the portfolio, valued at a point in time.</summary>
public sealed record PositionView
{
    public required Asset Asset { get; init; }

    public required decimal Quantity { get; init; }

    public required decimal AverageCost { get; init; }

    public required decimal CostBasis { get; init; }

    /// <summary>Null when no quote could be obtained for this asset.</summary>
    public decimal? Price { get; init; }

    public decimal? MarketValue { get; init; }

    public decimal? UnrealizedPnL { get; init; }

    public decimal? UnrealizedPnLPercent { get; init; }

    public decimal? ChangePercent24h { get; init; }

    public string? SourceId { get; init; }

    public bool IsSimulated { get; init; }

    public DateTimeOffset? QuotedAt { get; init; }

    /// <summary>Share of the total portfolio value, 0 to 100.</summary>
    public decimal WeightPercent { get; init; }

    /// <summary>+1 up, -1 down, 0 flat/unknown. Drives the green/red colouring.</summary>
    [JsonIgnore]
    public int Direction => UnrealizedPnL switch
    {
        null => 0,
        > 0 => 1,
        < 0 => -1,
        _ => 0,
    };
}

/// <summary>The portfolio as a whole, valued at a point in time.</summary>
public sealed record PortfolioSnapshot
{
    public required DateTimeOffset AsOf { get; init; }

    public required string Currency { get; init; }

    public required decimal Cash { get; init; }

    public required decimal StartingCash { get; init; }

    /// <summary>Value of the held positions only.</summary>
    public required decimal MarketValue { get; init; }

    /// <summary>Cash plus market value — the number shown in big on the dashboard.</summary>
    public required decimal TotalValue { get; init; }

    public required decimal TotalPnL { get; init; }

    public required decimal TotalPnLPercent { get; init; }

    public required decimal RealizedPnL { get; init; }

    public required decimal UnrealizedPnL { get; init; }

    public required IReadOnlyList<PositionView> Positions { get; init; }

    /// <summary>True when at least one position was valued with a simulated price.</summary>
    public bool ContainsSimulatedPrices { get; init; }

    /// <summary>Assets we hold but could not price at all.</summary>
    public IReadOnlyList<string> UnpricedSymbols { get; init; } = [];

    [JsonIgnore]
    public int Direction => TotalPnL switch
    {
        > 0 => 1,
        < 0 => -1,
        _ => 0,
    };
}
