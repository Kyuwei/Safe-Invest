using System.Collections.ObjectModel;
using System.ComponentModel;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Navigation;
using SafeInvest.App.Services;
using SafeInvest.App.ViewModels;
using SafeInvest.Core.Models;
using SafeInvest.Core.Storage;

namespace SafeInvest.App.Views;

/// <summary>
/// The menu the app opens on: start a game, pick up a saved one, or read the two-minute
/// primer. Deliberately the first thing a beginner sees, with no jargon on it.
/// </summary>
public sealed partial class StartPage : Page
{
    public StartPage()
    {
        ViewModel = new StartPageViewModel(AppServices.Get<IGameStore>());
        InitializeComponent();
    }

    public StartPageViewModel ViewModel { get; }

    protected override async void OnNavigatedTo(NavigationEventArgs e)
    {
        base.OnNavigatedTo(e);
        await ViewModel.LoadAsync();
    }

    private void OnNewGameClick(object sender, RoutedEventArgs e) =>
        Frame.Navigate(typeof(NewGamePage));

    private void OnSettingsClick(object sender, RoutedEventArgs e) =>
        Frame.Navigate(typeof(SettingsPage), "standalone");

    private async void OnHelpClick(object sender, RoutedEventArgs e)
    {
        ContentDialog dialog = new()
        {
            XamlRoot = XamlRoot,
            Title = "Comment ça marche ?",
            CloseButtonText = "J'ai compris",
            DefaultButton = ContentDialogButton.Close,
            Content = BuildPrimer(),
        };

        await dialog.ShowAsync();
    }

    private async void OnResumeGameClick(object sender, RoutedEventArgs e)
    {
        if (sender is not FrameworkElement { Tag: Guid id })
        {
            return;
        }

        GameSessionService session = AppServices.Get<GameSessionService>();
        if (await session.OpenAsync(id))
        {
            Frame.Navigate(typeof(ShellPage));
        }
    }

    /// <summary>
    /// The primer stays short on purpose. Someone who has never invested needs the shape
    /// of the thing, not a course — the rest is learned by playing.
    /// </summary>
    private static UIElement BuildPrimer()
    {
        StackPanel panel = new() { Spacing = 14, Width = 460 };

        void Section(string title, string body)
        {
            panel.Children.Add(new TextBlock
            {
                Text = title,
                FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
                FontSize = 15,
            });
            panel.Children.Add(new TextBlock
            {
                Text = body,
                TextWrapping = TextWrapping.Wrap,
                Opacity = 0.85,
            });
        }

        Section(
            "Vous recevez un capital fictif",
            "Vous choisissez le montant au début de la partie. Cet argent n'existe pas : rien ne peut être perdu.");

        Section(
            "Les cours, eux, sont réels",
            "Les prix des cryptos, actions et ETF viennent des mêmes sources que les vrais investisseurs. " +
            "Une position affichée en vert a pris de la valeur, en rouge elle en a perdu.");

        Section(
            "Acheter, c'est échanger",
            "Quand vous achetez, votre argent disponible baisse et une ligne apparaît dans le portefeuille. " +
            "Sa valeur bouge ensuite avec le marché. Vendre fait l'inverse et fige le gain ou la perte.");

        Section(
            "Une IA peut jouer à votre place",
            "En partie IA, c'est un assistant qui décide. L'application montre alors chaque opération avec " +
            "la raison qui l'a motivée : c'est fait pour être lu et discuté.");

        return panel;
    }
}

/// <summary>Backs the start menu: the list of saved games, newest first.</summary>
public sealed class StartPageViewModel : INotifyPropertyChanged
{
    private readonly IGameStore _store;
    private bool _hasNoGames = true;

    public StartPageViewModel(IGameStore store) => _store = store;

    public event PropertyChangedEventHandler? PropertyChanged;

    public ObservableCollection<GameCardItem> Games { get; } = [];

    public bool HasNoGames
    {
        get => _hasNoGames;
        private set
        {
            if (_hasNoGames == value)
            {
                return;
            }

            _hasNoGames = value;
            PropertyChanged?.Invoke(this, new PropertyChangedEventArgs(nameof(HasNoGames)));
        }
    }

    public async Task LoadAsync()
    {
        IReadOnlyList<GameSummary> games = await _store.ListAsync();
        DateTimeOffset now = DateTimeOffset.UtcNow;

        Games.Clear();
        foreach (GameSummary summary in games)
        {
            Games.Add(new GameCardItem(summary, now));
        }

        HasNoGames = Games.Count == 0;
    }
}
