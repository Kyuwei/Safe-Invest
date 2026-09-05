using System.Text.Json.Serialization;

namespace SafeInvest.Core.Models;

/// <summary>
/// An investable instrument. <see cref="Symbol"/> is the user-facing ticker (BTC, MSFT);
/// <see cref="ProviderId"/> carries the identifier a specific data provider needs
/// (CoinGecko, for instance, keys its API on "bitcoin" rather than "BTC").
/// </summary>
public sealed record Asset
{
    public required string Symbol { get; init; }

    public required string Name { get; init; }

    public required AssetKind Kind { get; init; }

    public string? ProviderId { get; init; }

    public string? LogoUrl { get; init; }

    /// <summary>Stable identity used as a dictionary key across the app.</summary>
    [JsonIgnore]
    public string Key => MakeKey(Kind, Symbol);

    public static string MakeKey(AssetKind kind, string symbol) =>
        $"{kind}:{Normalize(symbol)}";

    public static string Normalize(string symbol) =>
        symbol.Trim().ToUpperInvariant();
}
