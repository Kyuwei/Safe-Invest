using System.Security.Cryptography;
using System.Text;

namespace SafeInvest.Core.Storage;

/// <summary>Encrypts API keys before they touch the settings file.</summary>
public interface ISecretProtector
{
    string Protect(string plainText);

    string Unprotect(string protectedText);
}

/// <summary>
/// Picks the strongest protector the current OS offers. Windows gets DPAPI, bound to the
/// signed-in user; everywhere else (Linux CI, unit tests) falls back to a passthrough so
/// the rest of the code has no platform branches.
/// </summary>
public static class SecretProtectorFactory
{
    public static ISecretProtector Create() =>
        OperatingSystem.IsWindows() ? new DpapiSecretProtector() : new PassthroughSecretProtector();
}

/// <summary>DPAPI-backed protection, scoped to the current Windows user.</summary>
public sealed class DpapiSecretProtector : ISecretProtector
{
    private static readonly byte[] Entropy = Encoding.UTF8.GetBytes("SafeInvest.ApiKeys.v1");

    public string Protect(string plainText)
    {
        ArgumentNullException.ThrowIfNull(plainText);

        if (!OperatingSystem.IsWindows())
        {
            return plainText;
        }

        byte[] encrypted = ProtectedData.Protect(
            Encoding.UTF8.GetBytes(plainText),
            Entropy,
            DataProtectionScope.CurrentUser);

        return Convert.ToBase64String(encrypted);
    }

    public string Unprotect(string protectedText)
    {
        ArgumentNullException.ThrowIfNull(protectedText);

        if (!OperatingSystem.IsWindows())
        {
            return protectedText;
        }

        try
        {
            byte[] decrypted = ProtectedData.Unprotect(
                Convert.FromBase64String(protectedText),
                Entropy,
                DataProtectionScope.CurrentUser);

            return Encoding.UTF8.GetString(decrypted);
        }
        catch (Exception ex) when (ex is FormatException or CryptographicException)
        {
            // Key written by another user account, or the settings file was hand-edited.
            return string.Empty;
        }
    }
}

/// <summary>No-op protector for non-Windows hosts and tests.</summary>
public sealed class PassthroughSecretProtector : ISecretProtector
{
    public string Protect(string plainText) => plainText;

    public string Unprotect(string protectedText) => protectedText;
}
