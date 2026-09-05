using SafeInvest.Core.Models;

namespace SafeInvest.MarketData;

/// <summary>
/// A curated shortlist of well-known assets. It gives the "Marché" screen something to
/// show before the user types anything, and it maps tickers onto the ids CoinGecko needs
/// ("BTC" to "bitcoin") without spending a search call.
/// </summary>
public static class AssetCatalog
{
    private static readonly Asset[] Assets =
    [
        // --- Cryptomonnaies ---
        Crypto("BTC", "Bitcoin", "bitcoin"),
        Crypto("ETH", "Ethereum", "ethereum"),
        Crypto("SOL", "Solana", "solana"),
        Crypto("XRP", "XRP", "ripple"),
        Crypto("ADA", "Cardano", "cardano"),
        Crypto("DOGE", "Dogecoin", "dogecoin"),
        Crypto("AVAX", "Avalanche", "avalanche-2"),
        Crypto("DOT", "Polkadot", "polkadot"),
        Crypto("LINK", "Chainlink", "chainlink"),
        Crypto("MATIC", "Polygon", "matic-network"),
        Crypto("LTC", "Litecoin", "litecoin"),
        Crypto("BNB", "BNB", "binancecoin"),

        // --- Actions ---
        Stock("AAPL", "Apple"),
        Stock("MSFT", "Microsoft"),
        Stock("GOOGL", "Alphabet (Google)"),
        Stock("AMZN", "Amazon"),
        Stock("NVDA", "NVIDIA"),
        Stock("META", "Meta (Facebook)"),
        Stock("TSLA", "Tesla"),
        Stock("NFLX", "Netflix"),
        Stock("MC.PA", "LVMH"),
        Stock("AIR.PA", "Airbus"),
        Stock("OR.PA", "L'Oréal"),
        Stock("TTE.PA", "TotalEnergies"),
        Stock("SAN.PA", "Sanofi"),
        Stock("BNP.PA", "BNP Paribas"),

        // --- ETF ---
        Etf("CW8.PA", "Amundi MSCI World"),
        Etf("ESE.PA", "BNP Paribas S&P 500"),
        Etf("SPY", "SPDR S&P 500"),
        Etf("QQQ", "Invesco Nasdaq 100"),
        Etf("IWDA.AS", "iShares Core MSCI World"),
    ];

    private static readonly Dictionary<string, Asset> ByKey =
        Assets.ToDictionary(a => a.Key, StringComparer.Ordinal);

    /// <summary>Everything in the catalog, ordered by family then name.</summary>
    public static IReadOnlyList<Asset> All { get; } =
        [.. Assets.OrderBy(a => a.Kind).ThenBy(a => a.Name, StringComparer.CurrentCulture)];

    public static IReadOnlyList<Asset> OfKind(AssetKind kind) =>
        [.. All.Where(a => a.Kind == kind)];

    /// <summary>Looks a known asset up by ticker, so we can recover its provider id.</summary>
    public static Asset? Find(AssetKind kind, string symbol) =>
        ByKey.GetValueOrDefault(Asset.MakeKey(kind, symbol));

    /// <summary>
    /// Fills in a provider id from the catalog when the caller only had a ticker.
    /// Returns the asset unchanged when nothing is known about it.
    /// </summary>
    public static Asset Enrich(Asset asset)
    {
        ArgumentNullException.ThrowIfNull(asset);

        if (!string.IsNullOrWhiteSpace(asset.ProviderId))
        {
            return asset;
        }

        Asset? known = Find(asset.Kind, asset.Symbol);
        return known is null ? asset : asset with { ProviderId = known.ProviderId };
    }

    /// <summary>Offline name/ticker search, used to seed the market screen and as a search fallback.</summary>
    public static IReadOnlyList<Asset> Search(string query, AssetKind? kind, int limit)
    {
        if (string.IsNullOrWhiteSpace(query))
        {
            return kind is null ? All.Take(limit).ToList() : OfKind(kind.Value).Take(limit).ToList();
        }

        string needle = query.Trim();

        return All
            .Where(a => kind is null || a.Kind == kind.Value)
            .Where(a =>
                a.Symbol.Contains(needle, StringComparison.OrdinalIgnoreCase)
                || a.Name.Contains(needle, StringComparison.OrdinalIgnoreCase))
            .OrderByDescending(a => a.Symbol.Equals(needle, StringComparison.OrdinalIgnoreCase))
            .Take(limit)
            .ToList();
    }

    private static Asset Crypto(string symbol, string name, string coinGeckoId) => new()
    {
        Symbol = symbol,
        Name = name,
        Kind = AssetKind.Crypto,
        ProviderId = coinGeckoId,
    };

    private static Asset Stock(string symbol, string name) => new()
    {
        Symbol = symbol,
        Name = name,
        Kind = AssetKind.Stock,
    };

    private static Asset Etf(string symbol, string name) => new()
    {
        Symbol = symbol,
        Name = name,
        Kind = AssetKind.Etf,
    };
}
