namespace SafeInvest.Core.Storage;

/// <summary>
/// Raises an event whenever a game file changes on disk. This is what makes the AI mode
/// feel live: the MCP server writes a trade, the app notices within a fraction of a
/// second and refreshes the dashboard.
/// </summary>
public sealed class GameStoreWatcher : IDisposable
{
    private readonly FileSystemWatcher _watcher;
    private readonly Lock _gate = new();
    private readonly TimeSpan _debounce;
    private Timer? _timer;
    private bool _disposed;

    public GameStoreWatcher(string gamesDirectory, TimeSpan? debounce = null)
    {
        Directory.CreateDirectory(gamesDirectory);
        _debounce = debounce ?? TimeSpan.FromMilliseconds(250);

        _watcher = new FileSystemWatcher(gamesDirectory, "*.json")
        {
            NotifyFilter = NotifyFilters.LastWrite | NotifyFilters.FileName | NotifyFilters.Size,
            IncludeSubdirectories = false,
        };

        _watcher.Changed += OnFileEvent;
        _watcher.Created += OnFileEvent;
        _watcher.Deleted += OnFileEvent;
        _watcher.Renamed += OnFileEvent;
    }

    /// <summary>Fired on a background thread after the debounce window closes.</summary>
    public event EventHandler? GamesChanged;

    public void Start() => _watcher.EnableRaisingEvents = true;

    public void Stop() => _watcher.EnableRaisingEvents = false;

    private void OnFileEvent(object sender, FileSystemEventArgs e)
    {
        // An atomic save fires several events in a row (temp file created, then moved).
        // Collapse them into one refresh.
        lock (_gate)
        {
            if (_disposed)
            {
                return;
            }

            _timer?.Dispose();
            _timer = new Timer(_ => GamesChanged?.Invoke(this, EventArgs.Empty), null, _debounce, Timeout.InfiniteTimeSpan);
        }
    }

    public void Dispose()
    {
        lock (_gate)
        {
            if (_disposed)
            {
                return;
            }

            _disposed = true;
            _timer?.Dispose();
            _timer = null;
        }

        _watcher.Changed -= OnFileEvent;
        _watcher.Created -= OnFileEvent;
        _watcher.Deleted -= OnFileEvent;
        _watcher.Renamed -= OnFileEvent;
        _watcher.Dispose();
    }
}
