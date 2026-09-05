using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media.Animation;
using Microsoft.UI.Xaml.Navigation;
using SafeInvest.App.Services;
using SafeInvest.Core.Models;

namespace SafeInvest.App.Views;

/// <summary>
/// The in-game shell. Its header carries the two things a player always wants in view —
/// who is playing and what the portfolio is worth right now — while the pane switches
/// between the dashboard, the market, the history and the settings.
/// </summary>
public sealed partial class ShellPage : Page
{
    private readonly GameSessionService _session = AppServices.Get<GameSessionService>();

    public ShellPage()
    {
        InitializeComponent();
        Loaded += OnLoaded;
        Unloaded += OnUnloaded;
    }

    private void OnLoaded(object sender, RoutedEventArgs e)
    {
        _session.Attach(DispatcherQueue);
        _session.Changed += OnSessionChanged;
        _session.StartAutoRefresh();

        // An AI game is watched, not played: the market screen becomes an observation deck.
        if (_session.IsAiGame)
        {
            MarketItem.Content = "Observation";
        }

        Navigation.SelectedItem = Navigation.MenuItems[0];
        UpdateHeader();

        _ = _session.RefreshAsync();
    }

    private void OnUnloaded(object sender, RoutedEventArgs e)
    {
        _session.Changed -= OnSessionChanged;
        _session.StopAutoRefresh();
    }

    private void OnSessionChanged(object? sender, EventArgs e) => UpdateHeader();

    private void UpdateHeader()
    {
        GameSession? game = _session.Session;
        if (game is null)
        {
            return;
        }

        PlayerBadgeText.Text = Formatting.PlayerLabel(game.PlayerKind);
        PlayerBadge.Background = Converters.PaletteLookup.Brush(
            game.PlayerKind == PlayerKind.Ai ? "SafeInvestAiBrush" : "SafeInvestHumanBrush");

        PlayerNameText.Text = game.PlayerName;
        RefreshRing.IsActive = _session.IsRefreshing;

        PortfolioSnapshot? snapshot = _session.Snapshot;
        if (snapshot is null)
        {
            ValueText.Text = string.Empty;
            return;
        }

        ValueText.Text =
            $"{Formatting.Money(snapshot.TotalValue, game.Currency)}  " +
            $"({Formatting.MoneySigned(snapshot.TotalPnL, game.Currency)})";
        ValueText.Foreground = Converters.PaletteLookup.Brush(snapshot.Direction switch
        {
            > 0 => "SafeInvestUpBrush",
            < 0 => "SafeInvestDownBrush",
            _ => "SafeInvestFlatBrush",
        });

        // Being explicit about invented prices matters more here than a tidy header.
        SourceNoteText.Text = snapshot.ContainsSimulatedPrices
            ? "Cours simulés"
            : _session.LastError is null ? string.Empty : "Cours partiellement indisponibles";
    }

    private void OnNavigationSelectionChanged(NavigationView sender, NavigationViewSelectionChangedEventArgs args)
    {
        if (args.SelectedItem is not NavigationViewItem { Tag: string tag })
        {
            return;
        }

        Type target = tag switch
        {
            "market" => typeof(MarketPage),
            "history" => typeof(HistoryPage),
            "settings" => typeof(SettingsPage),
            _ => typeof(DashboardPage),
        };

        if (ContentFrame.CurrentSourcePageType != target)
        {
            ContentFrame.Navigate(target, null, new DrillInNavigationTransitionInfo());
        }
    }

    private void OnRefreshClick(object sender, RoutedEventArgs e) => _ = _session.RefreshAsync();

    private void OnBackToMenuClick(object sender, RoutedEventArgs e)
    {
        _session.StopAutoRefresh();
        Frame.Navigate(typeof(StartPage), null, new EntranceNavigationTransitionInfo());
    }

    protected override void OnNavigatedFrom(NavigationEventArgs e)
    {
        base.OnNavigatedFrom(e);
        _session.StopAutoRefresh();
    }
}
