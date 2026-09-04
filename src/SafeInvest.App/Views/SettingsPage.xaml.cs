using System.Collections.ObjectModel;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Navigation;
using SafeInvest.App.Services;
using SafeInvest.App.ViewModels;
using SafeInvest.Core.Storage;
using SafeInvest.MarketData;

namespace SafeInvest.App.Views;

/// <summary>
/// Where the player chooses which market data sources to use, pastes any API keys, and
/// tunes the display. Reachable both from the start menu and from inside a game, hence
/// the conditional back button.
/// </summary>
public sealed partial class SettingsPage : Page
{
    private readonly SettingsService _settingsService = AppServices.Get<SettingsService>();
    private AppSettings _settings = AppServices.Get<AppSettings>();
    private bool _standalone;

    public SettingsPage()
    {
        InitializeComponent();
        SourcesList.ItemsSource = Sources;
        Loaded += OnLoaded;
    }

    public ObservableCollection<SourceItem> Sources { get; } = [];

    protected override void OnNavigatedTo(NavigationEventArgs e)
    {
        base.OnNavigatedTo(e);

        _standalone = e.Parameter as string == "standalone";
        BackButton.Visibility = _standalone ? Visibility.Visible : Visibility.Collapsed;
    }

    private void OnLoaded(object sender, RoutedEventArgs e)
    {
        _settings = AppServices.Get<AppSettings>();

        SelectByTag(CryptoSourceBox, _settings.CryptoProviderOrder.FirstOrDefault() ?? "coingecko");
        SelectByTag(StockSourceBox, _settings.StockProviderOrder.FirstOrDefault() ?? "yahoo");
        SelectByTag(ThemeBox, _settings.Theme);

        SimulatedSwitch.IsOn = _settings.ForceSimulatedMode;
        ColorBlindSwitch.IsOn = _settings.ColorBlindPalette;
        RefreshBox.Value = _settings.RefreshIntervalSeconds;

        // Keys are write-only from here: showing a stored secret back has no use and only
        // creates a way to leak it over someone's shoulder.
        CoinGeckoKeyBox.PlaceholderText = Placeholder("coingecko", "Laisser vide pour l'accès public");
        CoinMarketCapKeyBox.PlaceholderText = Placeholder("coinmarketcap", "Laisser vide pour ne pas utiliser cette source");
        FinnhubKeyBox.PlaceholderText = Placeholder("finnhub", "Laisser vide pour ne pas utiliser cette source");

        DataFolderText.Text = SafeInvestPaths.Root;

        RefreshSources();
    }

    private string Placeholder(string providerId, string emptyText) =>
        _settingsService.GetApiKey(_settings, providerId) is null ? emptyText : "Clé enregistrée — saisir pour remplacer";

    private void RefreshSources()
    {
        Sources.Clear();
        foreach (ProviderStatus status in AppServices.Get<IMarketDataService>().GetProviderStatuses())
        {
            Sources.Add(new SourceItem(status));
        }
    }

    private async void OnTestSourcesClick(object sender, RoutedEventArgs e)
    {
        Show(InfoBarSeverity.Informational, "Vérification en cours", "Interrogation de chaque source…");

        try
        {
            IMarketDataService marketData = AppServices.Get<IMarketDataService>();

            // One crypto and one share exercise both chains in a single pass.
            await marketData.GetQuotesAsync(
                [AssetCatalog.All.First(a => a.Kind == SafeInvest.Core.Models.AssetKind.Crypto),
                 AssetCatalog.All.First(a => a.Kind == SafeInvest.Core.Models.AssetKind.Stock)],
                _settings.DefaultCurrency);

            RefreshSources();
            Show(InfoBarSeverity.Success, "Vérification terminée", "L'état de chaque source est à jour ci-dessus.");
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            RefreshSources();
            Show(InfoBarSeverity.Error, "La vérification a échoué", ex.Message);
        }
    }

    private async void OnSaveClick(object sender, RoutedEventArgs e)
    {
        try
        {
            string crypto = TagOf(CryptoSourceBox) ?? "coingecko";
            string stock = TagOf(StockSourceBox) ?? "yahoo";

            // The chosen source moves to the front; the rest keeps its order behind it, so
            // the fallbacks (and ultimately the simulator) are never lost.
            _settings.CryptoProviderOrder = Reorder(_settings.CryptoProviderOrder, crypto, ["coingecko", "coinmarketcap", "scraper", "simulated"]);
            _settings.StockProviderOrder = Reorder(_settings.StockProviderOrder, stock, ["yahoo", "finnhub", "scraper", "simulated"]);

            _settings.ForceSimulatedMode = SimulatedSwitch.IsOn;
            _settings.ColorBlindPalette = ColorBlindSwitch.IsOn;
            _settings.Theme = TagOf(ThemeBox) ?? "Default";
            _settings.RefreshIntervalSeconds = (int)RefreshBox.Value;

            StoreKey("coingecko", CoinGeckoKeyBox);
            StoreKey("coinmarketcap", CoinMarketCapKeyBox);
            StoreKey("finnhub", FinnhubKeyBox);

            await _settingsService.SaveAsync(_settings);

            // Rebuild the provider chain so a new key takes effect without a restart.
            AppServices.ReloadMarketData();
            PaletteService.Apply(_settings.ColorBlindPalette);
            PaletteService.ApplyTheme(_settings.Theme);

            RefreshSources();
            Show(InfoBarSeverity.Success, "Réglages enregistrés", "Les nouvelles sources sont actives immédiatement.");
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            Show(InfoBarSeverity.Error, "Enregistrement impossible", ex.Message);
        }
    }

    private void StoreKey(string providerId, PasswordBox box)
    {
        if (!string.IsNullOrWhiteSpace(box.Password))
        {
            _settingsService.SetApiKey(_settings, providerId, box.Password);
            box.Password = string.Empty;
            box.PlaceholderText = "Clé enregistrée — saisir pour remplacer";
        }
    }

    private static List<string> Reorder(IEnumerable<string> current, string first, IReadOnlyList<string> fallback)
    {
        List<string> order = [.. current];
        foreach (string id in fallback)
        {
            if (!order.Contains(id))
            {
                order.Add(id);
            }
        }

        order.Remove(first);
        order.Insert(0, first);
        return order;
    }

    private async void OnOpenFolderClick(object sender, RoutedEventArgs e)
    {
        SafeInvestPaths.EnsureCreated();
        await Windows.System.Launcher.LaunchFolderPathAsync(SafeInvestPaths.Root);
    }

    private void OnBackClick(object sender, RoutedEventArgs e)
    {
        if (Frame.CanGoBack)
        {
            Frame.GoBack();
        }
    }

    private static void SelectByTag(ComboBox box, string tag)
    {
        foreach (object item in box.Items)
        {
            if (item is ComboBoxItem { Tag: string candidate } && candidate == tag)
            {
                box.SelectedItem = item;
                return;
            }
        }

        box.SelectedIndex = 0;
    }

    private static string? TagOf(ComboBox box) =>
        box.SelectedItem is ComboBoxItem { Tag: string tag } ? tag : null;

    private void Show(InfoBarSeverity severity, string title, string message)
    {
        StatusBar.Severity = severity;
        StatusBar.Title = title;
        StatusBar.Message = message;
        StatusBar.IsOpen = true;
    }
}
