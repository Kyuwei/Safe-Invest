namespace SafeInvest.Core.Engine;

/// <summary>
/// Rounding rules used everywhere money or quantities are computed. Kept in one place
/// so the engine, the MCP server and the UI never disagree on the last cent.
/// </summary>
public static class MoneyMath
{
    public const int MoneyDecimals = 2;

    /// <summary>Enough precision for fractional crypto (1 satoshi is 1e-8 BTC).</summary>
    public const int QuantityDecimals = 8;

    public static decimal RoundMoney(decimal value) =>
        Math.Round(value, MoneyDecimals, MidpointRounding.AwayFromZero);

    /// <summary>
    /// Quantities are always rounded down: buying "as much as 100 € allows" must never
    /// round up into spending 100.01 €.
    /// </summary>
    public static decimal RoundQuantityDown(decimal value) =>
        Math.Round(value, QuantityDecimals, MidpointRounding.ToZero);

    public static decimal Percent(decimal part, decimal whole) =>
        whole == 0m ? 0m : Math.Round(part / whole * 100m, 4, MidpointRounding.AwayFromZero);
}
