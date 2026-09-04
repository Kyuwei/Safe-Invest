using SafeInvest.Core.Models;

namespace SafeInvest.Core.Engine;

/// <summary>Turns a session plus a set of quotes into the numbers the dashboard shows.</summary>
public static class ValuationService
{
    public static PortfolioSnapshot Create(
        GameSession session,
        IReadOnlyDictionary<string, Quote> quotes,
        DateTimeOffset asOf)
    {
        ArgumentNullException.ThrowIfNull(session);
        ArgumentNullException.ThrowIfNull(quotes);

        List<PositionView> positions = new(session.Holdings.Count);
        List<string> unpriced = [];
        decimal marketValue = 0m;
        decimal unrealized = 0m;
        bool simulated = false;

        foreach (Holding holding in session.Holdings)
        {
            quotes.TryGetValue(holding.Asset.Key, out Quote? quote);

            decimal costBasis = MoneyMath.RoundMoney(holding.CostBasis);
            decimal? value = quote is null ? null : MoneyMath.RoundMoney(holding.Quantity * quote.Price);
            decimal? pnl = value is null ? null : MoneyMath.RoundMoney(value.Value - costBasis);

            if (quote is null)
            {
                unpriced.Add(holding.Asset.Symbol);
            }
            else
            {
                marketValue += value!.Value;
                unrealized += pnl!.Value;
                simulated |= quote.IsSimulated;
            }

            positions.Add(new PositionView
            {
                Asset = holding.Asset,
                Quantity = holding.Quantity,
                AverageCost = holding.AverageCost,
                CostBasis = costBasis,
                Price = quote?.Price,
                MarketValue = value,
                UnrealizedPnL = pnl,
                UnrealizedPnLPercent = pnl is null ? null : MoneyMath.Percent(pnl.Value, costBasis),
                ChangePercent24h = quote?.ChangePercent24h,
                SourceId = quote?.SourceId,
                IsSimulated = quote?.IsSimulated ?? false,
                QuotedAt = quote?.AsOf,
            });
        }

        marketValue = MoneyMath.RoundMoney(marketValue);
        decimal totalValue = MoneyMath.RoundMoney(session.Cash + marketValue);
        decimal totalPnL = MoneyMath.RoundMoney(totalValue - session.StartingCash);

        // Weights need the total, so they are filled in on a second pass.
        List<PositionView> weighted = positions
            .Select(p => p with
            {
                WeightPercent = p.MarketValue is null || totalValue == 0m
                    ? 0m
                    : MoneyMath.Percent(p.MarketValue.Value, totalValue),
            })
            .ToList();

        return new PortfolioSnapshot
        {
            AsOf = asOf,
            Currency = session.Currency,
            Cash = session.Cash,
            StartingCash = session.StartingCash,
            MarketValue = marketValue,
            TotalValue = totalValue,
            TotalPnL = totalPnL,
            TotalPnLPercent = MoneyMath.Percent(totalPnL, session.StartingCash),
            RealizedPnL = MoneyMath.RoundMoney(session.RealizedPnL),
            UnrealizedPnL = MoneyMath.RoundMoney(unrealized),
            Positions = weighted,
            ContainsSimulatedPrices = simulated,
            UnpricedSymbols = unpriced,
        };
    }
}
