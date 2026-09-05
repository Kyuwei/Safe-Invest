using SafeInvest.Core.Models;

namespace SafeInvest.Core.Engine;

/// <summary>
/// Creates validated <see cref="GameSession"/> objects. Both the "new game" dialog and
/// the MCP <c>create_game</c> tool go through here so a game started by an AI is exactly
/// the same shape as one started by a human.
/// </summary>
public static class GameFactory
{
    public const decimal MinimumStartingCash = 1m;
    public const decimal MaximumStartingCash = 1_000_000_000m;

    public static GameSession Create(
        string playerName,
        PlayerKind playerKind,
        decimal startingCash,
        string currency = "EUR",
        decimal feePercent = 0m,
        decimal? goalAmount = null,
        DateTimeOffset? goalDeadline = null,
        TimeProvider? timeProvider = null)
    {
        TimeProvider clock = timeProvider ?? TimeProvider.System;
        DateTimeOffset now = clock.GetUtcNow();

        string name = string.IsNullOrWhiteSpace(playerName)
            ? (playerKind == PlayerKind.Ai ? "IA" : "Joueur")
            : playerName.Trim();

        if (startingCash < MinimumStartingCash || startingCash > MaximumStartingCash)
        {
            throw new ArgumentOutOfRangeException(
                nameof(startingCash),
                startingCash,
                $"Le capital de départ doit être compris entre {MinimumStartingCash:N0} et {MaximumStartingCash:N0}.");
        }

        if (feePercent is < 0m or > 100m)
        {
            throw new ArgumentOutOfRangeException(
                nameof(feePercent), feePercent, "Les frais doivent être compris entre 0 et 100 %.");
        }

        string code = string.IsNullOrWhiteSpace(currency) ? "EUR" : currency.Trim().ToUpperInvariant();
        decimal cash = MoneyMath.RoundMoney(startingCash);

        return new GameSession
        {
            Id = Guid.NewGuid(),
            PlayerName = name,
            PlayerKind = playerKind,
            Currency = code,
            StartingCash = cash,
            Cash = cash,
            FeePercent = feePercent,
            Goal = BuildGoal(goalAmount, goalDeadline, cash, now),
            CreatedAt = now,
            UpdatedAt = now,
        };
    }

    /// <summary>Validates and attaches a target amount and deadline to an existing game.</summary>
    public static Goal BuildGoal(decimal amount, DateTimeOffset deadline, decimal referenceValue, DateTimeOffset now)
        => BuildGoal(amount, deadline, referenceValue, now, required: true)!;

    private static Goal? BuildGoal(
        decimal? amount,
        DateTimeOffset? deadline,
        decimal referenceValue,
        DateTimeOffset now)
        => BuildGoal(amount, deadline, referenceValue, now, required: false);

    private static Goal? BuildGoal(
        decimal? amount,
        DateTimeOffset? deadline,
        decimal referenceValue,
        DateTimeOffset now,
        bool required)
    {
        if (amount is null && deadline is null)
        {
            return required
                ? throw new ArgumentException("Un objectif demande un montant et une date.", nameof(amount))
                : null;
        }

        if (amount is null || deadline is null)
        {
            throw new ArgumentException(
                "Un objectif demande à la fois un montant cible et une date limite.", nameof(amount));
        }

        if (amount.Value <= 0m)
        {
            throw new ArgumentOutOfRangeException(
                nameof(amount), amount.Value, "Le montant cible doit être strictement positif.");
        }

        if (amount.Value <= referenceValue)
        {
            throw new ArgumentOutOfRangeException(
                nameof(amount),
                amount.Value,
                $"Le montant cible ({amount.Value:N2}) doit dépasser la valeur actuelle ({referenceValue:N2}).");
        }

        if (deadline.Value <= now)
        {
            throw new ArgumentOutOfRangeException(
                nameof(deadline), deadline.Value, "La date limite doit être dans le futur.");
        }

        return new Goal { TargetAmount = MoneyMath.RoundMoney(amount.Value), Deadline = deadline.Value };
    }
}
