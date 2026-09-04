namespace SafeInvest.Core.Engine;

/// <summary>
/// A trade was refused by the rules of the game (not enough cash, missing rationale…).
/// Messages are written in French because they are surfaced verbatim in the UI and
/// returned to MCP clients.
/// </summary>
public sealed class TradeValidationException : Exception
{
    public TradeValidationException(string message)
        : base(message)
    {
    }

    public TradeValidationException(string message, Exception innerException)
        : base(message, innerException)
    {
    }
}
