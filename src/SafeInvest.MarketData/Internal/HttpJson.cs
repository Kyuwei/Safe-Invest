using System.Net;
using System.Text.Json;
using SafeInvest.Core.Abstractions;

namespace SafeInvest.MarketData.Internal;

/// <summary>
/// Shared plumbing for calling a market data API: sends the request, maps HTTP failures
/// onto <see cref="QuoteProviderException"/> so the provider chain can fall through, and
/// flags 429 separately because a rate limit is worth reporting to the user.
/// </summary>
internal static class HttpJson
{
    /// <summary>
    /// Sent on every request that does not set its own. HttpClient sends no User-Agent by
    /// default, and the Cloudflare front ends in front of CoinGecko (and others) answer 403
    /// to anything anonymous — so an identified client is the difference between the main
    /// crypto source working and the chain silently falling through to a scraper.
    /// </summary>
    public const string DefaultUserAgent = "SafeInvest/0.1 (+https://github.com/Kyuwei/Safe-Invest)";

    public static async Task<JsonDocument> GetAsync(
        HttpClient client,
        string providerId,
        string url,
        CancellationToken cancellationToken,
        IReadOnlyDictionary<string, string>? headers = null)
    {
        using HttpRequestMessage request = new(HttpMethod.Get, url);

        ApplyHeaders(request, headers);

        HttpResponseMessage response;
        try
        {
            response = await client
                .SendAsync(request, HttpCompletionOption.ResponseHeadersRead, cancellationToken)
                .ConfigureAwait(false);
        }
        catch (Exception ex) when (ex is HttpRequestException or TaskCanceledException && !cancellationToken.IsCancellationRequested)
        {
            throw new QuoteProviderException(providerId, $"{providerId} est injoignable : {ex.Message}", ex);
        }

        using (response)
        {
            if (response.StatusCode is HttpStatusCode.TooManyRequests)
            {
                throw new QuoteProviderException(
                    providerId,
                    $"{providerId} a renvoyé 429 : quota gratuit atteint, passage à la source suivante.")
                {
                    IsRateLimited = true,
                };
            }

            if (!response.IsSuccessStatusCode)
            {
                throw new QuoteProviderException(
                    providerId,
                    $"{providerId} a renvoyé {(int)response.StatusCode} ({response.ReasonPhrase}).");
            }

            await using Stream stream = await response.Content
                .ReadAsStreamAsync(cancellationToken)
                .ConfigureAwait(false);

            try
            {
                return await JsonDocument.ParseAsync(stream, cancellationToken: cancellationToken).ConfigureAwait(false);
            }
            catch (JsonException ex)
            {
                throw new QuoteProviderException(providerId, $"Réponse illisible de {providerId}.", ex);
            }
        }
    }

    public static async Task<string> GetStringAsync(
        HttpClient client,
        string providerId,
        string url,
        CancellationToken cancellationToken,
        IReadOnlyDictionary<string, string>? headers = null)
    {
        using HttpRequestMessage request = new(HttpMethod.Get, url);

        ApplyHeaders(request, headers);

        try
        {
            using HttpResponseMessage response = await client
                .SendAsync(request, cancellationToken)
                .ConfigureAwait(false);

            if (response.StatusCode is HttpStatusCode.TooManyRequests)
            {
                throw new QuoteProviderException(providerId, $"{providerId} : quota atteint (429).")
                {
                    IsRateLimited = true,
                };
            }

            if (!response.IsSuccessStatusCode)
            {
                throw new QuoteProviderException(
                    providerId,
                    $"{providerId} a renvoyé {(int)response.StatusCode} ({response.ReasonPhrase}).");
            }

            return await response.Content.ReadAsStringAsync(cancellationToken).ConfigureAwait(false);
        }
        catch (Exception ex) when (ex is HttpRequestException or TaskCanceledException && !cancellationToken.IsCancellationRequested)
        {
            throw new QuoteProviderException(providerId, $"{providerId} est injoignable : {ex.Message}", ex);
        }
    }

    private static void ApplyHeaders(HttpRequestMessage request, IReadOnlyDictionary<string, string>? headers)
    {
        if (headers is not null)
        {
            foreach ((string name, string value) in headers)
            {
                request.Headers.TryAddWithoutValidation(name, value);
            }
        }

        if (!request.Headers.Contains("User-Agent"))
        {
            request.Headers.TryAddWithoutValidation("User-Agent", DefaultUserAgent);
        }
    }

    /// <summary>Reads a JSON number that some APIs send as a string.</summary>
    public static decimal? Decimal(JsonElement parent, string property)
    {
        if (!parent.TryGetProperty(property, out JsonElement value))
        {
            return null;
        }

        return value.ValueKind switch
        {
            JsonValueKind.Number when value.TryGetDecimal(out decimal number) => number,
            JsonValueKind.String when decimal.TryParse(
                value.GetString(),
                System.Globalization.NumberStyles.Float,
                System.Globalization.CultureInfo.InvariantCulture,
                out decimal parsed) => parsed,
            _ => null,
        };
    }

    public static string? String(JsonElement parent, string property) =>
        parent.TryGetProperty(property, out JsonElement value) && value.ValueKind == JsonValueKind.String
            ? value.GetString()
            : null;
}
