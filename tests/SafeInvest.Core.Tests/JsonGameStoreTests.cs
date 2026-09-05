using SafeInvest.Core.Engine;
using SafeInvest.Core.Models;
using SafeInvest.Core.Storage;
using Xunit;

namespace SafeInvest.Core.Tests;

public sealed class JsonGameStoreTests : IDisposable
{
    private readonly string _root = Path.Combine(
        Path.GetTempPath(),
        "safeinvest-tests",
        Guid.NewGuid().ToString("N"));

    private JsonGameStore NewStore() => new(_root, TestData.Clock());

    [Fact]
    public async Task A_saved_game_comes_back_identical()
    {
        JsonGameStore store = NewStore();
        GameSession session = TestData.Session(PlayerKind.Ai, startingCash: 25_000m);
        session.Goal = new Goal { TargetAmount = 40_000m, Deadline = TestData.Origin.AddYears(2) };

        PortfolioEngine engine = new(TestData.Clock());
        engine.Buy(
            session,
            TestData.Bitcoin,
            TestData.Price(TestData.Bitcoin, 50_000m),
            quantity: 0.25m,
            rationale: "Position d'ouverture sur BTC.");

        await store.SaveAsync(session);
        GameSession? loaded = await store.LoadAsync(session.Id);

        Assert.NotNull(loaded);
        Assert.Equal(session.Id, loaded!.Id);
        Assert.Equal(PlayerKind.Ai, loaded.PlayerKind);
        Assert.Equal(25_000m, loaded.StartingCash);
        Assert.Equal(12_500m, loaded.Cash);
        Assert.Equal(40_000m, loaded.Goal!.TargetAmount);

        Holding holding = Assert.Single(loaded.Holdings);
        Assert.Equal(0.25m, holding.Quantity);
        Assert.Equal("BTC", holding.Asset.Symbol);

        Trade trade = Assert.Single(loaded.Trades);
        Assert.Equal("Position d'ouverture sur BTC.", trade.Rationale);
    }

    [Fact]
    public async Task Loading_an_unknown_game_returns_null()
    {
        JsonGameStore store = NewStore();

        Assert.Null(await store.LoadAsync(Guid.NewGuid()));
    }

    [Fact]
    public async Task The_list_is_ordered_by_most_recently_touched()
    {
        JsonGameStore store = NewStore();
        FakeTimeProvider clock = TestData.Clock();
        JsonGameStore clocked = new(_root, clock);

        GameSession older = TestData.Session();
        await clocked.SaveAsync(older);

        clock.Advance(TimeSpan.FromMinutes(5));
        GameSession newer = TestData.Session();
        await clocked.SaveAsync(newer);

        IReadOnlyList<GameSummary> games = await store.ListAsync();

        Assert.Equal(2, games.Count);
        Assert.Equal(newer.Id, games[0].Id);
    }

    [Fact]
    public async Task Mutate_reads_the_freshest_copy_before_applying_the_change()
    {
        JsonGameStore store = NewStore();
        GameSession session = TestData.Session(startingCash: 1_000m);
        await store.SaveAsync(session);

        // Simulate the WinUI app writing while the caller holds a stale object in memory.
        GameSession fromDisk = (await store.LoadAsync(session.Id))!;
        fromDisk.Cash = 750m;
        await store.SaveAsync(fromDisk);

        GameSession mutated = await store.MutateAsync(session.Id, s => s.Cash -= 250m);

        Assert.Equal(500m, mutated.Cash);
        Assert.Equal(500m, (await store.LoadAsync(session.Id))!.Cash);
    }

    [Fact]
    public async Task Mutating_a_missing_game_fails_loudly()
    {
        JsonGameStore store = NewStore();

        await Assert.ThrowsAsync<FileNotFoundException>(
            () => store.MutateAsync(Guid.NewGuid(), _ => { }));
    }

    [Fact]
    public async Task Concurrent_mutations_do_not_lose_a_single_trade()
    {
        JsonGameStore store = NewStore();
        GameSession session = TestData.Session(startingCash: 100_000m);
        await store.SaveAsync(session);

        // Twenty writers racing through the cross-process lock; all twenty must land.
        await Task.WhenAll(Enumerable.Range(0, 20).Select(i =>
            store.MutateAsync(session.Id, s => s.Cash -= 1m)));

        GameSession reloaded = (await store.LoadAsync(session.Id))!;
        Assert.Equal(99_980m, reloaded.Cash);
    }

    [Fact]
    public async Task The_current_game_pointer_survives_a_round_trip()
    {
        JsonGameStore store = NewStore();
        GameSession session = TestData.Session();
        await store.SaveAsync(session);

        Assert.Null(await store.GetCurrentGameIdAsync());

        await store.SetCurrentGameAsync(session.Id);
        Assert.Equal(session.Id, await store.GetCurrentGameIdAsync());

        await store.SetCurrentGameAsync(null);
        Assert.Null(await store.GetCurrentGameIdAsync());
    }

    [Fact]
    public async Task Deleting_a_game_also_clears_the_current_pointer()
    {
        JsonGameStore store = NewStore();
        GameSession session = TestData.Session();
        await store.SaveAsync(session);
        await store.SetCurrentGameAsync(session.Id);

        await store.DeleteAsync(session.Id);

        Assert.Null(await store.LoadAsync(session.Id));
        Assert.Null(await store.GetCurrentGameIdAsync());
    }

    [Fact]
    public async Task A_corrupt_save_is_skipped_rather_than_breaking_the_list()
    {
        JsonGameStore store = NewStore();
        GameSession good = TestData.Session();
        await store.SaveAsync(good);
        await File.WriteAllTextAsync(
            Path.Combine(store.GamesDirectory, $"{Guid.NewGuid():N}.json"),
            "{ ceci n'est pas du JSON");

        IReadOnlyList<GameSummary> games = await store.ListAsync();

        Assert.Equal(good.Id, Assert.Single(games).Id);
    }

    public void Dispose()
    {
        if (Directory.Exists(_root))
        {
            Directory.Delete(_root, recursive: true);
        }
    }
}

public sealed class SettingsServiceTests : IDisposable
{
    private readonly string _root = Path.Combine(
        Path.GetTempPath(),
        "safeinvest-tests",
        Guid.NewGuid().ToString("N"));

    private string SettingsPath => Path.Combine(_root, "settings.json");

    [Fact]
    public async Task Defaults_put_the_keyless_providers_first()
    {
        using SettingsService service = new(SettingsPath, new PassthroughSecretProtector());

        AppSettings settings = await service.LoadAsync();

        Assert.Equal("coingecko", settings.CryptoProviderOrder[0]);
        Assert.Equal("yahoo", settings.StockProviderOrder[0]);
        Assert.Equal("EUR", settings.DefaultCurrency);
    }

    [Fact]
    public async Task An_api_key_survives_a_save_and_reload()
    {
        using SettingsService service = new(SettingsPath, new PassthroughSecretProtector());
        AppSettings settings = await service.LoadAsync();

        service.SetApiKey(settings, "coinmarketcap", "clef-secrete");
        await service.SaveAsync(settings);

        using SettingsService reopened = new(SettingsPath, new PassthroughSecretProtector());
        AppSettings reloaded = await reopened.LoadAsync();

        Assert.Equal("clef-secrete", reopened.GetApiKey(reloaded, "coinmarketcap"));
    }

    [Fact]
    public async Task Clearing_a_key_removes_it_entirely()
    {
        using SettingsService service = new(SettingsPath, new PassthroughSecretProtector());
        AppSettings settings = await service.LoadAsync();

        service.SetApiKey(settings, "finnhub", "abc");
        service.SetApiKey(settings, "finnhub", null);

        Assert.Empty(settings.ProtectedApiKeys);
        Assert.Null(service.GetApiKey(settings, "finnhub"));
    }

    [Fact]
    public async Task An_environment_variable_stands_in_when_no_key_is_stored()
    {
        using SettingsService service = new(SettingsPath, new PassthroughSecretProtector());
        AppSettings settings = await service.LoadAsync();
        string variable = SettingsService.EnvironmentVariableFor("coingecko");

        try
        {
            Environment.SetEnvironmentVariable(variable, "depuis-l-environnement");
            Assert.Equal("depuis-l-environnement", service.GetApiKey(settings, "coingecko"));
        }
        finally
        {
            Environment.SetEnvironmentVariable(variable, null);
        }
    }

    public void Dispose()
    {
        if (Directory.Exists(_root))
        {
            Directory.Delete(_root, recursive: true);
        }
    }
}
