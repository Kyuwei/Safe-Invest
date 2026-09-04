using System.Text.Json;
using SafeInvest.Core.Models;

namespace SafeInvest.Core.Storage;

/// <summary>
/// Stores each game as one JSON file under %LOCALAPPDATA%\SafeInvest\games.
/// Writes are atomic (temp file then move) and serialised across processes by
/// <see cref="CrossProcessLock"/>, so a half-written save can never be observed.
/// </summary>
public sealed class JsonGameStore : IGameStore
{
    private readonly string _root;
    private readonly TimeProvider _timeProvider;

    public JsonGameStore(string? rootDirectory = null, TimeProvider? timeProvider = null)
    {
        _root = rootDirectory ?? SafeInvestPaths.Root;
        _timeProvider = timeProvider ?? TimeProvider.System;
        Directory.CreateDirectory(_root);
        Directory.CreateDirectory(GamesDirectory);
    }

    public string GamesDirectory => Path.Combine(_root, "games");

    private string LockFile => Path.Combine(_root, ".store.lock");

    private string CurrentGameFile => Path.Combine(_root, "current.json");

    private string GameFile(Guid id) => Path.Combine(GamesDirectory, $"{id:N}.json");

    public async Task<IReadOnlyList<GameSummary>> ListAsync(CancellationToken cancellationToken = default)
    {
        List<GameSummary> summaries = [];

        foreach (string file in Directory.EnumerateFiles(GamesDirectory, "*.json"))
        {
            GameSession? session = await ReadFileAsync(file, cancellationToken).ConfigureAwait(false);
            if (session is null)
            {
                continue;
            }

            summaries.Add(new GameSummary
            {
                Id = session.Id,
                PlayerName = session.PlayerName,
                PlayerKind = session.PlayerKind,
                Currency = session.Currency,
                StartingCash = session.StartingCash,
                Cash = session.Cash,
                HoldingCount = session.Holdings.Count,
                TradeCount = session.Trades.Count,
                CreatedAt = session.CreatedAt,
                UpdatedAt = session.UpdatedAt,
                Goal = session.Goal,
            });
        }

        return summaries.OrderByDescending(s => s.UpdatedAt).ToList();
    }

    public Task<GameSession?> LoadAsync(Guid id, CancellationToken cancellationToken = default) =>
        ReadFileAsync(GameFile(id), cancellationToken);

    public async Task SaveAsync(GameSession session, CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(session);

        using CrossProcessLock @lock = await CrossProcessLock
            .AcquireAsync(LockFile, cancellationToken: cancellationToken)
            .ConfigureAwait(false);

        await WriteUnlockedAsync(session, cancellationToken).ConfigureAwait(false);
    }

    public async Task DeleteAsync(Guid id, CancellationToken cancellationToken = default)
    {
        using CrossProcessLock @lock = await CrossProcessLock
            .AcquireAsync(LockFile, cancellationToken: cancellationToken)
            .ConfigureAwait(false);

        string file = GameFile(id);
        if (File.Exists(file))
        {
            File.Delete(file);
        }

        if (await ReadCurrentUnlockedAsync(cancellationToken).ConfigureAwait(false) == id)
        {
            await WriteCurrentUnlockedAsync(null, cancellationToken).ConfigureAwait(false);
        }
    }

    public async Task<Guid?> GetCurrentGameIdAsync(CancellationToken cancellationToken = default) =>
        await ReadCurrentUnlockedAsync(cancellationToken).ConfigureAwait(false);

    public async Task SetCurrentGameAsync(Guid? id, CancellationToken cancellationToken = default)
    {
        using CrossProcessLock @lock = await CrossProcessLock
            .AcquireAsync(LockFile, cancellationToken: cancellationToken)
            .ConfigureAwait(false);

        await WriteCurrentUnlockedAsync(id, cancellationToken).ConfigureAwait(false);
    }

    public async Task<GameSession> MutateAsync(
        Guid id,
        Action<GameSession> mutate,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(mutate);

        using CrossProcessLock @lock = await CrossProcessLock
            .AcquireAsync(LockFile, cancellationToken: cancellationToken)
            .ConfigureAwait(false);

        GameSession session = await ReadFileAsync(GameFile(id), cancellationToken).ConfigureAwait(false)
            ?? throw new FileNotFoundException($"Partie introuvable : {id}.", GameFile(id));

        mutate(session);

        await WriteUnlockedAsync(session, cancellationToken).ConfigureAwait(false);
        return session;
    }

    private async Task WriteUnlockedAsync(GameSession session, CancellationToken cancellationToken)
    {
        session.SchemaVersion = GameSession.CurrentSchemaVersion;
        session.UpdatedAt = _timeProvider.GetUtcNow();

        string target = GameFile(session.Id);
        string temp = target + ".tmp";

        await using (FileStream stream = File.Create(temp))
        {
            await JsonSerializer
                .SerializeAsync(stream, session, SafeInvestJson.Storage, cancellationToken)
                .ConfigureAwait(false);
        }

        File.Move(temp, target, overwrite: true);
    }

    private static async Task<GameSession?> ReadFileAsync(string path, CancellationToken cancellationToken)
    {
        if (!File.Exists(path))
        {
            return null;
        }

        try
        {
            await using FileStream stream = File.OpenRead(path);
            return await JsonSerializer
                .DeserializeAsync<GameSession>(stream, SafeInvestJson.Storage, cancellationToken)
                .ConfigureAwait(false);
        }
        catch (JsonException)
        {
            // A corrupt save must not take the whole "resume a game" list down with it.
            return null;
        }
        catch (IOException)
        {
            return null;
        }
    }

    private async Task<Guid?> ReadCurrentUnlockedAsync(CancellationToken cancellationToken)
    {
        if (!File.Exists(CurrentGameFile))
        {
            return null;
        }

        try
        {
            await using FileStream stream = File.OpenRead(CurrentGameFile);
            CurrentGamePointer? pointer = await JsonSerializer
                .DeserializeAsync<CurrentGamePointer>(stream, SafeInvestJson.Storage, cancellationToken)
                .ConfigureAwait(false);

            return pointer?.CurrentGameId;
        }
        catch (JsonException)
        {
            return null;
        }
        catch (IOException)
        {
            return null;
        }
    }

    private async Task WriteCurrentUnlockedAsync(Guid? id, CancellationToken cancellationToken)
    {
        string temp = CurrentGameFile + ".tmp";

        await using (FileStream stream = File.Create(temp))
        {
            await JsonSerializer
                .SerializeAsync(
                    stream,
                    new CurrentGamePointer { CurrentGameId = id },
                    SafeInvestJson.Storage,
                    cancellationToken)
                .ConfigureAwait(false);
        }

        File.Move(temp, CurrentGameFile, overwrite: true);
    }

    private sealed record CurrentGamePointer
    {
        public Guid? CurrentGameId { get; init; }
    }
}
