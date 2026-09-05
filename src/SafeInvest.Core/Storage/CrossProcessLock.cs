namespace SafeInvest.Core.Storage;

/// <summary>
/// A lock held across processes via an exclusively-opened file. The WinUI app and the
/// MCP server are two separate processes writing the same game files, so an in-process
/// lock would not be enough. A lock file works identically on Windows and on Linux CI,
/// unlike named mutexes.
/// </summary>
public sealed class CrossProcessLock : IDisposable
{
    private static readonly TimeSpan DefaultTimeout = TimeSpan.FromSeconds(10);
    private static readonly TimeSpan RetryDelay = TimeSpan.FromMilliseconds(25);

    private FileStream? _stream;

    private CrossProcessLock(FileStream stream) => _stream = stream;

    public static async Task<CrossProcessLock> AcquireAsync(
        string path,
        TimeSpan? timeout = null,
        CancellationToken cancellationToken = default)
    {
        Directory.CreateDirectory(Path.GetDirectoryName(path)!);

        TimeSpan budget = timeout ?? DefaultTimeout;
        long deadline = Environment.TickCount64 + (long)budget.TotalMilliseconds;

        while (true)
        {
            cancellationToken.ThrowIfCancellationRequested();

            try
            {
                FileStream stream = new(
                    path,
                    FileMode.OpenOrCreate,
                    FileAccess.ReadWrite,
                    FileShare.None,
                    bufferSize: 1,
                    FileOptions.WriteThrough);

                return new CrossProcessLock(stream);
            }
            catch (IOException) when (Environment.TickCount64 < deadline)
            {
                await Task.Delay(RetryDelay, cancellationToken).ConfigureAwait(false);
            }
            catch (UnauthorizedAccessException) when (Environment.TickCount64 < deadline)
            {
                await Task.Delay(RetryDelay, cancellationToken).ConfigureAwait(false);
            }
            catch (IOException ex)
            {
                throw new TimeoutException(
                    $"Impossible d'obtenir le verrou sur « {path} » : un autre processus Safe Invest le retient.",
                    ex);
            }
        }
    }

    public void Dispose()
    {
        _stream?.Dispose();
        _stream = null;
    }
}
