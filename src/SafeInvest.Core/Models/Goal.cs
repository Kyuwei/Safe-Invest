namespace SafeInvest.Core.Models;

/// <summary>An amount to reach by a date. Mainly used to brief an AI player.</summary>
public sealed record Goal
{
    public required decimal TargetAmount { get; init; }

    public required DateTimeOffset Deadline { get; init; }
}

/// <summary>How a session is doing against its <see cref="Goal"/>.</summary>
public sealed record GoalProgress
{
    public required decimal TargetAmount { get; init; }

    public required DateTimeOffset Deadline { get; init; }

    public required decimal CurrentValue { get; init; }

    public required decimal StartingCash { get; init; }

    /// <summary>0 to 100, clamped. Measured from the starting cash, not from zero.</summary>
    public required decimal ProgressPercent { get; init; }

    public required decimal AmountRemaining { get; init; }

    public required int DaysRemaining { get; init; }

    public required GoalStatus Status { get; init; }

    /// <summary>Annualised return still needed, from today's value, to land on target.</summary>
    public decimal? RequiredAnnualisedReturnPercent { get; init; }

    /// <summary>Annualised return actually achieved since the session started.</summary>
    public decimal? AchievedAnnualisedReturnPercent { get; init; }
}
