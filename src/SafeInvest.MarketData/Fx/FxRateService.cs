using System.Text.Json;
using Microsoft.Extensions.Caching.Memory;
using SafeInvest.Core.Abstractions;
using SafeInvest.Core.Models;
using SafeInvest.MarketData.Internal;

namespace SafeInvest.MarketData.Fx;

/// <summary>Converts between currencies. Needed because US shares quote in USD while a game runs in EUR.</summary>
public interface IFxRateService
{
    Task<decimal> GetRateAsync(string sourceCurrency, string targetCurrency, CancellationToken cancellationToken = default);

    Task<Quote> ConvertAsync(Quote quote, string targetCurrency, CancellationToken cancellationToken = default);
}

/// <summary>
/// Frankfurter (European Central Bank reference rates, no API key) with Yahoo's FX pairs
/// as a backup. Rates are cached for an hour — the ECB only publishes once a day, and an
/// educational simulator does not need tick-level foreign exchange.
/// </summary>
public sealed class FxRateService(
    HttpClient httpClient,
    IMemoryCache cache,
    MarketDataOptions options) : IFxRateService
{
    private const string Id = "fx";

    public async Task<decimal> GetRateAsync(
        string sourceCurrency,
        string targetCurrency,
        CancellationToken cancellationToken = default)
    {
        string source = Normalise(sourceCurrency);
        string target = Normalise(targetCurrency);

        if (source == target)
        {
            return 1m;
        }

        string key = $"fx:{source}:{target}";
        if (cache.TryGetValue(key, out decimal cached))
        {
            return cached;
        }

        decimal rate = await FetchAsync(source, target, cancellationToken).ConfigureAwait(false);
        cache.Set(key, rate, options.FxCacheDuration);
        return rate;
    }

    public async Task<Quote> ConvertAsync(
        Quote quote,
        string targetCurrency,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(quote);

        string target = Normalise(targetCurrency);
        if (Normalise(quote.Currency) == target)
        {
            return quote;
        }

        decimal rate = await GetRateAsync(quote.Currency, target, cancellationToken).ConfigureAwait(false);

        // Percentages are currency-independent, so only the absolute figures move.
        return quote with
        {
            Price = quote.Price * rate,
            Currency = target,
            Change24h = quote.Change24h * rate,
            MarketCap = quote.MarketCap * rate,
            Volume24h = quote.Volume24h * rate,
        };
    }

    private async Task<decimal> FetchAsync(string source, string target, CancellationToken cancellationToken)
    {
        try
        {
            return await FromFrankfurterAsync(source, target, cancellationToken).ConfigureAwait(false);
        }
        catch (QuoteProviderException)
        {
            return await FromYahooAsync(source, target, cancellationToken).ConfigureAwait(false);
        }
    }

    private async Task<decimal> FromFrankfurterAsync(string source, string target, CancellationToken cancellationToken)
    {
        string url = $"https://api.frankfurter.dev/v1/latest?base={source}&symbols={target}";
        using JsonDocument document = await HttpJson
            .GetAsync(httpClient, Id, url, cancellationToken)
            .ConfigureAwait(false);

        if (document.RootElement.TryGetProperty("rates", out JsonElement rates)
            && HttpJson.Decimal(rates, target) is { } rate
            && rate > 0m)
        {
            return rate;
        }

        throw new QuoteProviderException(Id, $"Taux {source}→{target} absent de la réponse Frankfurter.");
    }

    private async Task<decimal> FromYahooAsync(string source, string target, CancellationToken cancellationToken)
    {
        string url = $"https://query1.finance.yahoo.com/v8/finance/chart/{source}{target}=X?range=1d&interval=1d";
        using JsonDocument document = await HttpJson
            .GetAsync(httpClient, Id, url, cancellationToken, YahooFinanceHeaders.Default)
            .ConfigureAwait(false);

        if (document.RootElement.TryGetProperty("chart", out JsonElement chart)
            && chart.TryGetProperty("result", out JsonElement results)
            && results.ValueKind == JsonValueKind.Array
            && results.GetArrayLength() > 0
            && results[0].TryGetProperty("meta", out JsonElement meta)
            && HttpJson.Decimal(meta, "regularMarketPrice") is { } rate
            && rate > 0m)
        {
            return rate;
        }

        throw new QuoteProviderException(Id, $"Impossible d'obtenir le taux {source}→{target}.");
    }

    private static string Normalise(string currency) =>
        (currency ?? "EUR").Trim().ToUpperInvariant();
}

/// <summary>Yahoo's endpoints refuse requests without a browser-shaped User-Agent.</summary>
internal static class YahooFinanceHeaders
{
    public static readonly Dictionary<string, string> Default = new()
    {
        ["User-Agent"] = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 " +
                         "(KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36",
        ["Accept"] = "application/json,text/plain,*/*",
    };
}
