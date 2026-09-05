using Microsoft.UI.Xaml;

namespace SafeInvest.App.Services;

/// <summary>
/// Chooses between the green/red palette and a blue/orange one that stays legible with
/// the common forms of colour blindness. Roughly one man in twelve cannot reliably
/// separate the usual green from the usual red, which on a financial screen means losing
/// the single most important signal.
///
/// The swap is a key lookup, not a mutation: every brush exists in both variants inside
/// the theme dictionaries, and <c>PaletteLookup</c> picks which one to read. That keeps
/// the light and dark themes working, which mutating shared brush objects would not.
/// </summary>
internal static class PaletteService
{
    private static readonly string[] SwappableKeys =
    [
        "SafeInvestUpBrush",
        "SafeInvestDownBrush",
        "SafeInvestUpSoftBrush",
        "SafeInvestDownSoftBrush",
    ];

    public static bool IsColourBlindPaletteActive { get; private set; }

    public static void Apply(bool colourBlind) => IsColourBlindPaletteActive = colourBlind;

    /// <summary>Maps a brush key to the variant the current palette calls for.</summary>
    public static string Resolve(string brushKey) =>
        IsColourBlindPaletteActive && Array.IndexOf(SwappableKeys, brushKey) >= 0
            ? brushKey.Replace("Brush", "AltBrush", StringComparison.Ordinal)
            : brushKey;

    public static void ApplyTheme(string theme)
    {
        if (App.RootWindow?.Content is FrameworkElement root)
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
