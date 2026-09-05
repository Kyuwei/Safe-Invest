using SafeInvest.Core.Models;

namespace SafeInvest.Core.Storage;

/// <summary>
/// Persistence for game sessions, shared by the WinUI app and the MCP server.
/// Implementations must be safe to use from several processes at once.
/// </summary>
public interface IGameStore
{
    /// <summary>Folder the games live in; watched by the app for live refresh.</summary>
    string GamesDirectory { get; }

    Task<IReadOnlyList<GameSummary>> ListAsync(CancellationToken cancellationToken = default);

    Task<GameSession?> LoadAsync(Guid id, CancellationToken cancellationToken = default);

    Task SaveAsync(GameSession session, CancellationToken cancellationToken = default);

    Task DeleteAsync(Guid id, CancellationToken cancellationToken = default);

    Task<Guid?> GetCurrentGameIdAsync(CancellationToken cancellationToken = default);

    Task SetCurrentGameAsync(Guid? id, CancellationToken cancellationToken = default);

    /// <summary>
    /// Reads the freshest copy from disk, applies <paramref name="mutate"/> and writes it
    /// back, all while holding the cross-process lock. Every write that depends on the
    /// current state (a trade, above all) must go through here, otherwise the app and the
    /// AI can overwrite each other.
    /// </summary>
    Task<GameSession> MutateAsync(
        Guid id,
        Action<GameSession> mutate,
        CancellationToken cancellationToken = default);
}
