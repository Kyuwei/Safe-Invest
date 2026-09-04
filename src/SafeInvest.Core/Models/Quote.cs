using System.Text.Json.Serialization;

namespace SafeInvest.Core.Models;

/// <summary>
/// A price observation for one asset. <see cref="SourceId"/> and <see cref="IsSimulated"/>
/// travel with every quote so the UI can always tell the user where a number came from —
/// an educational app must never pass an invented price off as a real one.
/// </summary>
public sealed record Quote
{
    public required string Symbol { get; init; }

    public required AssetKind Kind { get; init; }

    public required decimal Price { get; init; }

    public required string Currency { get; init; }

    public required DateTimeOffset AsOf { get; init; }

    public required string SourceId { get; init; }

    public bool IsSimulated { get; init; }

    public string? Name { get; init; }

    public decimal? Change24h { get; init; }

    public decimal? ChangePercent24h { get; init; }

    public decimal? MarketCap { get; init; }

    public decimal? Volume24h { get; init; }

    [JsonIgnore]
    public string Key => Asset.MakeKey(Kind, Symbol);

    /// <summary>+1 when the asset rose over 24 h, -1 when it fell, 0 when flat or unknown.</summary>
    [JsonIgnore]
    public int Direction => ChangePercent24h switch
    {
        null => 0,
        > 0 => 1,
        < 0 => -1,
        _ => 0,
    };
}
