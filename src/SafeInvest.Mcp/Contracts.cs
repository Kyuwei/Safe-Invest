using SafeInvest.Core.Models;

namespace SafeInvest.Mcp;

/// <summary>
/// Shapes returned to the AI. They are deliberately flatter and more explicit than the
/// domain model: an agent reads these once per turn and should not have to infer anything.
/// </summary>
internal static class Contracts
{
    public sealed record ErrorResponse
    {
        /// <summary>Always false; it lets a caller branch on one field for every tool.</summary>
        public bool Ok { get; init; }

        public required string Error { get; init; }

        /// <summary>What the caller can do about it, in plain language.</summary>
        public string? Hint { get; init; }
    }

    public sealed record GameRow
    {
        public required string GameId { get; init; }

        public required string PlayerName { get; init; }

        public required string PlayerKind { get; init; }

        public required string Currency { get; init; }

        public required decimal StartingCash { get; init; }

        public required decimal Cash { get; init; }

        public required int Holdings { get; init; }

        public required int Trades { get; init; }

        public required DateTimeOffset UpdatedAt { get; init; }

        public decimal? GoalAmount { get; init; }

        public DateTimeOffset? GoalDeadline { get; init; }

        public bool IsCurrent { get; init; }
    }

    public sealed record PositionRow
    {
        public required string Symbol { get; init; }

        public required string Name { get; init; }

        public required string Kind { get; init; }

        public required decimal Quantity { get; init; }

        public required decimal AverageCost { get; init; }

        public decimal? Price { get; init; }

        public decimal? MarketValue { get; init; }

        public decimal? UnrealizedPnL { get; init; }

        public decimal? UnrealizedPnLPercent { get; init; }

        public decimal? ChangePercent24h { get; init; }

        public decimal WeightPercent { get; init; }

        /// <summary>"up", "down" or "flat" — the same colouring the human sees.</summary>
        public required string Direction { get; init; }

        public string? PriceSource { get; init; }

        public bool PriceIsSimulated { get; init; }
    }

    public sealed record PortfolioResponse
    {
        public bool Ok { get; } = true;

        public required string GameId { get; init; }

        public required string PlayerName { get; init; }

        public required string PlayerKind { get; init; }

        public required string Currency { get; init; }

        public required decimal Cash { get; init; }

        public required decimal StartingCash { get; init; }

        public required decimal InvestedValue { get; init; }

        public required decimal TotalValue { get; init; }

        public required decimal TotalPnL { get; init; }

        public required decimal TotalPnLPercent { get; init; }

        public required decimal RealizedPnL { get; init; }

        public required decimal UnrealizedPnL { get; init; }

        public required string Direction { get; init; }

        public required IReadOnlyList<PositionRow> Positions { get; init; }

        public GoalResponse? Goal { get; init; }

        /// <summary>Assets held whose price could not be obtained at all.</summary>
        public IReadOnlyList<string> UnpricedSymbols { get; init; } = [];

        /// <summary>True when at least one holding is valued with an invented price.</summary>
        public bool ContainsSimulatedPrices { get; init; }

        public required DateTimeOffset AsOf { get; init; }
    }

    public sealed record GoalResponse
    {
        public required decimal TargetAmount { get; init; }

        public required DateTimeOffset Deadline { get; init; }

        public required decimal CurrentValue { get; init; }

        public required decimal AmountRemaining { get; init; }

        public required decimal ProgressPercent { get; init; }

        public required int DaysRemaining { get; init; }

        public required string Status { get; init; }

        public decimal? RequiredAnnualisedReturnPercent { get; init; }

        public decimal? AchievedAnnualisedReturnPercent { get; init; }
    }

    public sealed record QuoteRow
    {
        public required string Symbol { get; init; }

        public required string Name { get; init; }

        public required string Kind { get; init; }

        public required decimal Price { get; init; }

        public required string Currency { get; init; }

        public decimal? ChangePercent24h { get; init; }

        public decimal? MarketCap { get; init; }

        public decimal? Volume24h { get; init; }

        public required string Direction { get; init; }

        public required string Source { get; init; }

        public required bool IsSimulated { get; init; }

        public required DateTimeOffset AsOf { get; init; }
    }

    public sealed record AssetRow
    {
        public required string Symbol { get; init; }

        public required string Name { get; init; }

        public required string Kind { get; init; }

        public string? ProviderId { get; init; }
    }

    public sealed record TradeRow
    {
        public required DateTimeOffset Timestamp { get; init; }

        public required string Side { get; init; }

        public required string Symbol { get; init; }

        public required string Name { get; init; }

        public required decimal Quantity { get; init; }

        public required decimal UnitPrice { get; init; }

        public required decimal Total { get; init; }

        public decimal Fees { get; init; }

        public decimal? RealizedPnL { get; init; }

        public string? Rationale { get; init; }

        public required string By { get; init; }

        public bool PriceWasSimulated { get; init; }
    }

    public sealed record TradeResponse
    {
        public bool Ok { get; } = true;

        public required TradeRow Trade { get; init; }

        public required decimal CashAfter { get; init; }

        public required decimal TotalValueAfter { get; init; }

        public required string Currency { get; init; }

        public GoalResponse? Goal { get; init; }

        /// <summary>Set when the trade was priced from an invented quote rather than a real one.</summary>
        public string? Warning { get; init; }
    }

    public sealed record CandleRow
    {
        public required DateTimeOffset Timestamp { get; init; }

        public required decimal Close { get; init; }
    }

    public static string Direction(int direction) => direction switch
    {
        > 0 => "up",
        < 0 => "down",
        _ => "flat",
    };

    public static PositionRow ToRow(PositionView position) => new()
    {
        Symbol = position.Asset.Symbol,
        Name = position.Asset.Name,
        Kind = position.Asset.Kind.ToString(),
        Quantity = position.Quantity,
        AverageCost = position.AverageCost,
        Price = position.Price,
        MarketValue = position.MarketValue,
        UnrealizedPnL = position.UnrealizedPnL,
        UnrealizedPnLPercent = position.UnrealizedPnLPercent,
        ChangePercent24h = position.ChangePercent24h,
        WeightPercent = position.WeightPercent,
        Direction = Direction(position.Direction),
        PriceSource = position.SourceId,
        PriceIsSimulated = position.IsSimulated,
    };

    public static QuoteRow ToRow(Quote quote) => new()
    {
        Symbol = quote.Symbol,
        Name = quote.Name ?? quote.Symbol,
        Kind = quote.Kind.ToString(),
        Price = quote.Price,
        Currency = quote.Currency,
        ChangePercent24h = quote.ChangePercent24h,
        MarketCap = quote.MarketCap,
        Volume24h = quote.Volume24h,
        Direction = Direction(quote.Direction),
        Source = quote.SourceId,
        IsSimulated = quote.IsSimulated,
        AsOf = quote.AsOf,
    };

    public static AssetRow ToRow(Asset asset) => new()
    {
        Symbol = asset.Symbol,
        Name = asset.Name,
        Kind = asset.Kind.ToString(),
        ProviderId = asset.ProviderId,
    };

    public static TradeRow ToRow(Trade trade) => new()
    {
        Timestamp = trade.Timestamp,
        Side = trade.Side.ToString(),
        Symbol = trade.Asset.Symbol,
        Name = trade.Asset.Name,
        Quantity = trade.Quantity,
        UnitPrice = trade.UnitPrice,
        Total = trade.Total,
        Fees = trade.Fees,
        RealizedPnL = trade.RealizedPnL,
        Rationale = trade.Rationale,
        By = trade.ActorKind.ToString(),
        PriceWasSimulated = trade.QuoteWasSimulated,
    };

    public static GoalResponse? ToResponse(GoalProgress? progress) => progress is null ? null : new GoalResponse
    {
        TargetAmount = progress.TargetAmount,
        Deadline = progress.Deadline,
        CurrentValue = progress.CurrentValue,
        AmountRemaining = progress.AmountRemaining,
        ProgressPercent = progress.ProgressPercent,
        DaysRemaining = progress.DaysRemaining,
        Status = progress.Status.ToString(),
        RequiredAnnualisedReturnPercent = progress.RequiredAnnualisedReturnPercent,
        AchievedAnnualisedReturnPercent = progress.AchievedAnnualisedReturnPercent,
    };
}
