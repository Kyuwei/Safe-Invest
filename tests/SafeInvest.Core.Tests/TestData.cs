using SafeInvest.Core.Engine;
using SafeInvest.Core.Models;

namespace SafeInvest.Core.Tests;

/// <summary>Shared builders so each test only states what it actually cares about.</summary>
internal static class TestData
{
    public static readonly DateTimeOffset Origin = new(2026, 1, 1, 12, 0, 0, TimeSpan.Zero);

    public static readonly Asset Bitcoin = new()
    {
        Symbol = "BTC",
        Name = "Bitcoin",
        Kind = AssetKind.Crypto,
        ProviderId = "bitcoin",
    };

    public static readonly Asset Microsoft = new()
    {
        Symbol = "MSFT",
        Name = "Microsoft Corporation",
        Kind = AssetKind.Stock,
    };

    public static FakeTimeProvider Clock(DateTimeOffset? start = null) => new(start ?? Origin);

    public static GameSession Session(
        PlayerKind kind = PlayerKind.Human,
        decimal startingCash = 10_000m,
        decimal feePercent = 0m,
        TimeProvider? clock = null) =>
        GameFactory.Create(
            playerName: kind == PlayerKind.Ai ? "Claude" : "Alice",
            playerKind: kind,
            startingCash: startingCash,
            currency: "EUR",
            feePercent: feePercent,
            timeProvider: clock ?? Clock());

    public static Quote Price(Asset asset, decimal price, decimal? changePercent = null, bool simulated = false) => new()
    {
        Symbol = asset.Symbol,
        Kind = asset.Kind,
        Price = price,
        Currency = "EUR",
        AsOf = Origin,
        SourceId = simulated ? "simulated" : "test",
        IsSimulated = simulated,
        ChangePercent24h = changePercent,
        Name = asset.Name,
    };

    public static Dictionary<string, Quote> Quotes(params Quote[] quotes) =>
        quotes.ToDictionary(q => q.Key, q => q);
}

/// <summary>Minimal controllable clock — enough for the engine and the goal tracker.</summary>
internal sealed class FakeTimeProvider(DateTimeOffset now) : TimeProvider
{
    public DateTimeOffset Now { get; set; } = now;

    public override DateTimeOffset GetUtcNow() => Now;

    public void Advance(TimeSpan delta) => Now += delta;
}
