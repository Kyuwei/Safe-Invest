using System.ComponentModel;
using ModelContextProtocol.Server;
using SafeInvest.Core.Abstractions;
using SafeInvest.Core.Models;

namespace SafeInvest.Mcp.Tools;

/// <summary>Looking up assets and their prices.</summary>
[McpServerToolType]
internal sealed class MarketTools(SafeInvestContext context)
{
    [McpServerTool(Name = "search_assets")]
    [Description("Finds cryptocurrencies, shares or ETFs by name or ticker. Returns the exact " +
                 "symbols to pass to get_quotes, buy and sell.")]
    public Task<string> SearchAssetsAsync(
        [Description("Name or ticker, e.g. \"solana\" or \"MSFT\".")]
        string query,
        [Description("Narrow the search: \"crypto\", \"stock\" or \"etf\". Omit to search everything.")]
        string? kind = null,
        [Description("Maximum results. Defaults to 15.")]
        int? limit = null,
        CancellationToken cancellationToken = default) =>
        SafeInvestContext.GuardAsync(async () =>
        {
            AssetKind? filter = string.IsNullOrWhiteSpace(kind) ? null : SafeInvestContext.ParseKind(kind);

            IReadOnlyList<Asset> assets = await context.MarketData
                .SearchAsync(query, filter, Math.Clamp(limit ?? 15, 1, 50), cancellationToken)
                .ConfigureAwait(false);

            return new
            {
                Ok = true,
                Query = query,
                Results = assets.Select(Contracts.ToRow).ToList(),
            };
        });

    [McpServerTool(Name = "get_quotes")]
    [Description("Current prices for one or more assets, with the 24-hour move and where the " +
                 "figure came from. A quote flagged isSimulated is invented because every real " +
                 "source failed — say so rather than treating it as a market price.")]
    public Task<string> GetQuotesAsync(
        [Description("Tickers to price, e.g. [\"BTC\", \"ETH\"].")]
        string[] symbols,
        [Description("Asset family of these tickers: \"crypto\" (default), \"stock\" or \"etf\".")]
        string? kind = null,
        [Description("Currency to quote in. Defaults to the open game's currency, else EUR.")]
        string? currency = null,
        [Description("Defaults to the currently open game.")]
        string? gameId = null,
        CancellationToken cancellationToken = default) =>
        SafeInvestContext.GuardAsync(async () =>
        {
            if (symbols is null || symbols.Length == 0)
            {
                throw new SafeInvestToolException("Aucun symbole demandé.", "Passez par exemple [\"BTC\"].");
            }

            AssetKind assetKind = SafeInvestContext.ParseKind(kind);
            string target = await ResolveCurrencyAsync(currency, gameId, cancellationToken).ConfigureAwait(false);

            List<Asset> assets = [];
            foreach (string symbol in symbols.Take(25))
            {
                assets.Add(await context.ResolveAssetAsync(symbol, assetKind, cancellationToken).ConfigureAwait(false));
            }

            IReadOnlyDictionary<string, Quote> quotes = await context.MarketData
                .GetQuotesAsync(assets, target, cancellationToken)
                .ConfigureAwait(false);

            List<string> missing = [.. assets.Where(a => !quotes.ContainsKey(a.Key)).Select(a => a.Symbol)];

            return new
            {
                Ok = true,
                Currency = target,
                Quotes = quotes.Values.Select(Contracts.ToRow).ToList(),
                NotFound = missing,
                ContainsSimulatedPrices = quotes.Values.Any(q => q.IsSimulated),
            };
        });

    [McpServerTool(Name = "get_price_history")]
    [Description("Past closing prices for one asset, to judge a trend before trading. " +
                 "Ranges: day, week, month, quarter, year.")]
    public Task<string> GetPriceHistoryAsync(
        [Description("Ticker, e.g. \"BTC\".")]
        string symbol,
        [Description("Asset family: \"crypto\" (default), \"stock\" or \"etf\".")]
        string? kind = null,
        [Description("How far back: day, week, month (default), quarter or year.")]
        string? range = null,
        [Description("Currency. Defaults to the open game's currency, else EUR.")]
        string? currency = null,
        [Description("Defaults to the currently open game.")]
        string? gameId = null,
        CancellationToken cancellationToken = default) =>
        SafeInvestContext.GuardAsync(async () =>
        {
            AssetKind assetKind = SafeInvestContext.ParseKind(kind);
            string target = await ResolveCurrencyAsync(currency, gameId, cancellationToken).ConfigureAwait(false);
            Asset asset = await context.ResolveAssetAsync(symbol, assetKind, cancellationToken).ConfigureAwait(false);

            IReadOnlyList<Candle> candles = await context.MarketData
                .GetHistoryAsync(asset, target, ParseRange(range), cancellationToken)
                .ConfigureAwait(false);

            decimal? first = candles.Count > 0 ? candles[0].Close : null;
            decimal? last = candles.Count > 0 ? candles[^1].Close : null;

            return new
            {
                Ok = true,
                Symbol = asset.Symbol,
                Name = asset.Name,
                Currency = target,
                Range = ParseRange(range).ToString(),
                ChangePercentOverRange = first is > 0m && last is not null
                    ? Math.Round((last.Value - first.Value) / first.Value * 100m, 2, MidpointRounding.AwayFromZero)
                    : (decimal?)null,
                Points = candles.Select(c => new Contracts.CandleRow
                {
                    Timestamp = c.Timestamp,
                    Close = c.Close,
                }).ToList(),
            };
        });

    [McpServerTool(Name = "list_popular_assets")]
    [Description("The built-in shortlist of well-known cryptocurrencies, shares and ETFs the " +
                 "app offers by default. A good starting point when no ticker is in mind.")]
    public static Task<string> ListPopularAssetsAsync(
        [Description("Filter by family: \"crypto\", \"stock\" or \"etf\". Omit for everything.")]
        string? kind = null) =>
        SafeInvestContext.GuardAsync(() =>
        {
            IReadOnlyList<Asset> assets = string.IsNullOrWhiteSpace(kind)
                ? MarketData.AssetCatalog.All
                : MarketData.AssetCatalog.OfKind(SafeInvestContext.ParseKind(kind));

            return Task.FromResult<object>(new
            {
                Ok = true,
                Assets = assets.Select(Contracts.ToRow).ToList(),
            });
        });

    /// <summary>Falls back to the open game's currency so quotes match what a trade would cost.</summary>
    private async Task<string> ResolveCurrencyAsync(
        string? currency,
        string? gameId,
        CancellationToken cancellationToken)
    {
        if (!string.IsNullOrWhiteSpace(currency))
        {
            return currency.Trim().ToUpperInvariant();
        }

        try
        {
            GameSession session = await context.RequireGameAsync(gameId, cancellationToken).ConfigureAwait(false);
            return session.Currency;
        }
        catch (SafeInvestToolException)
        {
            // Quoting prices should work even before a game exists.
            return "EUR";
        }
    }

    private static HistoryRange ParseRange(string? range) =>
        (range ?? "month").Trim().ToLowerInvariant() switch
        {
            "day" or "1d" or "jour" => HistoryRange.Day,
            "week" or "1w" or "7d" or "semaine" => HistoryRange.Week,
            "month" or "1m" or "30d" or "mois" => HistoryRange.Month,
            "quarter" or "3m" or "90d" or "trimestre" => HistoryRange.Quarter,
            "year" or "1y" or "365d" or "an" or "annee" or "année" => HistoryRange.Year,
            _ => throw new SafeInvestToolException(
                $"Période inconnue : « {range} ».",
                "Valeurs acceptées : day, week, month, quarter, year."),
        };
}
