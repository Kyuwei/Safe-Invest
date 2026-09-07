//! The rules of the game.
//!
//! Every buy and every sell — from the window, from the MCP server, from a
//! test — goes through this module. That is deliberate: the human and the AI
//! must be playing the same game, and there is exactly one place where a rule
//! can be written down.

use crate::model::{Asset, GameSession, Holding, PlayerKind, Quote, Trade, TradeSide};
use crate::money::{self, round_money, round_quantity_down};
use jiff::Timestamp;
use rust_decimal::Decimal;
use uuid::Uuid;

/// Quantities below this are rounding dust, not a position.
const DUST: Decimal = Decimal::from_parts(1, 0, 0, false, 8); // 1e-8

#[derive(Debug, thiserror::Error)]
pub enum TradeError {
    /// A rule the player broke, phrased for the player.
    #[error("{0}")]
    Rejected(String),
    #[error("calcul impossible : {0}")]
    Overflow(#[from] money::Overflow),
}

impl TradeError {
    fn rejected(message: impl Into<String>) -> Self {
        Self::Rejected(message.into())
    }
}

type Result<T> = core::result::Result<T, TradeError>;

/// How much to trade. Modelled as a choice rather than two optional arguments,
/// so "neither given" and "both given" cannot be expressed at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeAmount {
    /// An exact number of units.
    Units(Decimal),
    /// As many units as this much cash buys — or enough units to raise it.
    Cash(Decimal),
    /// The entire position. Selling only.
    All,
}

/// Buys into `session` and returns the recorded trade.
pub fn buy(
    session: &mut GameSession,
    asset: &Asset,
    quote: &Quote,
    amount: TradeAmount,
    rationale: Option<&str>,
    now: Timestamp,
) -> Result<Trade> {
    validate_open(session)?;
    validate_quote(session, asset, quote)?;
    let rationale = validate_rationale(session, rationale)?;
    let fee_rate = fee_rate(session)?;

    let units = match amount {
        TradeAmount::Units(q) => require_positive_quantity(q)?,
        TradeAmount::Cash(cash) => {
            units_affordable_for(require_positive_amount(cash)?, quote.price, fee_rate)?
        }
        TradeAmount::All => {
            return Err(TradeError::rejected(
                "« Tout » n'a pas de sens à l'achat : indiquez une quantité ou un montant.",
            ));
        }
    };

    if units <= Decimal::ZERO {
        return Err(TradeError::rejected(
            "Le montant est trop faible pour acheter ne serait-ce qu'une fraction de cet actif.",
        ));
    }

    let gross = money::money_mul(units, quote.price)?;
    let fees = money::money_mul(gross, fee_rate)?;
    let total = round_money(money::add(gross, fees)?);

    if total > session.cash {
        return Err(TradeError::rejected(format!(
            "Trésorerie insuffisante : il faudrait {total} {currency} mais il ne reste que {cash} {currency}.",
            currency = session.currency,
            cash = session.cash,
        )));
    }

    // Upsert the position, refreshing the stored metadata with what we just used
    // (a name or logo the catalogue did not have when the position was opened).
    if let Some(existing) = session.find_holding_mut(asset.kind, &asset.symbol) {
        existing.asset = asset.clone();
    } else {
        session.holdings.push(Holding {
            asset: asset.clone(),
            quantity: Decimal::ZERO,
            average_cost: Decimal::ZERO,
        });
    }
    let holding = session
        .find_holding_mut(asset.kind, &asset.symbol)
        .ok_or_else(|| TradeError::rejected("position introuvable juste après sa création"))?;

    let new_quantity = money::add(holding.quantity, units)?;
    holding.average_cost = if new_quantity.is_zero() {
        Decimal::ZERO
    } else {
        money::div(money::add(holding.cost_basis(), gross)?, new_quantity)?
    };
    holding.quantity = new_quantity;

    session.cash = round_money(money::sub(session.cash, total)?);

    let trade = Trade {
        id: Uuid::new_v4(),
        timestamp: now,
        side: TradeSide::Buy,
        asset: asset.clone(),
        quantity: units,
        unit_price: quote.price,
        fees,
        total,
        realized_pnl: None,
        rationale,
        actor_kind: session.player_kind,
        quote_source_id: Some(quote.source_id.clone()),
        quote_was_simulated: quote.is_simulated,
    };
    commit(session, trade.clone(), now);
    Ok(trade)
}

/// Sells out of `session` and returns the recorded trade.
pub fn sell(
    session: &mut GameSession,
    asset: &Asset,
    quote: &Quote,
    amount: TradeAmount,
    rationale: Option<&str>,
    now: Timestamp,
) -> Result<Trade> {
    validate_open(session)?;
    validate_quote(session, asset, quote)?;
    let rationale = validate_rationale(session, rationale)?;
    let fee_rate = fee_rate(session)?;

    let held = session
        .find_holding(asset.kind, &asset.symbol)
        .ok_or_else(|| {
            TradeError::rejected(format!(
                "Aucune position sur {} : impossible de vendre ce que l'on ne détient pas.",
                asset.symbol
            ))
        })?
        .clone();

    let mut units = match amount {
        TradeAmount::All => held.quantity,
        TradeAmount::Units(q) => require_positive_quantity(q)?,
        TradeAmount::Cash(cash) => {
            round_quantity_down(money::div(require_positive_amount(cash)?, quote.price)?)
        }
    };

    if units <= Decimal::ZERO {
        return Err(TradeError::rejected(
            "La quantité à vendre doit être strictement positive.",
        ));
    }

    // Absorb rounding dust so "sell everything" never leaves 1e-9 units behind.
    if units > held.quantity {
        if money::sub(units, held.quantity)? > DUST {
            return Err(TradeError::rejected(format!(
                "Quantité insuffisante : {units} {symbol} demandés mais seulement {have} détenus.",
                symbol = asset.symbol,
                have = held.quantity,
            )));
        }
        units = held.quantity;
    }

    let gross = money::money_mul(units, quote.price)?;
    let fees = money::money_mul(gross, fee_rate)?;
    let proceeds = round_money(money::sub(gross, fees)?);
    let realized = round_money(money::sub(
        money::mul(money::sub(quote.price, held.average_cost)?, units)?,
        fees,
    )?);

    let remaining = round_quantity_down(money::sub(held.quantity, units)?);
    if remaining <= Decimal::ZERO {
        session.holdings.retain(|h| h.asset.key() != asset.key());
    } else if let Some(holding) = session.find_holding_mut(asset.kind, &asset.symbol) {
        holding.quantity = remaining;
    }

    session.cash = round_money(money::add(session.cash, proceeds)?);

    let trade = Trade {
        id: Uuid::new_v4(),
        timestamp: now,
        side: TradeSide::Sell,
        asset: asset.clone(),
        quantity: units,
        unit_price: quote.price,
        fees,
        total: proceeds,
        realized_pnl: Some(realized),
        rationale,
        actor_kind: session.player_kind,
        quote_source_id: Some(quote.source_id.clone()),
        quote_was_simulated: quote.is_simulated,
    };
    commit(session, trade.clone(), now);
    Ok(trade)
}

fn commit(session: &mut GameSession, trade: Trade, now: Timestamp) {
    session.trades.push(trade);
    session.updated_at = now;
}

fn fee_rate(session: &GameSession) -> Result<Decimal> {
    if session.fee_percent < Decimal::ZERO {
        return Err(TradeError::rejected(
            "Le taux de frais ne peut pas être négatif.",
        ));
    }
    Ok(money::div(session.fee_percent, Decimal::ONE_HUNDRED)?)
}

fn units_affordable_for(amount: Decimal, price: Decimal, fee_rate: Decimal) -> Result<Decimal> {
    if price <= Decimal::ZERO {
        return Err(TradeError::rejected(
            "Le cours de l'actif est nul ou négatif : achat impossible.",
        ));
    }
    let with_fees = money::mul(price, money::add(Decimal::ONE, fee_rate)?)?;
    Ok(round_quantity_down(money::div(amount, with_fees)?))
}

fn require_positive_quantity(quantity: Decimal) -> Result<Decimal> {
    let rounded = round_quantity_down(quantity);
    if rounded <= Decimal::ZERO {
        return Err(TradeError::rejected(
            "La quantité doit être strictement positive.",
        ));
    }
    Ok(rounded)
}

fn require_positive_amount(amount: Decimal) -> Result<Decimal> {
    if amount <= Decimal::ZERO {
        return Err(TradeError::rejected(
            "Le montant doit être strictement positif.",
        ));
    }
    Ok(amount)
}

/// Refuses to touch a game that is over.
///
/// The summary quotes a final value taken at the moment the game stopped. One
/// more trade afterwards would make that number a lie about a portfolio that
/// had since changed, so the door closes here rather than in each caller.
fn validate_open(session: &GameSession) -> Result<()> {
    if let Some(outcome) = session.outcome {
        return Err(TradeError::rejected(format!(
            "Cette partie est terminée ({}). Commencez-en une nouvelle pour continuer à jouer.",
            outcome.reason.label().to_lowercase()
        )));
    }
    Ok(())
}

fn validate_quote(session: &GameSession, asset: &Asset, quote: &Quote) -> Result<()> {
    if !quote.currency.eq_ignore_ascii_case(&session.currency) {
        return Err(TradeError::rejected(format!(
            "Le cours est en {} alors que la partie est en {}.",
            quote.currency, session.currency
        )));
    }
    if quote.key() != asset.key() {
        return Err(TradeError::rejected(format!(
            "Le cours fourni concerne {} et non {}.",
            quote.key(),
            asset.key()
        )));
    }
    if quote.price <= Decimal::ZERO {
        return Err(TradeError::rejected(format!(
            "Cours invalide pour {} : {}.",
            asset.symbol, quote.price
        )));
    }
    Ok(())
}

/// An AI has to say why it trades. This is the whole point of AI mode: the
/// history must read as a chain of justified decisions.
fn validate_rationale(session: &GameSession, rationale: Option<&str>) -> Result<Option<String>> {
    let trimmed = rationale
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned);

    if session.player_kind == PlayerKind::Ai && trimmed.is_none() {
        return Err(TradeError::rejected(
            "En partie IA, chaque opération doit être accompagnée d'une justification (rationale).",
        ));
    }
    Ok(trimmed)
}
