using System.Text.Encodings.Web;
using System.Text.Json;
using System.Text.Json.Serialization;

namespace SafeInvest.Core.Storage;

/// <summary>Single JSON contract shared by the store, the settings and the MCP server.</summary>
public static class SafeInvestJson
{
    public static JsonSerializerOptions Storage { get; } = Create(indented: true);

    /// <summary>Compact form used for MCP tool payloads.</summary>
    public static JsonSerializerOptions Wire { get; } = Create(indented: false);

    private static JsonSerializerOptions Create(bool indented) => new(JsonSerializerDefaults.General)
    {
        WriteIndented = indented,
        PropertyNamingPolicy = JsonNamingPolicy.CamelCase,
        DictionaryKeyPolicy = null,
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
        // Accented French text must stay readable when a human opens the save file.
        Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
        Converters = { new JsonStringEnumConverter() },
    };
}
