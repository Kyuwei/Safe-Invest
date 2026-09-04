using SafeInvest.Core.Models;

namespace SafeInvest.Core.Engine;

/// <summary>
/// Applies buys and sells to a <see cref="GameSession"/>. Every rule of the game lives
/// here so that the WinUI app and the MCP server behave identically — the UI and the AI
/// go through this same code path.
/// </summary>
public sealed class PortfolioEngine(TimeProvider timeProvider)
{
    private readonly TimeProvider _timeProvider = timeProvider;

    public PortfolioEngine()
        : this(TimeProvider.System)
    {
    }

    /// <summary>
    /// Buys either an exact <paramref name="quantity"/> of units, or as many units as
    /// <paramref name="amount"/> of cash allows (fees included). Exactly one of the two
    /// must be supplied.
    /// </summary>
    public Trade Buy(
        GameSession session,
        Asset asset,
        Quote quote,
        decimal? quantity = null,
        decimal? amount = null,
        string? rationale = null)
    {
        ArgumentNullException.ThrowIfNull(session);
        ArgumentNullException.ThrowIfNull(asset);
        ArgumentNullException.ThrowIfNull(quote);

        ValidateQuote(session, asset, quote);
        string? checkedRationale = ValidateRationale(session, rationale);

        decimal feeRate = FeeRate(session);
        decimal units = quantity is not null
            ? RequirePositiveQuantity(quantity.Value)
            : UnitsAffordableFor(RequirePositiveAmount(amount), quote.Price, feeRate);

        if (units <= 0m)
        {
            throw new TradeValidationException(
                "Le montant est trop faible pour acheter ne serait-ce qu'une fraction de cet actif.");
        }

        decimal gross = MoneyMath.RoundMoney(units * quote.Price);
        decimal fees = MoneyMath.RoundMoney(gross * feeRate);
        decimal total = MoneyMath.RoundMoney(gross + fees);

        if (total > session.Cash)
        {
            throw new TradeValidationException(
                $"Trésorerie insuffisante : il faudrait {total:N2} {session.Currency} " +
                $"mais il ne reste que {session.Cash:N2} {session.Currency}.");
        }

        Holding? holding = session.FindHolding(asset.Kind, asset.Symbol);
        if (holding is null)
        {
            holding = new Holding { Asset = asset, Quantity = 0m, AverageCost = 0m };
            session.Holdings.Add(holding);
        }
        else
        {
            // Refresh the stored metadata (name, logo, provider id) with what we just used.
            holding.Asset = asset;
        }

        decimal newQuantity = holding.Quantity + units;
        holding.AverageCost = newQuantity == 0m
            ? 0m
            : (holding.CostBasis + gross) / newQuantity;
        holding.Quantity = newQuantity;

        session.Cash = MoneyMath.RoundMoney(session.Cash - total);

        Trade trade = new()
        {
            Id = Guid.NewGuid(),
            Timestamp = _timeProvider.GetUtcNow(),
            Side = TradeSide.Buy,
            Asset = asset,
            Quantity = units,
            UnitPrice = quote.Price,
            Fees = fees,
            Total = total,
            RealizedPnL = null,
            Rationale = checkedRationale,
            ActorKind = session.PlayerKind,
            QuoteSourceId = quote.SourceId,
            QuoteWasSimulated = quote.IsSimulated,
        };

        Commit(session, trade);
        return trade;
    }

    /// <summary>
    /// Sells either an exact <paramref name="quantity"/>, or enough units to raise
    /// <paramref name="amount"/> of cash, or the whole position when
    /// <paramref name="sellAll"/> is set.
    /// </summary>
    public Trade Sell(
        GameSession session,
        Asset asset,
        Quote quote,
        decimal? quantity = null,
        decimal? amount = null,
        bool sellAll = false,
        string? rationale = null)
    {
        ArgumentNullException.ThrowIfNull(session);
        ArgumentNullException.ThrowIfNull(asset);
        ArgumentNullException.ThrowIfNull(quote);

        ValidateQuote(session, asset, quote);
        string? checkedRationale = ValidateRationale(session, rationale);

        Holding holding = session.FindHolding(asset.Kind, asset.Symbol)
            ?? throw new TradeValidationException(
                $"Aucune position sur {asset.Symbol} : impossible de vendre ce que l'on ne détient pas.");

        decimal feeRate = FeeRate(session);
        decimal units = sellAll
            ? holding.Quantity
            : quantity is not null
                ? RequirePositiveQuantity(quantity.Value)
                : MoneyMath.RoundQuantityDown(RequirePositiveAmount(amount) / quote.Price);

        if (units <= 0m)
        {
            throw new TradeValidationException("La quantité à vendre doit être strictement positive.");
        }

        // Absorb the rounding dust so "sell everything" never leaves 1e-9 units behind.
        if (units > holding.Quantity)
        {
            if (units - holding.Quantity > 0.00000001m)
            {
                throw new TradeValidationException(
                    $"Quantité insuffisante : {units} {asset.Symbol} demandés " +
                    $"mais seulement {holding.Quantity} détenus.");
            }

            units = holding.Quantity;
        }

        decimal gross = MoneyMath.RoundMoney(units * quote.Price);
        decimal fees = MoneyMath.RoundMoney(gross * feeRate);
        decimal proceeds = MoneyMath.RoundMoney(gross - fees);
        decimal realized = MoneyMath.RoundMoney((quote.Price - holding.AverageCost) * units - fees);

        holding.Quantity = MoneyMath.RoundQuantityDown(holding.Quantity - units);
        if (holding.Quantity <= 0m)
        {
            session.Holdings.Remove(holding);
        }

        session.Cash = MoneyMath.RoundMoney(session.Cash + proceeds);

        Trade trade = new()
        {
            Id = Guid.NewGuid(),
            Timestamp = _timeProvider.GetUtcNow(),
            Side = TradeSide.Sell,
            Asset = asset,
            Quantity = units,
            UnitPrice = quote.Price,
            Fees = fees,
            Total = proceeds,
            RealizedPnL = realized,
            Rationale = checkedRationale,
            ActorKind = session.PlayerKind,
            QuoteSourceId = quote.SourceId,
            QuoteWasSimulated = quote.IsSimulated,
        };

        Commit(session, trade);
        return trade;
    }

    private void Commit(GameSession session, Trade trade)
    {
        session.Trades.Add(trade);
        session.UpdatedAt = _timeProvider.GetUtcNow();
    }

    private static decimal FeeRate(GameSession session)
    {
        if (session.FeePercent < 0m)
        {
            throw new TradeValidationException("Le taux de frais ne peut pas être négatif.");
        }

        return session.FeePercent / 100m;
    }

    private static decimal UnitsAffordableFor(decimal amount, decimal price, decimal feeRate)
    {
        if (price <= 0m)
        {
            throw new TradeValidationException("Le cours de l'actif est nul ou négatif : achat impossible.");
        }

        return MoneyMath.RoundQuantityDown(amount / (price * (1m + feeRate)));
    }

    private static decimal RequirePositiveQuantity(decimal quantity)
    {
        decimal rounded = MoneyMath.RoundQuantityDown(quantity);
        if (rounded <= 0m)
        {
            throw new TradeValidationException("La quantité doit être strictement positive.");
        }

        return rounded;
    }

    private static decimal RequirePositiveAmount(decimal? amount)
    {
        if (amount is null)
        {
            throw new TradeValidationException(
                "Précisez soit une quantité (quantity), soit un montant (amount).");
        }

        if (amount.Value <= 0m)
        {
            throw new TradeValidationException("Le montant doit être strictement positif.");
        }

        return amount.Value;
    }

    private static void ValidateQuote(GameSession session, Asset asset, Quote quote)
    {
        if (!string.Equals(quote.Currency, session.Currency, StringComparison.OrdinalIgnoreCase))
        {
            throw new TradeValidationException(
                $"Le cours est en {quote.Currency} alors que la partie est en {session.Currency}.");
        }

        if (quote.Key != asset.Key)
        {
            throw new TradeValidationException(
                $"Le cours fourni concerne {quote.Key} et non {asset.Key}.");
        }

        if (quote.Price <= 0m)
        {
            throw new TradeValidationException($"Cours invalide pour {asset.Symbol} : {quote.Price}.");
        }
    }

    /// <summary>
    /// An AI has to say why it trades. This is the whole point of the AI mode: the
    /// history must read as a chain of justified decisions, not a list of numbers.
    /// </summary>
    private static string? ValidateRationale(GameSession session, string? rationale)
    {
        string? trimmed = string.IsNullOrWhiteSpace(rationale) ? null : rationale.Trim();

        if (session.PlayerKind == PlayerKind.Ai && trimmed is null)
        {
            throw new TradeValidationException(
                "En partie IA, chaque opération doit être accompagnée d'une justification (rationale).");
        }

        return trimmed;
    }
}
