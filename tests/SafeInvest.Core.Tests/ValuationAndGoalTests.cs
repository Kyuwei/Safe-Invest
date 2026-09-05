using SafeInvest.Core.Engine;
using SafeInvest.Core.Models;
using Xunit;

namespace SafeInvest.Core.Tests;

public class ValuationServiceTests
{
    [Fact]
    public void An_untouched_portfolio_is_worth_exactly_its_starting_cash()
    {
        GameSession session = TestData.Session(startingCash: 10_000m);

        PortfolioSnapshot snapshot = ValuationService.Create(session, TestData.Quotes(), TestData.Origin);

        Assert.Equal(10_000m, snapshot.TotalValue);
        Assert.Equal(0m, snapshot.TotalPnL);
        Assert.Equal(0m, snapshot.MarketValue);
        Assert.Empty(snapshot.Positions);
    }

    [Fact]
    public void A_position_that_rose_shows_a_positive_direction()
    {
        GameSession session = TestData.Session(startingCash: 10_000m);
        PortfolioEngine engine = new(TestData.Clock());
        engine.Buy(session, TestData.Microsoft, TestData.Price(TestData.Microsoft, 100m), quantity: 10m);

        PortfolioSnapshot snapshot = ValuationService.Create(
            session,
            TestData.Quotes(TestData.Price(TestData.Microsoft, 130m, changePercent: 3m)),
            TestData.Origin);

        PositionView position = Assert.Single(snapshot.Positions);
        Assert.Equal(1_300m, position.MarketValue);
        Assert.Equal(300m, position.UnrealizedPnL);
        Assert.Equal(30m, position.UnrealizedPnLPercent);
        Assert.Equal(1, position.Direction);
        Assert.Equal(10_300m, snapshot.TotalValue);
        Assert.Equal(1, snapshot.Direction);
    }

    [Fact]
    public void A_position_that_fell_shows_a_negative_direction()
    {
        GameSession session = TestData.Session(startingCash: 10_000m);
        PortfolioEngine engine = new(TestData.Clock());
        engine.Buy(session, TestData.Microsoft, TestData.Price(TestData.Microsoft, 100m), quantity: 10m);

        PortfolioSnapshot snapshot = ValuationService.Create(
            session,
            TestData.Quotes(TestData.Price(TestData.Microsoft, 60m)),
            TestData.Origin);

        Assert.Equal(-1, Assert.Single(snapshot.Positions).Direction);
        Assert.Equal(-400m, snapshot.TotalPnL);
        Assert.Equal(-1, snapshot.Direction);
    }

    [Fact]
    public void An_asset_with_no_quote_is_reported_rather_than_valued_at_zero()
    {
        GameSession session = TestData.Session(startingCash: 10_000m);
        PortfolioEngine engine = new(TestData.Clock());
        engine.Buy(session, TestData.Microsoft, TestData.Price(TestData.Microsoft, 100m), quantity: 10m);

        PortfolioSnapshot snapshot = ValuationService.Create(session, TestData.Quotes(), TestData.Origin);

        Assert.Equal("MSFT", Assert.Single(snapshot.UnpricedSymbols));
        Assert.Null(Assert.Single(snapshot.Positions).MarketValue);
        Assert.Equal(0m, snapshot.MarketValue);
    }

    [Fact]
    public void A_simulated_price_is_flagged_all_the_way_up_to_the_snapshot()
    {
        GameSession session = TestData.Session(startingCash: 10_000m);
        PortfolioEngine engine = new(TestData.Clock());
        engine.Buy(session, TestData.Bitcoin, TestData.Price(TestData.Bitcoin, 50_000m), quantity: 0.1m);

        PortfolioSnapshot snapshot = ValuationService.Create(
            session,
            TestData.Quotes(TestData.Price(TestData.Bitcoin, 51_000m, simulated: true)),
            TestData.Origin);

        Assert.True(snapshot.ContainsSimulatedPrices);
        Assert.True(Assert.Single(snapshot.Positions).IsSimulated);
    }

    [Fact]
    public void Weights_are_computed_against_the_total_value_cash_included()
    {
        GameSession session = TestData.Session(startingCash: 10_000m);
        PortfolioEngine engine = new(TestData.Clock());
        engine.Buy(session, TestData.Microsoft, TestData.Price(TestData.Microsoft, 100m), quantity: 25m);

        PortfolioSnapshot snapshot = ValuationService.Create(
            session,
            TestData.Quotes(TestData.Price(TestData.Microsoft, 100m)),
            TestData.Origin);

        Assert.Equal(25m, Assert.Single(snapshot.Positions).WeightPercent);
    }
}

public class GoalTrackerTests
{
    [Fact]
    public void No_goal_means_no_progress_object()
    {
        GameSession session = TestData.Session();
        PortfolioSnapshot snapshot = ValuationService.Create(session, TestData.Quotes(), TestData.Origin);

        Assert.Null(GoalTracker.Evaluate(session, snapshot, TestData.Origin));
    }

    [Fact]
    public void Progress_is_measured_from_the_starting_cash_not_from_zero()
    {
        GameSession session = TestData.Session(startingCash: 10_000m);
        session.Goal = new Goal { TargetAmount = 20_000m, Deadline = TestData.Origin.AddYears(1) };
        session.Cash = 15_000m;

        PortfolioSnapshot snapshot = ValuationService.Create(session, TestData.Quotes(), TestData.Origin);
        GoalProgress progress = GoalTracker.Evaluate(session, snapshot, TestData.Origin)!;

        Assert.Equal(50m, progress.ProgressPercent);
        Assert.Equal(5_000m, progress.AmountRemaining);
    }

    [Fact]
    public void Reaching_the_target_marks_the_goal_achieved()
    {
        GameSession session = TestData.Session(startingCash: 10_000m);
        session.Goal = new Goal { TargetAmount = 12_000m, Deadline = TestData.Origin.AddYears(1) };
        session.Cash = 12_500m;

        PortfolioSnapshot snapshot = ValuationService.Create(session, TestData.Quotes(), TestData.Origin);
        GoalProgress progress = GoalTracker.Evaluate(session, snapshot, TestData.Origin)!;

        Assert.Equal(GoalStatus.Achieved, progress.Status);
        Assert.Equal(100m, progress.ProgressPercent);
        Assert.Equal(0m, progress.AmountRemaining);
    }

    [Fact]
    public void Passing_the_deadline_short_of_the_target_marks_it_expired()
    {
        GameSession session = TestData.Session(startingCash: 10_000m);
        session.Goal = new Goal { TargetAmount = 20_000m, Deadline = TestData.Origin.AddDays(30) };
        session.Cash = 11_000m;

        PortfolioSnapshot snapshot = ValuationService.Create(session, TestData.Quotes(), TestData.Origin.AddDays(60));
        GoalProgress progress = GoalTracker.Evaluate(session, snapshot, TestData.Origin.AddDays(60))!;

        Assert.Equal(GoalStatus.Expired, progress.Status);
        Assert.Equal(0, progress.DaysRemaining);
    }

    [Fact]
    public void Growing_faster_than_the_goal_requires_reads_as_on_track()
    {
        GameSession session = TestData.Session(startingCash: 10_000m);
        session.Goal = new Goal { TargetAmount = 12_000m, Deadline = TestData.Origin.AddYears(1) };
        session.Cash = 11_500m;

        DateTimeOffset now = TestData.Origin.AddDays(90);
        PortfolioSnapshot snapshot = ValuationService.Create(session, TestData.Quotes(), now);

        Assert.Equal(GoalStatus.OnTrack, GoalTracker.Evaluate(session, snapshot, now)!.Status);
    }

    [Fact]
    public void Lagging_behind_the_required_pace_reads_as_behind()
    {
        GameSession session = TestData.Session(startingCash: 10_000m);
        session.Goal = new Goal { TargetAmount = 20_000m, Deadline = TestData.Origin.AddYears(1) };
        session.Cash = 10_050m;

        DateTimeOffset now = TestData.Origin.AddDays(180);
        PortfolioSnapshot snapshot = ValuationService.Create(session, TestData.Quotes(), now);

        Assert.Equal(GoalStatus.Behind, GoalTracker.Evaluate(session, snapshot, now)!.Status);
    }

    [Fact]
    public void Doubling_in_one_year_needs_about_a_hundred_percent_a_year()
    {
        decimal? rate = GoalTracker.Annualised(from: 10_000m, to: 20_000m, years: 1d);

        Assert.NotNull(rate);
        Assert.InRange(rate!.Value, 99.9m, 100.1m);
    }

    [Fact]
    public void An_annualised_rate_over_a_span_shorter_than_a_day_is_not_reported()
    {
        Assert.Null(GoalTracker.Annualised(from: 10_000m, to: 11_000m, years: 0d));
    }
}
