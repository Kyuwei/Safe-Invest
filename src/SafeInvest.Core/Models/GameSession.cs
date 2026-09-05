using System.Text.Json.Serialization;

namespace SafeInvest.Core.Models;

/// <summary>
/// A whole game: who plays, with how much fictional money, what they hold and
/// everything they have done. This is the object persisted to disk and shared
/// between the WinUI app and the MCP server.
/// </summary>
public sealed class GameSession
{
    public const int CurrentSchemaVersion = 1;

    public required Guid Id { get; init; }

    public required string PlayerName { get; set; }

    public required PlayerKind PlayerKind { get; init; }

    /// <summary>ISO 4217 code the whole session is denominated in, e.g. "EUR".</summary>
    public required string Currency { get; init; }

    public required decimal StartingCash { get; init; }

    public decimal Cash { get; set; }

    public List<Holding> Holdings { get; init; } = [];

    public List<Trade> Trades { get; init; } = [];

    public Goal? Goal { get; set; }

    /// <summary>Trading fee as a percentage of the gross amount. 0 disables fees.</summary>
    public decimal FeePercent { get; set; }

    public DateTimeOffset CreatedAt { get; init; }

    public DateTimeOffset UpdatedAt { get; set; }

    public int SchemaVersion { get; set; } = CurrentSchemaVersion;

    [JsonIgnore]
    public decimal RealizedPnL => Trades.Sum(t => t.RealizedPnL ?? 0m);

    public Holding? FindHolding(AssetKind kind, string symbol)
    {
        string key = Asset.MakeKey(kind, symbol);
        return Holdings.FirstOrDefault(h => h.Asset.Key == key);
    }
}

/// <summary>Lightweight row for the "resume a game" list, so we never load every file.</summary>
public sealed record GameSummary
{
    public required Guid Id { get; init; }

    public required string PlayerName { get; init; }

    public required PlayerKind PlayerKind { get; init; }

    public required string Currency { get; init; }

    public required decimal StartingCash { get; init; }

    public required decimal Cash { get; init; }

    public required int HoldingCount { get; init; }

    public required int TradeCount { get; init; }

    public required DateTimeOffset CreatedAt { get; init; }

    public required DateTimeOffset UpdatedAt { get; init; }

    public Goal? Goal { get; init; }
}
