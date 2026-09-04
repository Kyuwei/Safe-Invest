using System.Security.Cryptography;
using System.Text;
using SafeInvest.Core.Abstractions;
using SafeInvest.Core.Models;

namespace SafeInvest.MarketData.Providers;

/// <summary>
/// Invents prices when there is no network, no API key and no reachable web page — and
/// for the demo mode a teacher can switch on deliberately.
///
/// The walk is deterministic: the same asset at the same minute always yields the same
/// price, on any machine. That keeps a classroom in sync and makes the tests repeatable.
/// Every quote is flagged <see cref="Quote.IsSimulated"/> so the app can badge it clearly:
/// an educational tool must never let a made-up number pass for a real one.
/// </summary>
public sealed class SimulatedQuoteProvider(TimeProvider? timeProvider = null) : IQuoteProvider
{
    private readonly TimeProvider _clock = timeProvider ?? TimeProvider.System;

    public string Id => "simulated";

    public string DisplayName => "Cours simulés (hors ligne)";

    public IReadOnlySet<AssetKind> SupportedKinds { get; } =
        new HashSet<AssetKind> { AssetKind.Crypto, AssetKind.Stock, AssetKind.Etf };

    public bool IsConfigured => true;

    public bool IsSimulated => true;

    public Task<IReadOnlyList<Quote>> GetQuotesAsync(
        IReadOnlyCollection<Asset> assets,
        string currency,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(assets);

        DateTimeOffset now = _clock.GetUtcNow();
        List<Quote> quotes = [];

        foreach (Asset asset in assets)
        {
            decimal price = PriceAt(asset, now);
            decimal yesterday = PriceAt(asset, now.AddDays(-1));

            quotes.Add(new Quote
            {
                Symbol = asset.Symbol,
                Kind = asset.Kind,
                Name = asset.Name,
                Price = price,
                Currency = currency.ToUpperInvariant(),
                Change24h = Round(price - yesterday, asset.Kind),
                ChangePercent24h = yesterday == 0m
                    ? 0m
                    : Math.Round((price - yesterday) / yesterday * 100m, 4, MidpointRounding.AwayFromZero),
                AsOf = now,
                SourceId = Id,
                IsSimulated = true,
            });
        }

        return Task.FromResult<IReadOnlyList<Quote>>(quotes);
    }

    public Task<IReadOnlyList<Asset>> SearchAsync(
        string query,
        AssetKind? kind,
        int limit,
        CancellationToken cancellationToken = default) =>
        Task.FromResult(AssetCatalog.Search(query, kind, limit));

    public Task<IReadOnlyList<Candle>> GetHistoryAsync(
        Asset asset,
        string currency,
        HistoryRange range,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(asset);

        DateTimeOffset now = _clock.GetUtcNow();
        (int points, TimeSpan step) = ShapeFor(range);

        List<Candle> candles = new(points);
        for (int i = points - 1; i >= 0; i--)
        {
            DateTimeOffset at = now - step * i;
            candles.Add(new Candle { Timestamp = at, Close = PriceAt(asset, at) });
        }

        return Task.FromResult<IReadOnlyList<Candle>>(candles);
    }

    /// <summary>
    /// A smooth pseudo-random walk built from three sine waves of different periods, so
    /// the curve has both a long trend and short-term noise without ever going negative.
    /// </summary>
    internal static decimal PriceAt(Asset asset, DateTimeOffset at)
    {
        uint seed = StableSeed(asset.Key);
        double basePrice = BasePriceFor(asset.Kind, seed);
        double volatility = asset.Kind == AssetKind.Crypto ? 0.28d : 0.11d;

        double hours = (at - DateTimeOffset.UnixEpoch).TotalHours;
        double phase = seed % 1000 / 1000d * Math.Tau;

        double slow = Math.Sin(hours / (24d * 90d) * Math.Tau + phase);
        double medium = Math.Sin(hours / (24d * 7d) * Math.Tau + phase * 2d);
        double fast = Math.Sin(hours / 6d * Math.Tau + phase * 3d);

        double factor = 1d + volatility * (0.6d * slow + 0.3d * medium + 0.1d * fast);
        double price = basePrice * Math.Max(factor, 0.05d);

        return Round((decimal)price, asset.Kind);
    }

    private static double BasePriceFor(AssetKind kind, uint seed) => kind switch
    {
        // Spread crypto over a wide range so BTC-sized and altcoin-sized prices both appear.
        AssetKind.Crypto => 5d + Math.Pow(10d, 0.5d + seed % 4500 / 1000d),
        AssetKind.Etf => 40d + seed % 400,
        _ => 15d + seed % 600,
    };

    private static decimal Round(decimal value, AssetKind kind) =>
        Math.Round(value, kind == AssetKind.Crypto && value < 1m ? 6 : 2, MidpointRounding.AwayFromZero);

    /// <summary>
    /// SHA-256 rather than string.GetHashCode: the latter is randomised per process, which
    /// would make every restart invent a different market.
    /// </summary>
    private static uint StableSeed(string key)
    {
        byte[] hash = SHA256.HashData(Encoding.UTF8.GetBytes(key));
        return BitConverter.ToUInt32(hash, 0);
    }

    private static (int Points, TimeSpan Step) ShapeFor(HistoryRange range) => range switch
    {
        HistoryRange.Day => (24, TimeSpan.FromHours(1)),
        HistoryRange.Week => (28, TimeSpan.FromHours(6)),
        HistoryRange.Month => (30, TimeSpan.FromDays(1)),
        HistoryRange.Quarter => (45, TimeSpan.FromDays(2)),
        _ => (52, TimeSpan.FromDays(7)),
    };
}
