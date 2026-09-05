using System.Collections.ObjectModel;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using SafeInvest.App.Services;
using SafeInvest.App.ViewModels;
using SafeInvest.Core.Models;

namespace SafeInvest.App.Views;

/// <summary>
/// Every buy and sell, newest first, with the reason attached. For an AI game this page
/// is the transcript of the reasoning — which is exactly what a learner is meant to read.
/// </summary>
public sealed partial class HistoryPage : Page
{
    private readonly GameSessionService _session = AppServices.Get<GameSessionService>();

    public HistoryPage()
    {
        InitializeComponent();
        TradesList.ItemsSource = Trades;

        Loaded += OnLoaded;
        Unloaded += OnUnloaded;
    }

    public ObservableCollection<TradeItem> Trades { get; } = [];

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
        if (game is null)
        {
            return;
        }

        DateTimeOffset now = DateTimeOffset.UtcNow;

        Trades.Clear();
        foreach (Trade trade in game.Trades.OrderByDescending(t => t.Timestamp))
        {
            Trades.Add(new TradeItem(trade, game.Currency, now));
        }

        bool empty = Trades.Count == 0;
        EmptyCard.Visibility = empty ? Visibility.Visible : Visibility.Collapsed;
        EmptyHintText.Text = _session.IsAiGame
            ? "Dès que l'IA passera un ordre, il apparaîtra ici avec la raison qu'elle aura donnée."
            : "Vos achats et vos ventes s'afficheront ici, avec la date et le prix pratiqué.";

        int buys = game.Trades.Count(t => t.Side == TradeSide.Buy);
        int sells = game.Trades.Count - buys;

        SummaryText.Text = empty
            ? string.Empty
            : $"{buys} achat(s) · {sells} vente(s) · résultat réalisé " +
              Formatting.MoneySigned(game.RealizedPnL, game.Currency);
    }
}
