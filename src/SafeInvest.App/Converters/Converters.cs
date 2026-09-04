using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Data;
using Microsoft.UI.Xaml.Media;
using SafeInvest.Core.Models;

namespace SafeInvest.App.Converters;

/// <summary>Looks a brush up in the app resources, falling back to a neutral grey.</summary>
internal static class PaletteLookup
{
    public static Brush Brush(string key) =>
        Application.Current.Resources.TryGetValue(key, out object? found) && found is Brush brush
            ? brush
            : new SolidColorBrush(Microsoft.UI.Colors.Gray);
}

/// <summary>
/// Turns a direction (+1 up, -1 down, 0 flat) into the green/red the whole app reads by.
/// Everything colour-coded goes through this one converter so the meaning never drifts
/// between screens.
/// </summary>
public sealed partial class DirectionToBrushConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language) =>
        PaletteLookup.Brush(ToDirection(value) switch
        {
            > 0 => "SafeInvestUpBrush",
            < 0 => "SafeInvestDownBrush",
            _ => "SafeInvestFlatBrush",
        });

    public object ConvertBack(object value, Type targetType, object parameter, string language) =>
        throw new NotSupportedException();

    internal static int ToDirection(object? value) => value switch
    {
        int direction => Math.Sign(direction),
        decimal amount => Math.Sign(amount),
        double amount => Math.Sign(amount),
        null => 0,
        _ => 0,
    };
}

/// <summary>The pale background used behind a coloured badge.</summary>
public sealed partial class DirectionToSoftBrushConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language) =>
        PaletteLookup.Brush(DirectionToBrushConverter.ToDirection(value) switch
        {
            > 0 => "SafeInvestUpSoftBrush",
            < 0 => "SafeInvestDownSoftBrush",
            _ => "SafeInvestCardBorderBrush",
        });

    public object ConvertBack(object value, Type targetType, object parameter, string language) =>
        throw new NotSupportedException();
}

/// <summary>An arrow that says the same thing as the colour, for anyone who cannot see it.</summary>
public sealed partial class DirectionToGlyphConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language) =>
        DirectionToBrushConverter.ToDirection(value) switch
        {
            > 0 => "\uE70E",
            < 0 => "\uE70D",
            _ => "\uE738",
        };

    public object ConvertBack(object value, Type targetType, object parameter, string language) =>
        throw new NotSupportedException();
}

/// <summary>Gives each asset family its own colour, so a list reads at a glance.</summary>
public sealed partial class AssetKindToBrushConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language) =>
        PaletteLookup.Brush(value switch
        {
            AssetKind.Crypto => "SafeInvestCryptoBrush",
            AssetKind.Stock => "SafeInvestStockBrush",
            AssetKind.Etf => "SafeInvestEtfBrush",
            _ => "SafeInvestMutedBrush",
        });

    public object ConvertBack(object value, Type targetType, object parameter, string language) =>
        throw new NotSupportedException();
}

public sealed partial class BoolToVisibilityConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language) =>
        value is true ? Visibility.Visible : Visibility.Collapsed;

    public object ConvertBack(object value, Type targetType, object parameter, string language) =>
        value is Visibility.Visible;
}

public sealed partial class InvertedBoolToVisibilityConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language) =>
        value is true ? Visibility.Collapsed : Visibility.Visible;

    public object ConvertBack(object value, Type targetType, object parameter, string language) =>
        value is Visibility.Collapsed;
}

public sealed partial class NullToVisibilityConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language) =>
        value is null ? Visibility.Collapsed : Visibility.Visible;

    public object ConvertBack(object value, Type targetType, object parameter, string language) =>
        throw new NotSupportedException();
}

/// <summary>Hides a block when its text is empty — used for optional AI comments.</summary>
public sealed partial class StringToVisibilityConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language) =>
        string.IsNullOrWhiteSpace(value as string) ? Visibility.Collapsed : Visibility.Visible;

    public object ConvertBack(object value, Type targetType, object parameter, string language) =>
        throw new NotSupportedException();
}

/// <summary>Colours the goal ring: green when on track, amber when behind, red when missed.</summary>
public sealed partial class GoalStatusToBrushConverter : IValueConverter
{
    public object Convert(object value, Type targetType, object parameter, string language) =>
        PaletteLookup.Brush(value switch
        {
            GoalStatus.Achieved or GoalStatus.OnTrack => "SafeInvestUpBrush",
            GoalStatus.Behind => "SafeInvestWarningBrush",
            GoalStatus.Expired => "SafeInvestDownBrush",
            _ => "SafeInvestMutedBrush",
        });

    public object ConvertBack(object value, Type targetType, object parameter, string language) =>
        throw new NotSupportedException();
}
