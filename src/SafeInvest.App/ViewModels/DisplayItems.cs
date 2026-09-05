using SafeInvest.App.Services;
using SafeInvest.Core.Models;
using SafeInvest.MarketData;

namespace SafeInvest.App.ViewModels;

/// <summary>
/// Rows shown in lists. They are built once from the domain objects and expose only
/// finished strings, so the XAML has nothing to compute and nothing to get wrong.
/// </summary>
public sealed class PositionItem
{
    public PositionItem(PositionView position, string currency)
    {
        ArgumentNullException.ThrowIfNull(position);

        Asset = position.Asset;
        Symbol = position.Asset.Symbol;
        Name = position.Asset.Name;
        Kind = position.Asset.Kind;
        KindLabel = Formatting.KindLabel(position.Asset.Kind);
        QuantityText = Formatting.Quantity(position.Quantity);
        AverageCostText = Formatting.UnitPrice(position.AverageCost, currency);
        PriceText = position.Price is null
            ? "cours indisponible"
            : Formatting.UnitPrice(position.Price.Value, currency);
        MarketValueText = position.MarketValue is null
            ? "—"
            : Formatting.Money(position.MarketValue.Value, currency);
        PnLText = position.UnrealizedPnL is null
            ? "—"
            : Formatting.MoneySigned(position.UnrealizedPnL.Value, currency);
        PnLPercentText = Formatting.Percent(position.UnrealizedPnLPercent);
        Change24hText = position.ChangePercent24h is null
            ? "24 h : —"
            : $"24 h : {Formatting.Percent(position.ChangePercent24h)}";
        WeightText = $"{position.WeightPercent:N1} % du portefeuille";
        DetailsText = $"{QuantityText} unités à {AverageCostText} en moyenne · {WeightText}";
        Direction = position.Direction;
        IsSimulated = position.IsSimulated;
        SourceText = position.IsSimulated
            ? "Cours simulé — aucune source réelle n'a répondu"
            : $"Source : {Formatting.SourceLabel(position.SourceId)}";
    }

    /// <summary>
    /// Internal on purpose. The XAML type-info generator builds an activator for every
    /// public property type of a bindable class, and Asset has `required` members, so the
    /// generated `new Asset()` does not compile. Nothing in XAML needs the domain object —
    /// only the code-behind does, and that lives in this assembly.
    /// </summary>
    internal Asset Asset { get; }

    public string Symbol { get; }

    public string Name { get; }

    public AssetKind Kind { get; }

    public string KindLabel { get; }

    public string QuantityText { get; }

    public string AverageCostText { get; }

    public string PriceText { get; }

    public string MarketValueText { get; }

    public string PnLText { get; }

    public string PnLPercentText { get; }

    public string Change24hText { get; }

    public string WeightText { get; }

    /// <summary>
    /// The whole secondary line, composed here rather than from several &lt;Run&gt; fragments
    /// in the template: one binding is both cheaper and easier to keep readable.
    /// </summary>
    public string DetailsText { get; }

    public int Direction { get; }

    public bool IsSimulated { get; }

    public string SourceText { get; }
}

/// <summary>A row on the market screen: an asset with its latest price, if we have one.</summary>
public sealed class MarketItem
{
    public MarketItem(Asset asset, Quote? quote, string currency, decimal heldQuantity)
    {
        ArgumentNullException.ThrowIfNull(asset);

        Asset = asset;
        Symbol = asset.Symbol;
        Name = asset.Name;
        Kind = asset.Kind;
        KindLabel = Formatting.KindLabel(asset.Kind);
        HasQuote = quote is not null;
        PriceText = quote is null ? "cours indisponible" : Formatting.UnitPrice(quote.Price, currency);
        ChangeText = quote?.ChangePercent24h is null ? "—" : Formatting.Percent(quote.ChangePercent24h);
        Direction = quote?.Direction ?? 0;
        IsSimulated = quote?.IsSimulated ?? false;
        SourceText = quote is null ? string.Empty : Formatting.SourceLabel(quote.SourceId);
        HeldQuantity = heldQuantity;
        HoldingText = heldQuantity > 0m ? $"Détenu : {Formatting.Quantity(heldQuantity)}" : string.Empty;
        IsHeld = heldQuantity > 0m;
    }

    /// <summary>Internal for the same reason as on <see cref="PositionItem"/>.</summary>
    internal Asset Asset { get; }

    public string Symbol { get; }

    public string Name { get; }

    public AssetKind Kind { get; }

    public string KindLabel { get; }

    public bool HasQuote { get; }

    public string PriceText { get; }

    public string ChangeText { get; }

    public int Direction { get; }

    public bool IsSimulated { get; }

    public string SourceText { get; }

    public decimal HeldQuantity { get; }

    public string HoldingText { get; }

    public bool IsHeld { get; }
}

/// <summary>A line of the history: what was done, when, at what price and — for an AI — why.</summary>
public sealed class TradeItem
{
    public TradeItem(Trade trade, string currency, DateTimeOffset now)
    {
        ArgumentNullException.ThrowIfNull(trade);

        Symbol = trade.Asset.Symbol;
        Name = trade.Asset.Name;
        Kind = trade.Asset.Kind;
        KindLabel = Formatting.KindLabel(trade.Asset.Kind);
        SideLabel = Formatting.SideLabel(trade.Side);
        IsBuy = trade.Side == TradeSide.Buy;
        WhenText = Formatting.DateTime(trade.Timestamp);
        RelativeText = Formatting.RelativeTime(trade.Timestamp, now);
        QuantityText = Formatting.Quantity(trade.Quantity);
        UnitPriceText = Formatting.UnitPrice(trade.UnitPrice, currency);
        TotalText = Formatting.Money(trade.Total, currency);
        FeesText = trade.Fees > 0m ? $"dont {Formatting.Money(trade.Fees, currency)} de frais" : string.Empty;
        RealizedText = trade.RealizedPnL is null
            ? string.Empty
            : $"Résultat réalisé : {Formatting.MoneySigned(trade.RealizedPnL.Value, currency)}";
        Direction = trade.RealizedPnL is null ? 0 : Math.Sign(trade.RealizedPnL.Value);
        Rationale = trade.Rationale ?? string.Empty;
        ByAi = trade.ActorKind == PlayerKind.Ai;
        ByLabel = Formatting.PlayerLabel(trade.ActorKind);
        WasSimulated = trade.QuoteWasSimulated;
        Summary = $"{SideLabel} de {QuantityText} {Symbol} à {UnitPriceText}";
        HeadlineText = $"{QuantityText} {Symbol} à {UnitPriceText}";
        DetailsText = $"{Name} · {WhenText} · par {ByLabel}";
    }

    public string Symbol { get; }

    public string Name { get; }

    public AssetKind Kind { get; }

    public string KindLabel { get; }

    public string SideLabel { get; }

    public bool IsBuy { get; }

    public string WhenText { get; }

    public string RelativeText { get; }

    public string QuantityText { get; }

    public string UnitPriceText { get; }

    public string TotalText { get; }

    public string FeesText { get; }

    public string RealizedText { get; }

    public int Direction { get; }

    public string Rationale { get; }

    public bool ByAi { get; }

    public string ByLabel { get; }

    public bool WasSimulated { get; }

    public string Summary { get; }

    /// <summary>"0,043 BTC à 68 000,00 €" — the line the history leads with.</summary>
    public string HeadlineText { get; }

    public string DetailsText { get; }
}

/// <summary>A saved game on the start menu.</summary>
public sealed class GameCardItem
{
    public GameCardItem(GameSummary summary, DateTimeOffset now)
    {
        ArgumentNullException.ThrowIfNull(summary);

        Id = summary.Id;
        PlayerName = summary.PlayerName;
        IsAi = summary.PlayerKind == PlayerKind.Ai;
        PlayerLabel = Formatting.PlayerLabel(summary.PlayerKind);
        StartingCashText = Formatting.Money(summary.StartingCash, summary.Currency);
        CashText = Formatting.Money(summary.Cash, summary.Currency);
        UpdatedText = $"Dernière activité {Formatting.RelativeTime(summary.UpdatedAt, now)}";
        TradesText = summary.TradeCount switch
        {
            0 => "aucune opération",
            1 => "1 opération",
            _ => $"{summary.TradeCount} opérations",
        };
        HoldingsText = summary.HoldingCount switch
        {
            0 => "portefeuille vide",
            1 => "1 ligne en portefeuille",
            _ => $"{summary.HoldingCount} lignes en portefeuille",
        };
        DetailsText = $"{HoldingsText} · {TradesText} · {UpdatedText}";
        GoalText = summary.Goal is null
            ? string.Empty
            : $"Objectif : {Formatting.Money(summary.Goal.TargetAmount, summary.Currency)} " +
              $"avant le {Formatting.ShortDate(summary.Goal.Deadline)}";
        HasGoal = summary.Goal is not null;
    }

    public Guid Id { get; }

    public string PlayerName { get; }

    public bool IsAi { get; }

    public string PlayerLabel { get; }

    public string StartingCashText { get; }

    public string CashText { get; }

    public string UpdatedText { get; }

    public string TradesText { get; }

    public string HoldingsText { get; }

    public string DetailsText { get; }

    public string GoalText { get; }

    public bool HasGoal { get; }
}

/// <summary>One market data source on the settings screen.</summary>
public sealed class SourceItem
{
    public SourceItem(ProviderStatus status)
    {
        ArgumentNullException.ThrowIfNull(status);

        Id = status.Id;
        DisplayName = status.DisplayName;
        IsSimulated = status.IsSimulated;

        StateText = status switch
        {
            { IsConfigured: false } => "Clé API manquante — source ignorée",
            { LastCallSucceeded: null } => "Pas encore sollicitée",
            { LastCallSucceeded: true } => $"Dernier appel réussi ({Formatting.DateTime(status.LastCallAt!.Value)})",
            _ => $"Dernier appel en échec : {status.LastError}",
        };

        Direction = status switch
        {
            { IsConfigured: false } => 0,
            { LastCallSucceeded: true } => 1,
            { LastCallSucceeded: false } => -1,
            _ => 0,
        };
    }

    public string Id { get; }

    public string DisplayName { get; }

    public bool IsSimulated { get; }

    public string StateText { get; }

    public int Direction { get; }
}
