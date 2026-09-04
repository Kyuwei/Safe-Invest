using System.Text.Json.Serialization;

namespace SafeInvest.Core.Models;

/// <summary>
/// A position currently held in a portfolio. Mutable because the engine updates
/// quantity and weighted average cost in place as trades are applied.
/// </summary>
public sealed class Holding
{
    public required Asset Asset { get; set; }

    public decimal Quantity { get; set; }

    /// <summary>Weighted average purchase price per unit, in the session currency.</summary>
    public decimal AverageCost { get; set; }

    [JsonIgnore]
    public decimal CostBasis => Quantity * AverageCost;
}
