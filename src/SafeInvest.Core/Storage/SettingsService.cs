using System.Text.Json;

namespace SafeInvest.Core.Storage;

/// <summary>
/// Loads and saves <see cref="AppSettings"/>, and resolves API keys. A key can come from
/// the settings file (encrypted) or from an environment variable, which is handy for CI
/// and for running the MCP server from a shell.
/// </summary>
public sealed class SettingsService(string? settingsPath = null, ISecretProtector? protector = null) : IDisposable
{
    private readonly string _path = settingsPath ?? SafeInvestPaths.SettingsFile;
    private readonly ISecretProtector _protector = protector ?? SecretProtectorFactory.Create();
    private readonly SemaphoreSlim _gate = new(1, 1);

    private AppSettings? _cached;

    /// <summary>Environment variable consulted when no key is stored for a provider.</summary>
    public static string EnvironmentVariableFor(string providerId) =>
        $"SAFEINVEST_{providerId.ToUpperInvariant()}_KEY";

    public async Task<AppSettings> LoadAsync(CancellationToken cancellationToken = default)
    {
        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            if (_cached is not null)
            {
                return _cached;
            }

            _cached = await ReadAsync(cancellationToken).ConfigureAwait(false) ?? new AppSettings();
            return _cached;
        }
        finally
        {
            _gate.Release();
        }
    }

    public async Task SaveAsync(AppSettings settings, CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(settings);

        await _gate.WaitAsync(cancellationToken).ConfigureAwait(false);
        try
        {
            Directory.CreateDirectory(Path.GetDirectoryName(_path)!);
            string temp = _path + ".tmp";

            await using (FileStream stream = File.Create(temp))
            {
                await JsonSerializer
                    .SerializeAsync(stream, settings, SafeInvestJson.Storage, cancellationToken)
                    .ConfigureAwait(false);
            }

            File.Move(temp, _path, overwrite: true);
            _cached = settings;
        }
        finally
        {
            _gate.Release();
        }
    }

    /// <summary>Returns the clear-text key for a provider, or null when none is available.</summary>
    public string? GetApiKey(AppSettings settings, string providerId)
    {
        ArgumentNullException.ThrowIfNull(settings);

        if (settings.ProtectedApiKeys.TryGetValue(providerId, out string? stored)
            && !string.IsNullOrWhiteSpace(stored))
        {
            string clear = _protector.Unprotect(stored);
            if (!string.IsNullOrWhiteSpace(clear))
            {
                return clear;
            }
        }

        string? fromEnvironment = Environment.GetEnvironmentVariable(EnvironmentVariableFor(providerId));
        return string.IsNullOrWhiteSpace(fromEnvironment) ? null : fromEnvironment;
    }

    public void SetApiKey(AppSettings settings, string providerId, string? clearKey)
    {
        ArgumentNullException.ThrowIfNull(settings);

        if (string.IsNullOrWhiteSpace(clearKey))
        {
            settings.ProtectedApiKeys.Remove(providerId);
            return;
        }

        settings.ProtectedApiKeys[providerId] = _protector.Protect(clearKey.Trim());
    }

    /// <summary>Drops the in-memory copy so the next read picks up an external edit.</summary>
    public void Invalidate() => _cached = null;

    public void Dispose() => _gate.Dispose();

    private async Task<AppSettings?> ReadAsync(CancellationToken cancellationToken)
    {
        if (!File.Exists(_path))
        {
            return null;
        }

        try
        {
            await using FileStream stream = File.OpenRead(_path);
            return await JsonSerializer
                .DeserializeAsync<AppSettings>(stream, SafeInvestJson.Storage, cancellationToken)
                .ConfigureAwait(false);
        }
        catch (Exception ex) when (ex is JsonException or IOException)
        {
            return null;
        }
    }
}
