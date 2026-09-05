using System.Globalization;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using SafeInvest.App.Services;
using SafeInvest.Core.Engine;
using SafeInvest.Core.Models;
using SafeInvest.Core.Storage;

namespace SafeInvest.App.Views;

/// <summary>
/// The "new game" form: who plays, with how much, and — mainly for an AI player — what
/// target by what date. The goal preview turns an abstract number into the yearly return
/// it would actually demand, which is the fastest way to show a beginner that "10 000 €
/// to a million in a year" is not a plan.
/// </summary>
public sealed partial class NewGamePage : Page
{
    private bool _isAi;

    public NewGamePage()
    {
        InitializeComponent();

        GoalDatePicker.Date = DateTimeOffset.Now.AddYears(1);
        GoalDatePicker.MinDate = DateTimeOffset.Now.AddDays(1);
        GoalDatePicker.MaxDate = DateTimeOffset.Now.AddYears(30);

        AppSettings settings = AppServices.Get<AppSettings>();
        StartingCashBox.Value = (double)settings.DefaultStartingCash;
        FeeBox.Value = (double)settings.DefaultFeePercent;

        UpdateGoalPreview();
    }

    private void OnBackClick(object sender, RoutedEventArgs e)
    {
        if (Frame.CanGoBack)
        {
            Frame.GoBack();
        }
    }

    private void OnPlayerKindChanged(object sender, RoutedEventArgs e)
    {
        _isAi = ReferenceEquals(sender, AiToggle);

        HumanToggle.IsChecked = !_isAi;
        AiToggle.IsChecked = _isAi;

        PlayerNameBox.PlaceholderText = _isAi ? "Claude" : "Alice";

        // A goal is what an AI player is briefed with, so offer it by default there.
        if (_isAi && !GoalSwitch.IsOn)
        {
            GoalSwitch.IsOn = true;
        }
    }

    private void OnPresetAmountClick(object sender, RoutedEventArgs e)
    {
        if (sender is FrameworkElement { Tag: string tag }
            && double.TryParse(tag, NumberStyles.Float, CultureInfo.InvariantCulture, out double amount))
        {
            StartingCashBox.Value = amount;

            // Keep the goal a stretch rather than something already reached.
            if (double.IsNaN(GoalAmountBox.Value) || GoalAmountBox.Value <= amount)
            {
                GoalAmountBox.Value = amount * 1.5d;
            }

            UpdateGoalPreview();
        }
    }

    private void OnGoalToggled(object sender, RoutedEventArgs e)
    {
        GoalPanel.Visibility = GoalSwitch.IsOn ? Visibility.Visible : Visibility.Collapsed;
        UpdateGoalPreview();
    }

    private void OnCurrencyChanged(object sender, SelectionChangedEventArgs e) => UpdateGoalPreview();

    private void OnGoalDateChanged(CalendarDatePicker sender, CalendarDatePickerDateChangedEventArgs args) =>
        UpdateGoalPreview();

    private void OnFormValueChanged(NumberBox sender, NumberBoxValueChangedEventArgs args) => UpdateGoalPreview();

    /// <summary>
    /// Restates the goal as the compound annual return it implies. "+42 %/an" means
    /// something; "atteindre 15 000 € en 2027" on its own does not.
    /// </summary>
    private void UpdateGoalPreview()
    {
        if (GoalPreviewText is null || !GoalSwitch.IsOn)
        {
            return;
        }

        decimal start = ReadAmount(StartingCashBox);
        decimal target = ReadAmount(GoalAmountBox);
        DateTimeOffset deadline = GoalDatePicker.Date ?? DateTimeOffset.Now.AddYears(1);
        double years = Math.Max((deadline - DateTimeOffset.Now).TotalDays, 0d) / 365.25d;

        if (start <= 0m)
        {
            GoalPreviewText.Text = "Renseignez d'abord un capital de départ.";
            return;
        }

        if (target <= 0m)
        {
            GoalPreviewText.Text = "Renseignez le montant que vous voulez atteindre.";
            return;
        }

        if (target <= start)
        {
            GoalPreviewText.Text = "Le montant à atteindre doit dépasser le capital de départ, sinon l'objectif est déjà rempli.";
            return;
        }

        decimal? rate = GoalTracker.Annualised(start, target, years);
        string currency = SelectedCurrency();
        string horizon = years >= 1d
            ? $"{years:N1} an(s)"
            : $"{Math.Max((deadline - DateTimeOffset.Now).TotalDays, 0d):N0} jour(s)";

        if (rate is null)
        {
            GoalPreviewText.Text = "Choisissez une date limite plus éloignée : sur moins d'une journée, l'objectif n'a pas de sens.";
            return;
        }

        string verdict = rate.Value switch
        {
            < 8m => "C'est à la portée d'un marché calme.",
            < 20m => "C'est ambitieux : proche des meilleures années des indices boursiers.",
            < 60m => "C'est très ambitieux : peu de gestionnaires y parviennent durablement.",
            _ => "C'est irréaliste sans une prise de risque considérable — et donc un risque de tout perdre.",
        };

        GoalPreviewText.Text =
            $"Passer de {Formatting.Money(start, currency)} à {Formatting.Money(target, currency)} en {horizon} " +
            $"demande environ {Formatting.Percent(rate)} par an. {verdict}";
    }

    /// <summary>
    /// A NumberBox reports double.NaN while its field is empty, and casting that to decimal
    /// throws. Every read of a NumberBox goes through here.
    /// </summary>
    private static decimal ReadAmount(NumberBox box) =>
        double.IsNaN(box.Value) || double.IsInfinity(box.Value) ? 0m : (decimal)box.Value;

    private string SelectedCurrency() =>
        CurrencyBox.SelectedItem is ComboBoxItem { Tag: string code } ? code : "EUR";

    private async void OnStartClick(object sender, RoutedEventArgs e)
    {
        ErrorBar.IsOpen = false;

        try
        {
            GameSessionService session = AppServices.Get<GameSessionService>();

            decimal? goalAmount = GoalSwitch.IsOn ? ReadAmount(GoalAmountBox) : null;
            DateTimeOffset? goalDeadline = GoalSwitch.IsOn
                ? (GoalDatePicker.Date ?? DateTimeOffset.Now.AddYears(1))
                : null;

            await session.CreateAsync(
                playerName: PlayerNameBox.Text,
                playerKind: _isAi ? PlayerKind.Ai : PlayerKind.Human,
                startingCash: ReadAmount(StartingCashBox),
                currency: SelectedCurrency(),
                feePercent: ReadAmount(FeeBox),
                goalAmount: goalAmount,
                goalDeadline: goalDeadline);

            Frame.Navigate(typeof(ShellPage));
        }
        catch (Exception ex) when (ex is ArgumentException or InvalidOperationException)
        {
            ErrorBar.Title = "Impossible de démarrer la partie";
            ErrorBar.Message = ex.Message;
            ErrorBar.IsOpen = true;
        }
    }
}
