using System.ComponentModel;
using ModelContextProtocol.Server;
using SafeInvest.Core.Engine;
using SafeInvest.Core.Models;
using SafeInvest.MarketData;

namespace SafeInvest.Mcp.Tools;

/// <summary>Starting, listing and inspecting games.</summary>
[McpServerToolType]
internal sealed class GameTools(SafeInvestContext context)
{
    [McpServerTool(Name = "list_games")]
    [Description("Lists every saved Safe Invest game, most recently played first. " +
                 "Use it to find the gameId to pass to open_game.")]
    public Task<string> ListGamesAsync(CancellationToken cancellationToken) =>
        SafeInvestContext.GuardAsync(async () =>
        {
            IReadOnlyList<GameSummary> games = await context.Store.ListAsync(cancellationToken).ConfigureAwait(false);
            Guid? current = await context.Store.GetCurrentGameIdAsync(cancellationToken).ConfigureAwait(false);

            return new
            {
                Ok = true,
                CurrentGameId = current?.ToString(),
                Games = games.Select(g => new Contracts.GameRow
                {
                    GameId = g.Id.ToString(),
                    PlayerName = g.PlayerName,
                    PlayerKind = g.PlayerKind.ToString(),
                    Currency = g.Currency,
                    StartingCash = g.StartingCash,
                    Cash = g.Cash,
                    Holdings = g.HoldingCount,
                    Trades = g.TradeCount,
                    UpdatedAt = g.UpdatedAt,
                    GoalAmount = g.Goal?.TargetAmount,
                    GoalDeadline = g.Goal?.Deadline,
                    IsCurrent = g.Id == current,
                }).ToList(),
            };
        });

    [McpServerTool(Name = "create_game")]
    [Description("Starts a new game with a fictional starting balance and opens it, so the " +
                 "Safe Invest desktop app displays it live. Give a goal (goalAmount plus " +
                 "goalDeadline) when the game should aim at a target by a date.")]
    public Task<string> CreateGameAsync(
        [Description("Amount of fictional money to start with, in the chosen currency.")]
        decimal startingCash,
        [Description("Name shown in the app. Defaults to \"IA\".")]
        string? playerName = null,
        [Description("Who plays: \"ai\" (default) or \"human\". An AI game requires a rationale on every trade.")]
        string? playerKind = null,
        [Description("ISO currency code for the whole game. Defaults to EUR.")]
        string? currency = null,
        [Description("Target amount to reach. Requires goalDeadline. Must exceed startingCash.")]
        decimal? goalAmount = null,
        [Description("ISO-8601 date by which the target should be reached, e.g. 2027-06-30.")]
        string? goalDeadline = null,
        [Description("Trading fee as a percentage of each trade. Defaults to 0.")]
        decimal? feePercent = null,
        CancellationToken cancellationToken = default) =>
        SafeInvestContext.GuardAsync(async () =>
        {
            GameSession session = GameFactory.Create(
                playerName: playerName ?? string.Empty,
                playerKind: SafeInvestContext.ParsePlayerKind(playerKind),
                startingCash: startingCash,
                currency: currency ?? "EUR",
                feePercent: feePercent ?? 0m,
                goalAmount: goalAmount,
                goalDeadline: ParseDeadline(goalDeadline),
                timeProvider: TimeProvider.System);

            await context.Store.SaveAsync(session, cancellationToken).ConfigureAwait(false);
            await context.Store.SetCurrentGameAsync(session.Id, cancellationToken).ConfigureAwait(false);

            return await BuildPortfolioAsync(session, cancellationToken).ConfigureAwait(false);
        });

    [McpServerTool(Name = "open_game")]
    [Description("Makes an existing game the current one. Later calls without a gameId act " +
                 "on it, and the desktop app switches to it too.")]
    public Task<string> OpenGameAsync(
        [Description("Identifier from list_games.")] string gameId,
        CancellationToken cancellationToken = default) =>
        SafeInvestContext.GuardAsync(async () =>
        {
            GameSession session = await context.RequireGameAsync(gameId, cancellationToken).ConfigureAwait(false);
            await context.Store.SetCurrentGameAsync(session.Id, cancellationToken).ConfigureAwait(false);

            return await BuildPortfolioAsync(session, cancellationToken).ConfigureAwait(false);
        });

    [McpServerTool(Name = "get_portfolio")]
    [Description("The current state of a game: cash, every position valued at the latest " +
                 "price, profit and loss, and goal progress. Call this before deciding to trade.")]
    public Task<string> GetPortfolioAsync(
        [Description("Defaults to the currently open game.")] string? gameId = null,
        CancellationToken cancellationToken = default) =>
        SafeInvestContext.GuardAsync(async () =>
        {
            GameSession session = await context.RequireGameAsync(gameId, cancellationToken).ConfigureAwait(false);
            return await BuildPortfolioAsync(session, cancellationToken).ConfigureAwait(false);
        });

    [McpServerTool(Name = "set_goal")]
    [Description("Sets or replaces the target amount and deadline of a game.")]
    public Task<string> SetGoalAsync(
        [Description("Target amount, which must exceed the portfolio's current value.")]
        decimal goalAmount,
        [Description("ISO-8601 deadline, e.g. 2027-06-30.")]
        string goalDeadline,
        [Description("Defaults to the currently open game.")] string? gameId = null,
        CancellationToken cancellationToken = default) =>
        SafeInvestContext.GuardAsync(async () =>
        {
            GameSession session = await context.RequireGameAsync(gameId, cancellationToken).ConfigureAwait(false);
            PortfolioSnapshot snapshot = await context.SnapshotAsync(session, cancellationToken).ConfigureAwait(false);

            DateTimeOffset deadline = ParseDeadline(goalDeadline)
                ?? throw new SafeInvestToolException("La date limite est illisible.", "Utilisez le format AAAA-MM-JJ.");

            Goal goal = GameFactory.BuildGoal(goalAmount, deadline, snapshot.TotalValue, context.Now);

            GameSession updated = await context.Store
                .MutateAsync(session.Id, s => s.Goal = goal, cancellationToken)
                .ConfigureAwait(false);

            return await BuildPortfolioAsync(updated, cancellationToken).ConfigureAwait(false);
        });

    [McpServerTool(Name = "get_goal_progress")]
    [Description("How the game is tracking against its target: percentage covered, days " +
                 "left, and the yearly return still needed to get there.")]
    public Task<string> GetGoalProgressAsync(
        [Description("Defaults to the currently open game.")] string? gameId = null,
        CancellationToken cancellationToken = default) =>
        SafeInvestContext.GuardAsync(async () =>
        {
            GameSession session = await context.RequireGameAsync(gameId, cancellationToken).ConfigureAwait(false);

            if (session.Goal is null)
            {
                throw new SafeInvestToolException(
                    "Cette partie n'a pas d'objectif.",
                    "Définissez-en un avec set_goal.");
            }

            PortfolioSnapshot snapshot = await context.SnapshotAsync(session, cancellationToken).ConfigureAwait(false);

            return new
            {
                Ok = true,
                GameId = session.Id.ToString(),
                Currency = session.Currency,
                Goal = Contracts.ToResponse(context.Goal(session, snapshot)),
            };
        });

    [McpServerTool(Name = "get_trade_history")]
    [Description("Past trades, newest first, each with its date, price and — for AI games — " +
                 "the rationale given at the time.")]
    public Task<string> GetTradeHistoryAsync(
        [Description("How many trades to return. Defaults to 25.")] int? limit = null,
        [Description("Defaults to the currently open game.")] string? gameId = null,
        CancellationToken cancellationToken = default) =>
        SafeInvestContext.GuardAsync(async () =>
        {
            GameSession session = await context.RequireGameAsync(gameId, cancellationToken).ConfigureAwait(false);

            return new
            {
                Ok = true,
                GameId = session.Id.ToString(),
                Currency = session.Currency,
                TotalTrades = session.Trades.Count,
                RealizedPnL = session.RealizedPnL,
                Trades = session.Trades
                    .OrderByDescending(t => t.Timestamp)
                    .Take(Math.Clamp(limit ?? 25, 1, 500))
                    .Select(Contracts.ToRow)
                    .ToList(),
            };
        });

    [McpServerTool(Name = "get_market_sources")]
    [Description("Health of each market data source: whether it has an API key, whether its " +
                 "last call worked, and which ones invent prices. Use it when a quote looks wrong.")]
    public Task<string> GetMarketSourcesAsync() =>
        SafeInvestContext.GuardAsync(() => Task.FromResult<object>(new
        {
            Ok = true,
            Sources = context.MarketData.GetProviderStatuses(),
        }));

    internal async Task<Contracts.PortfolioResponse> BuildPortfolioAsync(
        GameSession session,
        CancellationToken cancellationToken)
    {
        PortfolioSnapshot snapshot = await context.SnapshotAsync(session, cancellationToken).ConfigureAwait(false);

        return new Contracts.PortfolioResponse
        {
            GameId = session.Id.ToString(),
            PlayerName = session.PlayerName,
            PlayerKind = session.PlayerKind.ToString(),
            Currency = session.Currency,
            Cash = snapshot.Cash,
            StartingCash = snapshot.StartingCash,
            InvestedValue = snapshot.MarketValue,
            TotalValue = snapshot.TotalValue,
            TotalPnL = snapshot.TotalPnL,
            TotalPnLPercent = snapshot.TotalPnLPercent,
            RealizedPnL = snapshot.RealizedPnL,
            UnrealizedPnL = snapshot.UnrealizedPnL,
            Direction = Contracts.Direction(snapshot.Direction),
            Positions = [.. snapshot.Positions.Select(Contracts.ToRow)],
            Goal = Contracts.ToResponse(context.Goal(session, snapshot)),
            UnpricedSymbols = snapshot.UnpricedSymbols,
            ContainsSimulatedPrices = snapshot.ContainsSimulatedPrices,
            AsOf = snapshot.AsOf,
        };
    }

    private static DateTimeOffset? ParseDeadline(string? value)
    {
        if (string.IsNullOrWhiteSpace(value))
        {
            return null;
        }

        if (DateTimeOffset.TryParse(
                value,
                System.Globalization.CultureInfo.InvariantCulture,
                System.Globalization.DateTimeStyles.AssumeUniversal | System.Globalization.DateTimeStyles.AdjustToUniversal,
                out DateTimeOffset parsed))
        {
            return parsed;
        }

        throw new SafeInvestToolException(
            $"Date illisible : « {value} ».",
            "Utilisez le format ISO-8601, par exemple 2027-06-30.");
    }
}
