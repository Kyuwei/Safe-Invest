namespace SafeInvest.Core.Storage;

/// <summary>
/// Where Safe Invest keeps its data. The WinUI app and the MCP server both resolve the
/// same folder, which is what lets an AI play a game the app is displaying live.
/// </summary>
public static class SafeInvestPaths
{
    public const string EnvironmentOverride = "SAFEINVEST_DATA_DIR";

    public static string Root
    {
        get
        {
            string? overridden = Environment.GetEnvironmentVariable(EnvironmentOverride);
            if (!string.IsNullOrWhiteSpace(overridden))
            {
                return overridden;
            }

            string localAppData = Environment.GetFolderPath(
                Environment.SpecialFolder.LocalApplicationData,
                Environment.SpecialFolderOption.Create);

            // LocalApplicationData can come back empty on some Unix setups.
            if (string.IsNullOrWhiteSpace(localAppData))
            {
                localAppData = Path.Combine(
                    Environment.GetFolderPath(Environment.SpecialFolder.UserProfile),
                    ".local",
                    "share");
            }

            return Path.Combine(localAppData, "SafeInvest");
        }
    }

    public static string GamesDirectory => Path.Combine(Root, "games");

    public static string SettingsFile => Path.Combine(Root, "settings.json");

    public static string CurrentGameFile => Path.Combine(Root, "current.json");

    public static string LockFile => Path.Combine(Root, ".store.lock");

    public static string GameFile(Guid id) => Path.Combine(GamesDirectory, $"{id:N}.json");

    public static void EnsureCreated()
    {
        Directory.CreateDirectory(Root);
        Directory.CreateDirectory(GamesDirectory);
    }
}
