using System.Collections.ObjectModel;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using SafeInvest.App.Services;
using SafeInvest.App.ViewModels;
using SafeInvest.Core.Models;
using SafeInvest.MarketData;

namespace SafeInvest.App.Views;

/// <summary>
/// Where a human player browses and trades. In an AI game the same screen turns into an
/// observation deck: the prices are still there, the buttons are not.
/// </summary>
public sealed partial class MarketPage : Page
{
    private readonly GameSessionService _session = AppServices.Get<GameSessionService>();
    private AssetKind? _filter;

    public MarketPage()
    {
        InitializeComponent();
        AssetsList.ItemsSource = Assets;
        Loaded += OnLoaded;
    }

    public ObservableCollection<MarketItem> Assets { get; } = [];

    private bool IsReadOnly => _session.IsAiGame;

    private async void OnLoaded(object sender, RoutedEventArgs e)
    {
        if (IsReadOnly)
        {
            TitleText.Text = "Observation du marché";
            ObservationBar.IsOpen = true;
        }

        await LoadAsync(string.Empty);
    }

    private async void OnSearchSubmitted(AutoSuggestBox sender, AutoSuggestBoxQuerySubmittedEventArgs args) =>
        await LoadAsync(args.QueryText ?? string.Empty);

    private async void OnFilterClick(object sender, RoutedEventArgs e)
    {
        _filter = sender switch
        {
            _ when ReferenceEquals(sender, CryptoChip) => AssetKind.Crypto,
            _ when ReferenceEquals(sender, StockChip) => AssetKind.Stock,
            _ when ReferenceEquals(sender, EtfChip) => AssetKind.Etf,
            _ => null,
        };

        AllChip.IsChecked = _filter is null;
        CryptoChip.IsChecked = _filter == AssetKind.Crypto;
        StockChip.IsChecked = _filter == AssetKind.Stock;
        EtfChip.IsChecked = _filter == AssetKind.Etf;

        await LoadAsync(SearchBox.Text ?? string.Empty);
    }

    /// <summary>
    /// Lists assets and prices them in one batch. Quoting the whole list at once is what
    /// keeps the free API tiers viable — one call for twelve cryptos, not twelve calls.
    /// </summary>
    private async Task LoadAsync(string query)
    {
        LoadingRing.IsActive = true;
        MessageBar.IsOpen = false;

        try
        {
            IReadOnlyList<Asset> assets = string.IsNullOrWhiteSpace(query)
                ? (_filter is null ? AssetCatalog.All : AssetCatalog.OfKind(_filter.Value))
                : await _session.MarketData.SearchAsync(query, _filter, limit: 20);

            IReadOnlyDictionary<string, Quote> quotes = assets.Count == 0
                ? new Dictionary<string, Quote>()
                : await _session.MarketData.GetQuotesAsync(assets, _session.Currency);

            Assets.Clear();
            foreach (Asset asset in assets)
            {
                decimal held = _session.Session?.FindHolding(asset.Kind, asset.Symbol)?.Quantity ?? 0m;
                Assets.Add(new MarketItem(asset, quotes.GetValueOrDefault(asset.Key), _session.Currency, held));
            }

            if (Assets.Count == 0)
            {
                Show(InfoBarSeverity.Informational, "Aucun résultat", $"Rien ne correspond à « {query} ».");
            }
            else if (Assets.Any(a => a.IsSimulated))
            {
                Show(
                    InfoBarSeverity.Warning,
                    "Cours simulés",
                    "Certains prix sont générés localement faute de source disponible. Ils ne reflètent aucun marché réel.");
            }
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            Show(InfoBarSeverity.Error, "Impossible de charger les cours", ex.Message);
        }
        finally
        {
            LoadingRing.IsActive = false;
        }
    }

    private async void OnTradeClick(object sender, RoutedEventArgs e)
    {
        if (sender is not FrameworkElement { Tag: string symbol })
        {
            return;
        }

        MarketItem? item = Assets.FirstOrDefault(a => a.Symbol == symbol);
        if (item is null)
        {
            return;
        }

        if (IsReadOnly)
        {
            Show(
                InfoBarSeverity.Informational,
                "Partie pilotée par une IA",
                "Les opérations sont passées par l'IA via MCP. Créez une partie « une personne » pour investir vous-même.");
            return;
        }

        TradeDialog dialog = new(_session, item.Asset) { XamlRoot = XamlRoot };
        await dialog.ShowAsync();

        if (dialog.TradeWasMade)
        {
            await LoadAsync(SearchBox.Text ?? string.Empty);
        }
    }

    private void Show(InfoBarSeverity severity, string title, string message)
    {
        MessageBar.Severity = severity;
        MessageBar.Title = title;
        MessageBar.Message = message;
        MessageBar.IsOpen = true;
    }
}
