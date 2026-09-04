using System.Collections.ObjectModel;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using SafeInvest.App.Converters;
using SafeInvest.App.Services;
using SafeInvest.App.ViewModels;
using SafeInvest.Core.Models;

namespace SafeInvest.App.Views;

/// <summary>
/// The main screen: what the portfolio is worth, how each line is doing, and how far the
/// game is from its goal. In an AI game it also carries the decision feed — every move the
/// AI made with the reason it gave, which is what makes the mode worth watching.
/// </summary>
public sealed partial class DashboardPage : Page
{
    private const int AiFeedLength = 8;

    private readonly GameSessionService _session = AppServices.Get<GameSessionService>();

    public DashboardPage()
    {
        InitializeComponent();

        PositionsList.ItemsSource = Positions;
        AiFeedList.ItemsSource = AiFeed;

        Loaded += OnLoaded;
        Unloaded += OnUnloaded;
    }

    public ObservableCollection<PositionItem> Positions { get; } = [];

    public ObservableCollection<TradeItem> AiFeed { get; } = [];

    private void OnLoaded(object sender, RoutedEventArgs e)
    {
        _session.Changed += OnSessionChanged;
        Render();
    }

    private void OnUnloaded(object sender, RoutedEventArgs e) => _session.Changed -= OnSessionChanged;

    private void OnSessionChanged(object? sender, EventArgs e) => Render();

    private void Render()
    {
        GameSession? game = _session.Session;
        PortfolioSnapshot? snapshot = _session.Snapshot;

        if (game is null || snapshot is null)
        {
            return;
        }

        string currency = game.Currency;

        TotalValueText.Text = Formatting.Money(snapshot.TotalValue, currency);
        TotalValueText.Foreground = PaletteLookup.Brush(snapshot.Direction switch
        {
            > 0 => "SafeInvestUpBrush",
            < 0 => "SafeInvestDownBrush",
            _ => "SafeInvestFlatBrush",
        });

        PnLText.Text = Formatting.MoneySigned(snapshot.TotalPnL, currency);
        PnLPercentText.Text = $"({Formatting.Percent(snapshot.TotalPnLPercent)})";
        PnLGlyph.Text = snapshot.Direction switch { > 0 => "\uE70E", < 0 => "\uE70D", _ => "\uE738" };

        Microsoft.UI.Xaml.Media.Brush pnlBrush = PaletteLookup.Brush(snapshot.Direction switch
        {
            > 0 => "SafeInvestUpBrush",
            < 0 => "SafeInvestDownBrush",
            _ => "SafeInvestFlatBrush",
        });
        PnLText.Foreground = pnlBrush;
        PnLPercentText.Foreground = pnlBrush;
        PnLGlyph.Foreground = pnlBrush;

        CashText.Text = Formatting.Money(snapshot.Cash, currency);
        InvestedText.Text = Formatting.Money(snapshot.MarketValue, currency);
        RealizedText.Text = Formatting.MoneySigned(snapshot.RealizedPnL, currency);
        RealizedText.Foreground = PaletteLookup.Brush(Math.Sign(snapshot.RealizedPnL) switch
        {
            > 0 => "SafeInvestUpBrush",
            < 0 => "SafeInvestDownBrush",
            _ => "SafeInvestFlatBrush",
        });

        SimulatedBar.IsOpen = snapshot.ContainsSimulatedPrices;

        RenderGoal(game, currency);
        RenderPositions(snapshot, currency);
        RenderAiFeed(game, currency);
    }

    private void RenderGoal(GameSession game, string currency)
    {
        GoalProgress? goal = _session.Goal;

        if (goal is null)
        {
            GoalCard.Visibility = Visibility.Collapsed;
            return;
        }

        GoalCard.Visibility = Visibility.Visible;
        GoalRing.Value = (double)goal.ProgressPercent;
        GoalRing.Foreground = PaletteLookup.Brush(goal.Status switch
        {
            GoalStatus.Achieved or GoalStatus.OnTrack => "SafeInvestUpBrush",
            GoalStatus.Behind => "SafeInvestWarningBrush",
            GoalStatus.Expired => "SafeInvestDownBrush",
            _ => "SafeInvestMutedBrush",
        });

        GoalPercentText.Text = $"{goal.ProgressPercent:N0} %";
        GoalStatusText.Text = Formatting.GoalStatusLabel(goal.Status);
        GoalStatusText.Foreground = GoalRing.Foreground;

        GoalTargetText.Text = goal.Status == GoalStatus.Achieved
            ? $"Objectif de {Formatting.Money(goal.TargetAmount, currency)} atteint."
            : $"Il manque {Formatting.Money(goal.AmountRemaining, currency)} pour atteindre " +
              $"{Formatting.Money(goal.TargetAmount, currency)} avant le {Formatting.Date(goal.Deadline)} — " +
              Formatting.Countdown(goal.DaysRemaining) + ".";

        // Stating the pace in %/an is what makes a goal legible rather than just a number.
        GoalPaceText.Text = (goal.RequiredAnnualisedReturnPercent, goal.AchievedAnnualisedReturnPercent) switch
        {
            (null, null) => string.Empty,
            (var required, null) => $"Rendement encore nécessaire : {Formatting.Percent(required)} par an.",
            (null, var achieved) => $"Rendement obtenu jusqu'ici : {Formatting.Percent(achieved)} par an.",
            var (required, achieved) =>
                $"Rendement obtenu jusqu'ici : {Formatting.Percent(achieved)} par an · " +
                $"encore nécessaire : {Formatting.Percent(required)} par an.",
        };
    }

    private void RenderPositions(PortfolioSnapshot snapshot, string currency)
    {
        Positions.Clear();
        foreach (PositionView position in snapshot.Positions.OrderByDescending(p => p.MarketValue ?? 0m))
        {
            Positions.Add(new PositionItem(position, currency));
        }

        PositionCountText.Text = Positions.Count switch
        {
            0 => string.Empty,
            1 => "1 ligne",
            _ => $"{Positions.Count} lignes",
        };

        bool empty = Positions.Count == 0;
        EmptyPortfolioCard.Visibility = empty ? Visibility.Visible : Visibility.Collapsed;

        if (empty)
        {
            EmptyPortfolioTitle.Text = "Rien en portefeuille pour l'instant.";
            EmptyPortfolioText.Text = _session.IsAiGame
                ? "L'IA n'a encore rien acheté. Dès qu'elle passe une opération, elle apparaît ici avec sa justification."
                : "Rendez-vous dans l'onglet Marché pour choisir une première crypto, action ou ETF.";
        }
    }

    private void RenderAiFeed(GameSession game, string currency)
    {
        if (game.PlayerKind != PlayerKind.Ai)
        {
            AiFeedSection.Visibility = Visibility.Collapsed;
            return;
        }

        AiFeedSection.Visibility = game.Trades.Count > 0 ? Visibility.Visible : Visibility.Collapsed;

        DateTimeOffset now = DateTimeOffset.UtcNow;
        AiFeed.Clear();

        foreach (Trade trade in game.Trades.OrderByDescending(t => t.Timestamp).Take(AiFeedLength))
        {
            AiFeed.Add(new TradeItem(trade, currency, now));
        }
    }
}
