using SafeInvest.Core.Models;

namespace SafeInvest.Core.Engine;

/// <summary>
/// Scores a session against its target amount and deadline: how far along it is, how
/// long is left, and what yearly return would still be needed to get there.
/// </summary>
public static class GoalTracker
{
    private const decimal DaysPerYear = 365.25m;

    public static GoalProgress? Evaluate(GameSession session, PortfolioSnapshot snapshot, DateTimeOffset now)
    {
        ArgumentNullException.ThrowIfNull(session);
        ArgumentNullException.ThrowIfNull(snapshot);

        if (session.Goal is not { } goal)
        {
            return null;
        }

        decimal current = snapshot.TotalValue;
        decimal remaining = MoneyMath.RoundMoney(goal.TargetAmount - current);

        // Progress is measured on the ground actually covered: from the starting cash
        // to the target, not from zero — otherwise every game starts at 80 %.
        decimal span = goal.TargetAmount - session.StartingCash;
        decimal progress = span <= 0m
            ? 100m
            : Math.Clamp(MoneyMath.Percent(current - session.StartingCash, span), 0m, 100m);

        double yearsLeft = Math.Max((goal.Deadline - now).TotalDays, 0d) / (double)DaysPerYear;
        double yearsElapsed = Math.Max((now - session.CreatedAt).TotalDays, 0d) / (double)DaysPerYear;
        double fullHorizon = Math.Max((goal.Deadline - session.CreatedAt).TotalDays, 0d) / (double)DaysPerYear;

        decimal? required = Annualised(current, goal.TargetAmount, yearsLeft);
        decimal? achieved = Annualised(session.StartingCash, current, yearsElapsed);
        decimal? requiredFromStart = Annualised(session.StartingCash, goal.TargetAmount, fullHorizon);

        GoalStatus status;
        if (current >= goal.TargetAmount)
        {
            status = GoalStatus.Achieved;
        }
        else if (now > goal.Deadline)
        {
            status = GoalStatus.Expired;
        }
        else if (achieved is null || requiredFromStart is null)
        {
            // Too early to judge a trend — do not scare the player on day one.
            status = GoalStatus.OnTrack;
        }
        else
        {
            status = achieved.Value >= requiredFromStart.Value ? GoalStatus.OnTrack : GoalStatus.Behind;
        }

        return new GoalProgress
        {
            TargetAmount = goal.TargetAmount,
            Deadline = goal.Deadline,
            CurrentValue = current,
            StartingCash = session.StartingCash,
            ProgressPercent = progress,
            AmountRemaining = Math.Max(remaining, 0m),
            DaysRemaining = (int)Math.Ceiling(Math.Max((goal.Deadline - now).TotalDays, 0d)),
            Status = status,
            RequiredAnnualisedReturnPercent = required,
            AchievedAnnualisedReturnPercent = achieved,
        };
    }

    /// <summary>
    /// Compound annual growth rate, in percent. Null when the maths would be meaningless
    /// (no elapsed time, or a non-positive starting value).
    /// </summary>
    public static decimal? Annualised(decimal from, decimal to, double years)
    {
        if (from <= 0m || years <= 0.0027d)
        {
            // Under ~1 day the rate explodes to absurd values; report nothing instead.
            return null;
        }

        double ratio = (double)(to / from);
        if (ratio <= 0d)
        {
            return null;
        }

        double rate = Math.Pow(ratio, 1d / years) - 1d;
        if (double.IsNaN(rate) || double.IsInfinity(rate))
        {
            return null;
        }

        // Cap the display: "+9 999 %/an" says "impossible" just as well as "+4e12 %".
        rate = Math.Clamp(rate, -1d, 99.99d);
        return Math.Round((decimal)(rate * 100d), 2, MidpointRounding.AwayFromZero);
    }
}
