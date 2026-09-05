using Microsoft.UI.Text;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using SafeInvest.App.Controls;
using SafeInvest.App.Converters;
using SafeInvest.App.Services;
using SafeInvest.Core.Abstractions;
using SafeInvest.Core.Engine;
using SafeInvest.Core.Models;

namespace SafeInvest.App.Views;

/// <summary>
/// The buy/sell sheet. Built in code rather than XAML because it is one screen with a lot
/// of derived text — the estimated quantity, what is already held, what the cash allows —
/// and keeping that in one place makes it far easier to keep the numbers honest.
///
/// It doubles as the teaching moment: alongside the inputs it says, in plain French, what
/// the price means, what the 24-hour move means, and where the figure came from.
/// </summary>
internal sealed class TradeDialog : ContentDialog
{
    private readonly GameSessionService _session;
    private readonly Asset _asset;

    private readonly TextBlock _priceText = new() { FontSize = 30, FontWeight = FontWeights.Bold };
    private readonly TextBlock _changeText = new() { FontSize = 14 };
    private readonly TextBlock _sourceText = new() { FontSize = 12, Opacity = 0.75, TextWrapping = TextWrapping.Wrap };
    private readonly TextBlock _holdingText = new() { FontSize = 13, TextWrapping = TextWrapping.Wrap };
    private readonly TextBlock _estimateText = new() { FontSize = 13, TextWrapping = TextWrapping.Wrap };
    private readonly TextBlock _lessonText = new() { FontSize = 12, Opacity = 0.8, TextWrapping = TextWrapping.Wrap };
    private readonly SparklineControl _sparkline = new() { Height = 70 };
    private readonly NumberBox _amountBox = new() { Minimum = 0, SpinButtonPlacementMode = NumberBoxSpinButtonPlacementMode.Hidden };
    private readonly InfoBar _errorBar = new() { Severity = InfoBarSeverity.Error, IsOpen = false, IsClosable = false };
    private readonly ProgressRing _busy = new() { Width = 18, Height = 18, IsActive = true };

    private Quote? _quote;

    public TradeDialog(GameSessionService session, Asset asset)
    {
        _session = session;
        _asset = asset;

        Title = $"{asset.Symbol} — {asset.Name}";
        PrimaryButtonText = "Acheter";
        SecondaryButtonText = "Vendre";
        CloseButtonText = "Annuler";
        DefaultButton = ContentDialogButton.Primary;
        IsPrimaryButtonEnabled = false;
        IsSecondaryButtonEnabled = false;

        _amountBox.Header = $"Montant à investir ({Formatting.Symbol(session.Currency)})";
        _amountBox.ValueChanged += (_, _) => UpdateEstimate();

        Content = BuildContent();

        PrimaryButtonClick += OnBuy;
        SecondaryButtonClick += OnSell;
        Opened += async (_, _) => await LoadAsync();
    }

    /// <summary>True once an order went through, so the caller knows to refresh.</summary>
    public bool TradeWasMade { get; private set; }

    private UIElement BuildContent()
    {
        StackPanel panel = new() { Spacing = 14, Width = 420 };

        StackPanel priceRow = new() { Spacing = 8 };
        priceRow.Children.Add(_busy);
        priceRow.Children.Add(_priceText);
        priceRow.Children.Add(_changeText);
        priceRow.Children.Add(_sourceText);
        panel.Children.Add(priceRow);

        panel.Children.Add(_sparkline);
        panel.Children.Add(_holdingText);
        panel.Children.Add(_amountBox);

        StackPanel presets = new() { Orientation = Orientation.Horizontal, Spacing = 8 };
        foreach (decimal preset in new[] { 100m, 500m, 1_000m })
        {
            Button button = new() { Content = Formatting.Money(preset, _session.Currency), CornerRadius = new CornerRadius(8) };
            decimal captured = preset;
            button.Click += (_, _) => { _amountBox.Value = (double)captured; };
            presets.Children.Add(button);
        }

        Button allIn = new() { Content = "Tout l'argent disponible", CornerRadius = new CornerRadius(8) };
        allIn.Click += (_, _) => { _amountBox.Value = (double)(_session.Snapshot?.Cash ?? 0m); };
        presets.Children.Add(allIn);
        panel.Children.Add(presets);

        panel.Children.Add(_estimateText);

        Border lesson = new()
        {
            Background = PaletteLookup.Brush("SafeInvestCardBorderBrush"),
            CornerRadius = new CornerRadius(8),
            Padding = new Thickness(12, 9, 12, 9),
        };
        lesson.Child = _lessonText;
        panel.Children.Add(lesson);

        panel.Children.Add(_errorBar);

        return new ScrollViewer { Content = panel, MaxHeight = 620 };
    }

    private async Task LoadAsync()
    {
        try
        {
            _quote = await _session.QuoteAsync(_asset);

            if (_quote is null)
            {
                ShowError(
                    "Cours indisponible",
                    $"Aucune source n'a pu donner un prix pour {_asset.Symbol}. Vérifiez l'état des sources dans les Réglages.");
                return;
            }

            _priceText.Text = Formatting.UnitPrice(_quote.Price, _session.Currency);
            _priceText.Foreground = PaletteLookup.Brush(_quote.Direction switch
            {
                > 0 => "SafeInvestUpBrush",
                < 0 => "SafeInvestDownBrush",
                _ => "SafeInvestFlatBrush",
            });

            _changeText.Text = _quote.ChangePercent24h is null
                ? "variation 24 h inconnue"
                : $"{Formatting.Percent(_quote.ChangePercent24h)} sur 24 h";
            _changeText.Foreground = _priceText.Foreground;

            _sourceText.Text = _quote.IsSimulated
                ? "Cours simulé : aucune source réelle n'a répondu. Ce prix ne correspond à aucun marché."
                : $"Source : {Formatting.SourceLabel(_quote.SourceId)}, relevé {Formatting.DateTime(_quote.AsOf)}";
            _sourceText.Foreground = PaletteLookup.Brush(
                _quote.IsSimulated ? "SafeInvestWarningBrush" : "SafeInvestMutedBrush");

            UpdateHolding();
            UpdateLesson();
            UpdateEstimate();

            IsPrimaryButtonEnabled = true;
            IsSecondaryButtonEnabled = HeldQuantity > 0m;

            await LoadHistoryAsync();
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            ShowError("Impossible de charger le cours", ex.Message);
        }
        finally
        {
            _busy.IsActive = false;
        }
    }

    private async Task LoadHistoryAsync()
    {
        try
        {
            IReadOnlyList<Candle> candles = await _session.MarketData
                .GetHistoryAsync(_asset, _session.Currency, HistoryRange.Month);

            _sparkline.SetValues([.. candles.Select(c => c.Close)]);
            _sparkline.Visibility = candles.Count >= 2 ? Visibility.Visible : Visibility.Collapsed;
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            // A missing chart is not a reason to block a trade.
            _sparkline.Visibility = Visibility.Collapsed;
        }
    }

    private decimal HeldQuantity =>
        _session.Session?.FindHolding(_asset.Kind, _asset.Symbol)?.Quantity ?? 0m;

    /// <summary>
    /// A NumberBox reports double.NaN while its field is empty, and casting that to decimal
    /// throws. Empty simply means "no amount given" here.
    /// </summary>
    private decimal TypedAmount =>
        double.IsNaN(_amountBox.Value) || double.IsInfinity(_amountBox.Value)
            ? 0m
            : (decimal)_amountBox.Value;

    private void UpdateHolding()
    {
        decimal cash = _session.Snapshot?.Cash ?? 0m;
        decimal held = HeldQuantity;

        _holdingText.Text = held > 0m
            ? $"Vous détenez {Formatting.Quantity(held)} {_asset.Symbol}. " +
              $"Argent disponible : {Formatting.Money(cash, _session.Currency)}."
            : $"Argent disponible : {Formatting.Money(cash, _session.Currency)}.";
    }

    /// <summary>Restates the order in units, which is what actually enters the portfolio.</summary>
    private void UpdateEstimate()
    {
        if (_quote is null)
        {
            return;
        }

        decimal amount = TypedAmount;
        if (amount <= 0m)
        {
            _estimateText.Text = "Indiquez un montant à investir.";
            return;
        }

        decimal feeRate = (_session.Session?.FeePercent ?? 0m) / 100m;
        decimal units = MoneyMath.RoundQuantityDown(amount / (_quote.Price * (1m + feeRate)));
        decimal fees = MoneyMath.RoundMoney(units * _quote.Price * feeRate);

        string feeNote = fees > 0m ? $", dont {Formatting.Money(fees, _session.Currency)} de frais" : string.Empty;

        _estimateText.Text = units <= 0m
            ? "Ce montant est trop faible pour acheter la moindre fraction de cet actif."
            : $"≈ {Formatting.Quantity(units)} {_asset.Symbol} pour {Formatting.Money(amount, _session.Currency)}{feeNote}.";
    }

    /// <summary>The teaching line: what this number actually tells the player.</summary>
    private void UpdateLesson()
    {
        string volatility = _asset.Kind == AssetKind.Crypto
            ? "Les cryptomonnaies bougent beaucoup plus vite que les actions : une variation de 10 % en une journée y est banale."
            : "Une action représente une part d'entreprise ; son cours suit les résultats de celle-ci et l'humeur du marché.";

        string change = _quote?.ChangePercent24h switch
        {
            null => "La variation sur 24 h n'est pas disponible pour cet actif.",
            > 5m => "Cet actif a nettement monté ces dernières 24 h. Acheter après une hausse, c'est payer plus cher qu'hier.",
            < -5m => "Cet actif a nettement baissé ces dernières 24 h. Une baisse n'annonce ni un rebond ni une poursuite de la chute.",
            _ => "La variation des dernières 24 h est modérée.",
        };

        _lessonText.Text = $"{volatility} {change}";
    }

    private async void OnBuy(ContentDialog sender, ContentDialogButtonClickEventArgs args)
    {
        ContentDialogButtonClickDeferral deferral = args.GetDeferral();
        args.Cancel = true;

        try
        {
            decimal amount = TypedAmount;
            if (amount <= 0m)
            {
                ShowError("Montant manquant", "Indiquez le montant que vous voulez investir.");
                return;
            }

            await _session.BuyAsync(_asset, quantity: null, amount: amount, rationale: null);

            TradeWasMade = true;
            args.Cancel = false;
        }
        catch (TradeValidationException ex)
        {
            ShowError("Achat impossible", ex.Message);
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            ShowError("Achat impossible", ex.Message);
        }
        finally
        {
            deferral.Complete();
        }
    }

    private async void OnSell(ContentDialog sender, ContentDialogButtonClickEventArgs args)
    {
        ContentDialogButtonClickDeferral deferral = args.GetDeferral();
        args.Cancel = true;

        try
        {
            decimal amount = TypedAmount;

            // No amount typed means "close the position" — the common case when selling.
            if (amount <= 0m)
            {
                await _session.SellAsync(_asset, quantity: null, amount: null, sellAll: true, rationale: null);
            }
            else
            {
                await _session.SellAsync(_asset, quantity: null, amount: amount, sellAll: false, rationale: null);
            }

            TradeWasMade = true;
            args.Cancel = false;
        }
        catch (TradeValidationException ex)
        {
            ShowError("Vente impossible", ex.Message);
        }
        catch (Exception ex) when (ex is not OperationCanceledException)
        {
            ShowError("Vente impossible", ex.Message);
        }
        finally
        {
            deferral.Complete();
        }
    }

    private void ShowError(string title, string message)
    {
        _errorBar.Title = title;
        _errorBar.Message = message;
        _errorBar.IsOpen = true;
    }
}
