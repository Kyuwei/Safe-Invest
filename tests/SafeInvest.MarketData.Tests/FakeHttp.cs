using System.Net;

namespace SafeInvest.MarketData.Tests;

/// <summary>
/// Serves canned responses by URL substring so the provider tests never touch the network.
/// Recorded shapes live in Fixtures/ and come from real calls to each API.
/// </summary>
internal sealed class FakeHttpMessageHandler : HttpMessageHandler
{
    private readonly List<(string Match, HttpStatusCode Status, string Body)> _routes = [];

    public List<string> RequestedUrls { get; } = [];

    public FakeHttpMessageHandler Respond(string urlContains, string body, HttpStatusCode status = HttpStatusCode.OK)
    {
        _routes.Add((urlContains, status, body));
        return this;
    }

    public FakeHttpMessageHandler RespondWithFixture(
        string urlContains,
        string fixtureName,
        HttpStatusCode status = HttpStatusCode.OK) =>
        Respond(urlContains, Fixture(fixtureName), status);

    public static string Fixture(string name) =>
        File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Fixtures", name));

    protected override Task<HttpResponseMessage> SendAsync(
        HttpRequestMessage request,
        CancellationToken cancellationToken)
    {
        string url = request.RequestUri?.ToString() ?? string.Empty;
        RequestedUrls.Add(url);

        foreach ((string match, HttpStatusCode status, string body) in _routes)
        {
            if (url.Contains(match, StringComparison.OrdinalIgnoreCase))
            {
                return Task.FromResult(new HttpResponseMessage(status)
                {
                    Content = new StringContent(body),
                });
            }
        }

        return Task.FromResult(new HttpResponseMessage(HttpStatusCode.NotFound)
        {
            Content = new StringContent($"Aucune règle de test pour {url}"),
        });
    }

    public HttpClient CreateClient() => new(this, disposeHandler: false)
    {
        Timeout = TimeSpan.FromSeconds(5),
    };
}
