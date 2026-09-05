//! Display projections: numbers already turned into the strings the interface
//! shows, and the direction that decides their colour.
//!
//! Formatting lives here rather than in the front end for two reasons. It is
//! the same in the window and in an MCP answer, so a player and an AI read the
//! same figures; and money formatting is easy to get subtly wrong, so it is
//! worth writing once and testing.

use crate::ops::PortfolioReport;
use jiff::Timestamp;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use safe_invest_core::model::{
    Asset, GoalProgress, GoalStatus, PlayerKind, PositionView, Quote, Trade, TradeSide,
};
use serde::Serialize;

/// Which way a number moved: `1` up, `-1` down, `0` flat or unknown.
///
/// The interface maps this to a colour once, so the green/red decision — and
/// the colour-blind alternative — is made in exactly one place.
pub type Direction = i8;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardView {
    pub game_id: String,
    pub player_name: String,
    pub player_kind: PlayerKind,
    /// True in AI games: the interface goes read-only and becomes an observer.
    pub observer_mode: bool,
    pub currency: String,

    pub total_value: String,
    pub total_value_raw: f64,
    pub cash: String,
    pub invested: String,
    pub total_pnl: String,
    pub total_pnl_percent: String,
    pub direction: Direction,
    pub realized_pnl: String,
    pub unrealized_pnl: String,

    pub positions: Vec<PositionCard>,
    pub goal: Option<GoalView>,

    /// Sources behind the numbers on screen, so the badge can name them.
    pub sources: Vec<String>,
    pub contains_simulated_prices: bool,
    pub unpriced_symbols: Vec<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionCard {
    pub symbol: String,
    pub name: String,
    pub kind: String,
    pub quantity: String,
    pub average_cost: String,
    pub price: Option<String>,
    pub market_value: Option<String>,
    pub pnl: Option<String>,
    pub pnl_percent: Option<String>,
    pub change_percent_24h: Option<String>,
    pub direction: Direction,
    pub change_direction: Direction,
    pub weight_percent: f64,
    pub source_id: Option<String>,
    pub is_simulated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalView {
    pub target_amount: String,
    pub deadline: String,
    pub progress_percent: f64,
    pub amount_remaining: String,
    pub days_remaining: i64,
    pub status: GoalStatus,
    pub status_label: String,
    pub required_return: Option<String>,
    pub achieved_return: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeRow {
    pub id: String,
    pub timestamp: String,
    pub side: TradeSide,
    pub side_label: String,
    pub symbol: String,
    pub name: String,
    pub quantity: String,
    pub unit_price: String,
    pub total: String,
    pub fees: String,
    pub realized_pnl: Option<String>,
    pub direction: Direction,
    /// The AI's justification. Present on every AI trade — the engine refuses
    /// one without it.
    pub rationale: Option<String>,
    pub by_ai: bool,
    pub source_id: Option<String>,
    pub was_simulated: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketRow {
    pub symbol: String,
    pub name: String,
    pub kind: String,
    pub price: Option<String>,
    pub change_percent_24h: Option<String>,
    pub direction: Direction,
    pub source_id: Option<String>,
    pub is_simulated: bool,
}

// ------------------------------------------------------------ projections

pub fn dashboard(report: &PortfolioReport) -> DashboardView {
    let session = &report.session;
    let snapshot = &report.snapshot;
    let currency = &session.currency;

    let mut sources: Vec<String> = snapshot
        .positions
        .iter()
        .filter_map(|p| p.source_id.clone())
        .collect();
    sources.sort_unstable();
    sources.dedup();

    DashboardView {
        game_id: session.id.to_string(),
        player_name: session.player_name.clone(),
        player_kind: session.player_kind,
        observer_mode: session.player_kind == PlayerKind::Ai,
        currency: currency.clone(),

        total_value: money(snapshot.total_value, currency),
        total_value_raw: to_f64(snapshot.total_value),
        cash: money(snapshot.cash, currency),
        invested: money(snapshot.market_value, currency),
        total_pnl: signed_money(snapshot.total_pnl, currency),
        total_pnl_percent: signed_percent(snapshot.total_pnl_percent),
        direction: snapshot.direction(),
        realized_pnl: signed_money(snapshot.realized_pnl, currency),
        unrealized_pnl: signed_money(snapshot.unrealized_pnl, currency),

        positions: snapshot
            .positions
            .iter()
            .map(|p| position(p, currency))
            .collect(),
        goal: report.goal.as_ref().map(|g| goal(g, currency)),

        sources,
        contains_simulated_prices: snapshot.contains_simulated_prices,
        unpriced_symbols: snapshot.unpriced_symbols.clone(),
        updated_at: datetime(snapshot.as_of),
    }
}

pub fn position(view: &PositionView, currency: &str) -> PositionCard {
    PositionCard {
        symbol: view.asset.symbol.clone(),
        name: view.asset.name.clone(),
        kind: view.asset.kind.as_str().to_owned(),
        quantity: quantity(view.quantity),
        average_cost: money(view.average_cost, currency),
        price: view.price.map(|p| money(p, currency)),
        market_value: view.market_value.map(|v| money(v, currency)),
        pnl: view.unrealized_pnl.map(|p| signed_money(p, currency)),
        pnl_percent: view.unrealized_pnl_percent.map(signed_percent),
        change_percent_24h: view.change_percent_24h.map(signed_percent),
        direction: view.direction(),
        change_direction: direction_of(view.change_percent_24h),
        weight_percent: to_f64(view.weight_percent),
        source_id: view.source_id.clone(),
        is_simulated: view.is_simulated,
    }
}

pub fn goal(progress: &GoalProgress, currency: &str) -> GoalView {
    GoalView {
        target_amount: money(progress.target_amount, currency),
        deadline: date(progress.deadline),
        progress_percent: to_f64(progress.progress_percent),
        amount_remaining: money(progress.amount_remaining, currency),
        days_remaining: progress.days_remaining,
        status: progress.status,
        status_label: match progress.status {
            GoalStatus::Achieved => "Objectif atteint",
            GoalStatus::OnTrack => "Dans les temps",
            GoalStatus::Behind => "En retard",
            GoalStatus::Expired => "Échéance dépassée",
        }
        .to_owned(),
        required_return: progress
            .required_annualised_return_percent
            .map(|r| format!("{} /an", signed_percent(r))),
        achieved_return: progress
            .achieved_annualised_return_percent
            .map(|r| format!("{} /an", signed_percent(r))),
    }
}

pub fn trade(trade: &Trade, currency: &str) -> TradeRow {
    TradeRow {
        id: trade.id.to_string(),
        timestamp: datetime(trade.timestamp),
        side: trade.side,
        side_label: match trade.side {
            TradeSide::Buy => "Achat",
            TradeSide::Sell => "Vente",
        }
        .to_owned(),
        symbol: trade.asset.symbol.clone(),
        name: trade.asset.name.clone(),
        quantity: quantity(trade.quantity),
        unit_price: money(trade.unit_price, currency),
        total: money(trade.total, currency),
        fees: money(trade.fees, currency),
        realized_pnl: trade.realized_pnl.map(|p| signed_money(p, currency)),
        direction: direction_of(trade.realized_pnl),
        rationale: trade.rationale.clone(),
        by_ai: trade.actor_kind == PlayerKind::Ai,
        source_id: trade.quote_source_id.clone(),
        was_simulated: trade.quote_was_simulated,
    }
}

pub fn market_row(asset: &Asset, quote: Option<&Quote>, currency: &str) -> MarketRow {
    MarketRow {
        symbol: asset.symbol.clone(),
        name: asset.name.clone(),
        kind: asset.kind.as_str().to_owned(),
        price: quote.map(|q| money(q.price, currency)),
        change_percent_24h: quote.and_then(|q| q.change_percent_24h).map(signed_percent),
        direction: quote.map_or(0, Quote::direction),
        source_id: quote.map(|q| q.source_id.clone()),
        is_simulated: quote.is_some_and(|q| q.is_simulated),
    }
}

// -------------------------------------------------------------- formatting

/// French formatting: a narrow no-break space every three digits, a comma for
/// the decimal point, the currency after the number — `12 345,67 €`.
pub fn money(value: Decimal, currency: &str) -> String {
    format!("{} {}", grouped(value, 2), symbol_for(currency))
}

/// The same, with an explicit `+` on gains so a green figure reads as a gain
/// even to someone who cannot see the green.
pub fn signed_money(value: Decimal, currency: &str) -> String {
    let sign = if value > Decimal::ZERO { "+" } else { "" };
    format!("{sign}{}", money(value, currency))
}

pub fn signed_percent(value: Decimal) -> String {
    let sign = if value > Decimal::ZERO { "+" } else { "" };
    format!("{sign}{} %", grouped(value.round_dp(2), 2))
}

/// Quantities keep as many decimals as they need, up to eight, with no
/// trailing zeros: `0,5 BTC`, not `0,50000000 BTC`.
pub fn quantity(value: Decimal) -> String {
    let trimmed = value.normalize();
    let decimals = trimmed.scale().min(8);
    grouped(trimmed, decimals as usize)
}

pub fn datetime(at: Timestamp) -> String {
    at.strftime("%d/%m/%Y %H:%M").to_string()
}

pub fn date(at: Timestamp) -> String {
    at.strftime("%d/%m/%Y").to_string()
}

fn symbol_for(currency: &str) -> &str {
    match currency.to_uppercase().as_str() {
        "EUR" => "€",
        "USD" => "$",
        "GBP" => "£",
        "CHF" => "CHF",
        _ => "",
    }
}

/// Groups thousands with U+202F, the narrow no-break space French typography
/// uses. A plain space would let a number wrap across two lines.
fn grouped(value: Decimal, decimals: usize) -> String {
    let rendered = format!("{value:.decimals$}");
    let (sign, digits) = match rendered.strip_prefix('-') {
        Some(rest) => ("-", rest),
        None => ("", rendered.as_str()),
    };

    let (whole, fraction) = digits.split_once('.').unwrap_or((digits, ""));

    let mut grouped = String::with_capacity(whole.len() + whole.len() / 3);
    for (index, ch) in whole.chars().enumerate() {
        if index > 0 && (whole.len() - index).is_multiple_of(3) {
            grouped.push('\u{202f}');
        }
        grouped.push(ch);
    }

    if fraction.is_empty() {
        format!("{sign}{grouped}")
    } else {
        format!("{sign}{grouped},{fraction}")
    }
}

fn direction_of(value: Option<Decimal>) -> Direction {
    match value {
        Some(v) if v > Decimal::ZERO => 1,
        Some(v) if v < Decimal::ZERO => -1,
        _ => 0,
    }
}

fn to_f64(value: Decimal) -> f64 {
    value.to_f64().unwrap_or_default()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a test that trips is a test that failed"
)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn d(text: &str) -> Decimal {
        Decimal::from_str(text).unwrap()
    }

    const NBSP: char = '\u{202f}';

    #[test]
    fn money_is_grouped_the_french_way() {
        assert_eq!(money(d("12345.67"), "EUR"), format!("12{NBSP}345,67 €"));
        assert_eq!(money(d("999.5"), "EUR"), "999,50 €");
        assert_eq!(
            money(d("1234567.89"), "USD"),
            format!("1{NBSP}234{NBSP}567,89 $")
        );
    }

    #[test]
    fn a_negative_amount_keeps_its_sign_in_front_of_the_digits() {
        assert_eq!(money(d("-1234.5"), "EUR"), format!("-1{NBSP}234,50 €"));
    }

    #[test]
    fn a_gain_is_marked_with_a_plus_so_colour_is_not_the_only_signal() {
        assert_eq!(signed_money(d("120"), "EUR"), "+120,00 €");
        assert_eq!(signed_money(d("-120"), "EUR"), "-120,00 €");
        assert_eq!(signed_money(Decimal::ZERO, "EUR"), "0,00 €");
        assert_eq!(signed_percent(d("3.456")), "+3,46 %");
    }

    #[test]
    fn quantities_do_not_carry_pointless_zeros() {
        assert_eq!(quantity(d("0.50000000")), "0,5");
        assert_eq!(quantity(d("2")), "2");
        assert_eq!(quantity(d("0.00000001")), "0,00000001");
    }

    #[test]
    fn an_unknown_currency_prints_the_number_without_inventing_a_symbol() {
        assert_eq!(money(d("10"), "JPY"), "10,00 ");
    }

    #[test]
    fn direction_is_zero_when_there_is_nothing_to_compare() {
        assert_eq!(direction_of(None), 0);
        assert_eq!(direction_of(Some(Decimal::ZERO)), 0);
        assert_eq!(direction_of(Some(d("0.01"))), 1);
        assert_eq!(direction_of(Some(d("-0.01"))), -1);
    }
}
