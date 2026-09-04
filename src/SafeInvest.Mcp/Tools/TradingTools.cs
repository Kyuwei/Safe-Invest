using System.ComponentModel;
using ModelContextProtocol.Server;
using SafeInvest.Core.Models;

namespace SafeInvest.Mcp.Tools;

/// <summary>Buying and selling. Every AI trade must come with a stated reason.</summary>
[McpServerToolType]
internal sealed class TradingTools(SafeInvestContext context, GameTools gameTools)
{
    [McpServerTool(Name = "buy")]
    [Description("Buys an asset with the game's fictional cash, at the current market price. " +
                 "Give either quantity (exact units) or amount (spend this much cash, fees " +
                 "included) — not both. In an AI game the rationale is mandatory: it is shown " +
                 "next to the trade in the app so a learner can follow the reasoning.")]
    public Task<string> BuyAsync(
        [Description("Ticker to buy, e.g. \"BTC\" or \"MSFT\".")]
        string symbol,
        [Description("Why this purchase, in one or two sentences. Required for AI players.")]
        string? rationale = null,
        [Description("Exact number of units to buy. Fractions are allowed.")]
        decimal? quantity = null,
        [Description("Cash to spend, fees included. Buys as many units as it covers.")]
        decimal? amount = null,
        [Description("Asset family: \"crypto\" (default), \"stock\" or \"etf\".")]
        string? kind = null,
        [Description("Defaults to the currently open game.")]
        string? gameId = null,
        CancellationToken cancellationToken = default) =>
        SafeInvestContext.GuardAsync(async () =>
        {
            RequireExactlyOneSize(quantity, amount, allowAll: false, all: false);

            (GameSession session, Asset asset, Quote quote) =
                await PrepareAsync(symbol, kind, gameId, cancellationToken).ConfigureAwait(false);

            Trade? executed = null;
            GameSession updated = await context.Store.MutateAsync(
                session.Id,
                s => executed = context.Engine.Buy(s, asset, quote, quantity, amount, rationale),
                cancellationToken).ConfigureAwait(false);

            return await BuildTradeResponseAsync(updated, executed!, quote, cancellationToken).ConfigureAwait(false);
        });

    [McpServerTool(Name = "sell")]
    [Description("Sells all or part of a position at the current market price. Give quantity, " +
                 "or amount to raise a sum, or all=true to close the position. In an AI game " +
                 "the rationale is mandatory.")]
    public Task<string> SellAsync(
        [Description("Ticker to sell, e.g. \"BTC\".")]
        string symbol,
        [Description("Why this sale, in one or two sentences. Required for AI players.")]
        string? rationale = null,
        [Description("Exact number of units to sell.")]
        decimal? quantity = null,
        [Description("Cash to raise; sells roughly enough units to cover it.")]
        decimal? amount = null,
        [Description("Set true to close the whole position.")]
        bool? all = null,
        [Description("Asset family: \"crypto\" (default), \"stock\" or \"etf\".")]
        string? kind = null,
        [Description("Defaults to the currently open game.")]
        string? gameId = null,
        CancellationToken cancellationToken = default) =>
        SafeInvestContext.GuardAsync(async () =>
        {
            bool sellAll = all ?? false;
            RequireExactlyOneSize(quantity, amount, allowAll: true, all: sellAll);

            (GameSession session, Asset asset, Quote quote) =
                await PrepareAsync(symbol, kind, gameId, cancellationToken).ConfigureAwait(false);

            Trade? executed = null;
            GameSession updated = await context.Store.MutateAsync(
                session.Id,
                s => executed = context.Engine.Sell(s, asset, quote, quantity, amount, sellAll, rationale),
                cancellationToken).ConfigureAwait(false);

            return await BuildTradeResponseAsync(updated, executed!, quote, cancellationToken).ConfigureAwait(false);
        });

    /// <summary>
    /// Loads the game, resolves the ticker and gets a price in the game's currency — the
    /// three things both trades need before touching the store.
    /// </summary>
    private async Task<(GameSession Session, Asset Asset, Quote Quote)> PrepareAsync(
        string symbol,
        string? kind,
        string? gameId,
        CancellationToken cancellationToken)
    {
        GameSession session = await context.RequireGameAsync(gameId, cancellationToken).ConfigureAwait(false);
        AssetKind assetKind = SafeInvestContext.ParseKind(kind);

        // Prefer the asset already held, so a sale reuses the exact instrument bought.
        Asset asset = session.FindHolding(assetKind, symbol)?.Asset
                      ?? await context.ResolveAssetAsync(symbol, assetKind, cancellationToken).ConfigureAwait(false);

        Quote quote = await context.RequireQuoteAsync(asset, session.Currency, cancellationToken).ConfigureAwait(false);

        return (session, asset, quote);
    }

    private async Task<Contracts.TradeResponse> BuildTradeResponseAsync(
        GameSession session,
        Trade trade,
        Quote quote,
        CancellationToken cancellationToken)
    {
        Contracts.PortfolioResponse portfolio = await gameTools
            .BuildPortfolioAsync(session, cancellationToken)
            .ConfigureAwait(false);

        return new Contracts.TradeResponse
        {
            Trade = Contracts.ToRow(trade),
            CashAfter = portfolio.Cash,
            TotalValueAfter = portfolio.TotalValue,
            Currency = session.Currency,
            Goal = portfolio.Goal,
            Warning = quote.IsSimulated
                ? "Cette opération a été passée à un cours simulé : aucune source réelle n'était joignable."
                : null,
        };
    }

    private static void RequireExactlyOneSize(decimal? quantity, decimal? amount, bool allowAll, bool all)
    {
        int provided = (quantity is not null ? 1 : 0) + (amount is not null ? 1 : 0) + (all ? 1 : 0);

        if (provided == 1)
        {
            return;
        }

        string options = allowAll ? "quantity, amount ou all" : "quantity ou amount";

        throw new SafeInvestToolException(
            provided == 0
                ? $"Précisez la taille de l'opération : {options}."
                : $"Précisez une seule taille d'opération parmi {options}.");
    }
}
