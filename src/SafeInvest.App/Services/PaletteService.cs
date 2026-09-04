using Microsoft.UI;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Media;
using Windows.UI;

namespace SafeInvest.App.Services;

/// <summary>
/// Swaps the up/down colours for a pair that stays distinguishable with the common forms
/// of colour blindness. Roughly one man in twelve cannot reliably separate the usual
/// green from the usual red, which on a financial screen means not being able to read the
/// single most important signal.
/// </summary>
internal static class PaletteService
{
    private static readonly Color ColourBlindUp = Color.FromArgb(255, 0x1D, 0x4E, 0xD8);
    private static readonly Color ColourBlindDown = Color.FromArgb(255, 0xC2, 0x41, 0x0C);
    private static readonly Color ColourBlindUpSoft = Color.FromArgb(255, 0xDB, 0xEA, 0xFE);
    private static readonly Color ColourBlindDownSoft = Color.FromArgb(255, 0xFF, 0xED, 0xD5);

    public static bool IsColourBlindPaletteActive { get; private set; }

    public static void Apply(bool colourBlind)
    {
        IsColourBlindPaletteActive = colourBlind;

        if (colourBlind)
        {
            Set("SafeInvestUpBrush", ColourBlindUp);
            Set("SafeInvestDownBrush", ColourBlindDown);
            Set("SafeInvestUpSoftBrush", ColourBlindUpSoft);
            Set("SafeInvestDownSoftBrush", ColourBlindDownSoft);
        }
        else
        {
            // Back to the theme's own values: re-reading the colour resource keeps light
            // and dark correct without hardcoding either here.
            Reset("SafeInvestUpBrush", "SafeInvestUpColor");
            Reset("SafeInvestDownBrush", "SafeInvestDownColor");
            Reset("SafeInvestUpSoftBrush", "SafeInvestUpSoftColor");
            Reset("SafeInvestDownSoftBrush", "SafeInvestDownSoftColor");
        }
    }

    private static void Set(string brushKey, Color colour)
    {
        if (Application.Current.Resources.TryGetValue(brushKey, out object? found)
            && found is SolidColorBrush brush)
        {
            brush.Color = colour;
        }
    }

    private static void Reset(string brushKey, string colourKey)
    {
        if (Application.Current.Resources.TryGetValue(colourKey, out object? colour) && colour is Color value)
        {
            Set(brushKey, value);
        }
        else
        {
            Set(brushKey, Colors.Gray);
        }
    }

    public static void ApplyTheme(string theme)
    {
        if (App.MainWindow?.Content is FrameworkElement root)
        {
            root.RequestedTheme = theme switch
            {
                "Light" => ElementTheme.Light,
                "Dark" => ElementTheme.Dark,
                _ => ElementTheme.Default,
            };
        }
    }
}
