using SafeInvest.Core.Engine;
using SafeInvest.Core.Models;
using Xunit;

namespace SafeInvest.Core.Tests;

public class PortfolioEngineTests
{
    [Fact]
    public void Buy_by_quantity_moves_cash_and_opens_a_position()
    {
        GameSession session = TestData.Session(startingCash: 10_000m);
        PortfolioEngine engine = new(TestData.Clock());

        Trade trade = engine.Buy(session, TestData.Bitcoin, TestData.Price(TestData.Bitcoin, 50_000m), quantity: 0.1m);

        Assert.Equal(TradeSide.Buy, trade.Side);
        Assert.Equal(0.1m, trade.Quantity);
        Assert.Equal(5_000m, trade.Total);
        Assert.Equal(5_000m, session.Cash);

        Holding holding = Assert.Single(session.Holdings);
        Assert.Equal(0.1m, holding.Quantity);
        Assert.Equal(50_000m, holding.AverageCost);
    }

    [Fact]
    public void Buy_by_amount_never_spends_more_than_the_amount_given()
    {
        GameSession session = TestData.Session(startingCash: 1_000m, feePercent: 1m);
        PortfolioEngine engine = new(TestData.Clock());

        Trade trade = engine.Buy(session, TestData.Bitcoin, TestData.Price(TestData.Bitcoin, 37_123.45m), amount: 500m);

        Assert.True(trade.Total <= 500m, $"Total {trade.Total} dépasse le montant demandé de 500.");
        Assert.True(session.Cash >= 500m);
        Assert.True(trade.Fees > 0m);
    }

    [Fact]
    public void Buy_averages_the_cost_across_several_purchases()
    {
        GameSession session = TestData.Session(startingCash: 10_000m);
        PortfolioEngine engine = new(TestData.Clock());

        engine.Buy(session, TestData.Microsoft, TestData.Price(TestData.Microsoft, 100m), quantity: 10m);
        engine.Buy(session, TestData.Microsoft, TestData.Price(TestData.Microsoft, 200m), quantity: 10m);

        Holding holding = Assert.Single(session.Holdings);
        Assert.Equal(20m, holding.Quantity);
        Assert.Equal(150m, holding.AverageCost);
        Assert.Equal(7_000m, session.Cash);
    }

    [Fact]
    public void Buy_is_refused_when_cash_is_short()
    {
        GameSession session = TestData.Session(startingCash: 100m);
        PortfolioEngine engine = new(TestData.Clock());

        TradeValidationException ex = Assert.Throws<TradeValidationException>(() =>
            engine.Buy(session, TestData.Bitcoin, TestData.Price(TestData.Bitcoin, 50_000m), quantity: 1m));

        Assert.Contains("Trésorerie insuffisante", ex.Message, StringComparison.Ordinal);
        Assert.Equal(100m, session.Cash);
        Assert.Empty(session.Holdings);
    }

    [Fact]
    public void Sell_realises_the_gain_and_credits_the_cash()
    {
        GameSession session = TestData.Session(startingCash: 10_000m);
        PortfolioEngine engine = new(TestData.Clock());
        engine.Buy(session, TestData.Microsoft, TestData.Price(TestData.Microsoft, 100m), quantity: 10m);

        Trade sale = engine.Sell(session, TestData.Microsoft, TestData.Price(TestData.Microsoft, 150m), quantity: 10m);

        Assert.Equal(500m, sale.RealizedPnL);
        Assert.Equal(1_500m, sale.Total);
        Assert.Equal(10_500m, session.Cash);
        Assert.Empty(session.Holdings);
    }

    [Fact]
    public void Sell_all_closes_the_position_without_leaving_dust()
    {
        GameSession session = TestData.Session(startingCash: 10_000m);
        PortfolioEngine engine = new(TestData.Clock());
        engine.Buy(session, TestData.Bitcoin, TestData.Price(TestData.Bitcoin, 31_337m), amount: 1_000m);

        engine.Sell(session, TestData.Bitcoin, TestData.Price(TestData.Bitcoin, 31_337m), sellAll: true);

        Assert.Empty(session.Holdings);
    }

    [Fact]
    public void Sell_is_refused_beyond_the_quantity_held()
    {
        GameSession session = TestData.Session(startingCash: 10_000m);
        PortfolioEngine engine = new(TestData.Clock());
        engine.Buy(session, TestData.Microsoft, TestData.Price(TestData.Microsoft, 100m), quantity: 5m);

        Assert.Throws<TradeValidationException>(() =>
            engine.Sell(session, TestData.Microsoft, TestData.Price(TestData.Microsoft, 100m), quantity: 6m));
    }

    [Fact]
    public void Sell_is_refused_when_nothing_is_held()
    {
        GameSession session = TestData.Session();
        PortfolioEngine engine = new(TestData.Clock());

        Assert.Throws<TradeValidationException>(() =>
            engine.Sell(session, TestData.Bitcoin, TestData.Price(TestData.Bitcoin, 50_000m), quantity: 1m));
    }

    [Fact]
    public void Ai_trades_must_carry_a_rationale()
    {
        GameSession session = TestData.Session(PlayerKind.Ai);
        PortfolioEngine engine = new(TestData.Clock());

        TradeValidationException ex = Assert.Throws<TradeValidationException>(() =>
            engine.Buy(session, TestData.Bitcoin, TestData.Price(TestData.Bitcoin, 50_000m), quantity: 0.01m));

        Assert.Contains("justification", ex.Message, StringComparison.Ordinal);
    }

    [Fact]
    public void Ai_trades_keep_the_rationale_in_the_history()
    {
        GameSession session = TestData.Session(PlayerKind.Ai);
        PortfolioEngine engine = new(TestData.Clock());

        Trade trade = engine.Buy(
            session,
            TestData.Bitcoin,
            TestData.Price(TestData.Bitcoin, 50_000m),
            quantity: 0.01m,
            rationale: "  Diversification sur la crypto la plus liquide.  ");

        Assert.Equal("Diversification sur la crypto la plus liquide.", trade.Rationale);
        Assert.Equal(PlayerKind.Ai, trade.ActorKind);
        Assert.Single(session.Trades);
    }

    [Fact]
    public void Human_trades_may_omit_the_rationale()
    {
        GameSession session = TestData.Session();
        PortfolioEngine engine = new(TestData.Clock());

        Trade trade = engine.Buy(session, TestData.Bitcoin, TestData.Price(TestData.Bitcoin, 50_000m), quantity: 0.01m);

        Assert.Null(trade.Rationale);
    }

    [Fact]
    public void A_quote_in_another_currency_is_refused()
    {
        GameSession session = TestData.Session();
        PortfolioEngine engine = new(TestData.Clock());
        Quote dollars = TestData.Price(TestData.Bitcoin, 50_000m) with { Currency = "USD" };

        Assert.Throws<TradeValidationException>(() =>
            engine.Buy(session, TestData.Bitcoin, dollars, quantity: 0.01m));
    }

    [Fact]
    public void A_quote_for_another_asset_is_refused()
    {
        GameSession session = TestData.Session();
        PortfolioEngine engine = new(TestData.Clock());

        Assert.Throws<TradeValidationException>(() =>
            engine.Buy(session, TestData.Bitcoin, TestData.Price(TestData.Microsoft, 400m), quantity: 1m));
    }

    [Fact]
    public void The_provenance_of_the_price_is_recorded_on_the_trade()
    {
        GameSession session = TestData.Session();
        PortfolioEngine engine = new(TestData.Clock());

        Trade trade = engine.Buy(
            session,
            TestData.Bitcoin,
            TestData.Price(TestData.Bitcoin, 50_000m, simulated: true),
            quantity: 0.01m);

        Assert.True(trade.QuoteWasSimulated);
        Assert.Equal("simulated", trade.QuoteSourceId);
    }
}
