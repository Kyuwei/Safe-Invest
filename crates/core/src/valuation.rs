//! Pricing a portfolio at an instant.

use crate::model::{GameSession, PortfolioSnapshot, PositionView, Quote};
use crate::money::{self, round_money};
use jiff::Timestamp;
use rust_decimal::Decimal;
use std::collections::HashMap;

/// Values every holding against `quotes`, keyed by [`crate::model::Asset::key`].
///
/// A holding with no quote is kept in the list with `None` prices rather than
/// dropped or valued at zero — the player has to see that a line could not be
/// priced, not silently watch their portfolio shrink.
pub fn snapshot(
    session: &GameSession,
    quotes: &HashMap<String, Quote>,
    as_of: Timestamp,
) -> PortfolioSnapshot {
    let mut positions = Vec::with_capacity(session.holdings.len());
    let mut unpriced = Vec::new();
    let mut market_value = Decimal::ZERO;
    let mut unrealized = Decimal::ZERO;
    let mut simulated = false;

    for holding in &session.holdings {
        let quote = quotes.get(&holding.asset.key());
        let cost_basis = round_money(holding.cost_basis());

        let value = quote.and_then(|q| money::money_mul(holding.quantity, q.price).ok());
        let pnl = value.and_then(|v| money::sub(v, cost_basis).ok().map(round_money));

        match (quote, value, pnl) {
            (Some(q), Some(v), Some(p)) => {
                market_value = money::add(market_value, v).unwrap_or(market_value);
                unrealized = money::add(unrealized, p).unwrap_or(unrealized);
                simulated |= q.is_simulated;
            }
            _ => unpriced.push(holding.asset.symbol.clone()),
        }

        positions.push(PositionView {
            asset: holding.asset.clone(),
            quantity: holding.quantity,
            average_cost: holding.average_cost,
            cost_basis,
            price: quote.map(|q| q.price),
            market_value: value,
            unrealized_pnl: pnl,
            unrealized_pnl_percent: pnl.and_then(|p| money::percent(p, cost_basis).ok()),
            change_percent_24h: quote.and_then(|q| q.change_percent_24h),
            source_id: quote.map(|q| q.source_id.clone()),
            is_simulated: quote.is_some_and(|q| q.is_simulated),
            quoted_at: quote.map(|q| q.as_of),
            // Filled in below, once the total it is a share of is known.
            weight_percent: Decimal::ZERO,
        });
    }

    let market_value = round_money(market_value);
    let total_value = money::add(session.cash, market_value)
        .map(round_money)
        .unwrap_or(session.cash);
    let total_pnl = money::sub(total_value, session.starting_cash)
        .map(round_money)
        .unwrap_or(Decimal::ZERO);

    for position in &mut positions {
        position.weight_percent = position
            .market_value
            .and_then(|v| money::percent(v, total_value).ok())
            .unwrap_or(Decimal::ZERO);
    }

    PortfolioSnapshot {
        as_of,
        currency: session.currency.clone(),
        cash: session.cash,
        starting_cash: session.starting_cash,
        market_value,
        total_value,
        total_pnl,
        total_pnl_percent: money::percent(total_pnl, session.starting_cash)
            .unwrap_or(Decimal::ZERO),
        realized_pnl: round_money(session.realized_pnl()),
        unrealized_pnl: round_money(unrealized),
        positions,
        contains_simulated_prices: simulated,
        unpriced_symbols: unpriced,
    }
}
