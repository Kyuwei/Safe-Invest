using System.Globalization;
using SafeInvest.Core.Models;

namespace SafeInvest.App.Services;

/// <summary>
/// Every number the user sees is formatted here rather than in XAML. It keeps French
/// conventions (space as thousands separator, comma as decimal mark) consistent across
/// screens, and lets the view models expose ready-to-display strings — which is one less
/// thing that can go wrong in a binding.
/// </summary>
internal static class Formatting
{
    private static readonly CultureInfo French = CultureInfo.GetCultureInfo("fr-FR");

    public static string Money(decimal amount, string currency) =>
        $"{amount.ToString("N2", French)} {Symbol(currency)}";

    public static string MoneySigned(decimal amount, string currency) =>
        $"{(amount > 0 ? "+" : string.Empty)}{amount.ToString("N2", French)} {Symbol(currency)}";

    public static string Percent(decimal? value, bool signed = true)
    {
        if (value is null)
        {
            return "—";
        }

        string sign = signed && value.Value > 0 ? "+" : string.Empty;
        return $"{sign}{value.Value.ToString("N2", French)} %";
    }

    /// <summary>Crypto needs more decimals than shares; trailing zeros are noise.</summary>
    public static string Quantity(decimal quantity)
    {
        decimal rounded = Math.Round(quantity, 8, MidpointRounding.ToZero);
        string text = rounded.ToString("0.########", French);
        return text.Length == 0 ? "0" : text;
    }

    public static string UnitPrice(decimal price, string currency) =>
        price < 1m
            ? $"{price.ToString("0.######", French)} {Symbol(currency)}"
            : Money(price, currency);

    public static string DateTime(DateTimeOffset moment) =>
        moment.ToLocalTime().ToString("dd/MM/yyyy HH:mm", French);

    public static string Date(DateTimeOffset moment) =>
        moment.ToLocalTime().ToString("dd MMMM yyyy", French);

    public static string ShortDate(DateTimeOffset moment) =>
        moment.ToLocalTime().ToString("dd/MM/yyyy", French);

    public static string RelativeTime(DateTimeOffset moment, DateTimeOffset now)
    {
        TimeSpan elapsed = now - moment;

        return elapsed switch
        {
            { TotalSeconds: < 60 } => "à l'instant",
            { TotalMinutes: < 60 } => $"il y a {(int)elapsed.TotalMinutes} min",
            { TotalHours: < 24 } => $"il y a {(int)elapsed.TotalHours} h",
            { TotalDays: < 30 } => $"il y a {(int)elapsed.TotalDays} j",
            _ => ShortDate(moment),
        };
    }

    public static string Symbol(string currency) => currency?.ToUpperInvariant() switch
    {
        "EUR" => "€",
        "USD" => "$",
        "GBP" => "£",
        "CHF" => "CHF",
        "JPY" => "¥",
        _ => currency ?? string.Empty,
    };

    public static string KindLabel(AssetKind kind) => kind switch
    {
        AssetKind.Crypto => "Crypto",
        AssetKind.Stock => "Action",
        AssetKind.Etf => "ETF",
        _ => kind.ToString(),
    };

    public static string SideLabel(TradeSide side) =>
        side == TradeSide.Buy ? "Achat" : "Vente";

    public static string PlayerLabel(PlayerKind kind) =>
        kind == PlayerKind.Ai ? "IA" : "Humain";

    public static string GoalStatusLabel(GoalStatus status) => status switch
    {
        GoalStatus.Achieved => "Objectif atteint",
        GoalStatus.OnTrack => "Dans les temps",
        GoalStatus.Behind => "En retard sur l'objectif",
        GoalStatus.Expired => "Échéance dépassée",
        _ => "Aucun objectif",
    };

    /// <summary>Turns "il reste 483 jours" into something a beginner can picture.</summary>
    public static string Countdown(int days) => days switch
    {
        <= 0 => "échéance passée",
        1 => "il reste 1 jour",
        < 60 => $"il reste {days} jours",
        < 730 => $"il reste {days} jours (environ {days / 30} mois)",
        _ => $"il reste {days} jours (environ {days / 365} ans)",
    };

    public static string SourceLabel(string? sourceId) => sourceId switch
    {
        "coingecko" => "CoinGecko",
        "coinmarketcap" => "CoinMarketCap",
        "yahoo" => "Yahoo Finance",
        "finnhub" => "Finnhub",
        "scraper" => "repli web",
        "simulated" => "cours simulé",
        null => "source inconnue",
        _ => sourceId,
    };
}
