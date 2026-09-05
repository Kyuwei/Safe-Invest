namespace SafeInvest.Core.Models;

/// <summary>Family an investable asset belongs to.</summary>
public enum AssetKind
{
    Crypto,
    Stock,
    Etf,
}

/// <summary>Who is playing a game session.</summary>
public enum PlayerKind
{
    Human,
    Ai,
}

/// <summary>Direction of a trade.</summary>
public enum TradeSide
{
    Buy,
    Sell,
}

/// <summary>Where a session stands against its target amount and deadline.</summary>
public enum GoalStatus
{
    /// <summary>The session has no goal set.</summary>
    None,

    /// <summary>The target amount has been reached.</summary>
    Achieved,

    /// <summary>Growth so far is at least what the goal requires.</summary>
    OnTrack,

    /// <summary>Growth so far is below what the goal requires.</summary>
    Behind,

    /// <summary>The deadline passed without the target being reached.</summary>
    Expired,
}
