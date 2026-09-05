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
    Asset, AssetKind, GoalProgress, GoalStatus, PlayerKind, PositionView, Quote, Trade, TradeSide,
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

    /// Cash as a share of everything, so the dashboard can say how much of the
    /// portfolio is still sitting still.
    pub cash_percent: f64,

    pub positions: Vec<PositionCard>,
    /// How the money is spread, largest slice first, cash included. Slices sum
    /// to a hundred: leaving cash out would draw a "répartition" of the money
    /// that happens to be invested, which is not the question being asked.
    pub allocation: Vec<AllocationSlice>,
    /// The position that has gained the most, when anything has gained at all.
    pub best_position: Option<BestPosition>,
    pub goal: Option<GoalView>,
    /// The portfolio's value over time, oldest first, for the dashboard curve.
    /// Empty until the game has been open long enough to record a second point.
    pub value_history: Vec<f64>,
    /// The span the curve covers, in words — "sur les 30 derniers jours".
    pub value_history_label: String,

    /// Sources behind the numbers on screen, so the badge can name them.
    pub sources: Vec<String>,
    pub contains_simulated_prices: bool,
    pub unpriced_symbols: Vec<String>,
    pub updated_at: String,
}

/// One band of the allocation bar: a kind of asset, or the cash left over.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AllocationSlice {
    /// `crypto`, `stock`, `etf`, or `cash` — the interface colours by this.
    pub kind: String,
    pub label: String,
    pub percent: f64,
    pub value: String,
}

/// The best line in the portfolio, named on the dashboard.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BestPosition {
    pub symbol: String,
    pub name: String,
    pub pnl_percent: String,
    pub direction: Direction,
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
    /// The same price as a plain number, for the trade dialog's live estimate.
    /// Display only — the engine recomputes everything from `Decimal`.
    pub price_raw: Option<f64>,
    pub quantity_raw: f64,
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
    pub kind: String,
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

/// One asset's page: the price, its recent shape, and what is already held.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetView {
    pub symbol: String,
    pub name: String,
    pub kind: String,
    pub kind_label: String,
    pub price: Option<String>,
    pub price_raw: Option<f64>,
    pub change_percent_24h: Option<String>,
    pub direction: Direction,
    pub source_id: Option<String>,
    pub is_simulated: bool,
    pub quoted_at: Option<String>,
    /// Daily closes, oldest first.
    pub history: Vec<f64>,
    pub history_days: u16,
    /// The move over the window the history covers.
    pub period_change: Option<String>,
    pub period_direction: Direction,
    pub currency: String,
    pub cash: String,
    pub fee_percent: String,
    pub held_quantity: Option<String>,
    pub held_value: Option<String>,
    pub held_average_cost: Option<String>,
    pub observer_mode: bool,
    /// A sentence explaining what this kind of asset is, for someone learning.
    pub primer: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketRow {
    pub symbol: String,
    pub name: String,
    pub kind: String,
    pub price: Option<String>,
    pub price_raw: Option<f64>,
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

    let cards: Vec<PositionCard> = snapshot
        .positions
        .iter()
        .map(|p| position(p, currency))
        .collect();

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

        cash_percent: to_f64(
            safe_invest_core::money::percent(snapshot.cash, snapshot.total_value)
                .unwrap_or(Decimal::ZERO),
        ),

        allocation: allocation(snapshot, currency),
        best_position: best_position(snapshot),
        positions: cards,
        goal: report.goal.as_ref().map(|g| goal(g, currency)),
        value_history: session
            .value_history
            .iter()
            .map(|point| to_f64(point.total_value))
            .collect(),
        value_history_label: history_span(&session.value_history),

        sources,
        contains_simulated_prices: snapshot.contains_simulated_prices,
        unpriced_symbols: snapshot.unpriced_symbols.clone(),
        updated_at: datetime(snapshot.as_of),
    }
}

/// Groups the portfolio into the bands of the allocation bar.
///
/// Cash is a band like any other. Someone holding nine tenths of their money in
/// cash has a portfolio that is nine tenths cash, and a chart that quietly
/// leaves it out tells them the opposite of what they need to know.
fn allocation(
    snapshot: &safe_invest_core::model::PortfolioSnapshot,
    currency: &str,
) -> Vec<AllocationSlice> {
    use safe_invest_core::money;

    let mut slices: Vec<AllocationSlice> = Vec::new();

    for kind in [AssetKind::Crypto, AssetKind::Stock, AssetKind::Etf] {
        let (value, percent) = snapshot
            .positions
            .iter()
            .filter(|position| position.asset.kind == kind)
            .fold(
                (Decimal::ZERO, Decimal::ZERO),
                |(value, percent), position| {
                    (
                        money::add(value, position.market_value.unwrap_or(Decimal::ZERO))
                            .unwrap_or(value),
                        money::add(percent, position.weight_percent).unwrap_or(percent),
                    )
                },
            );

        if value.is_zero() {
            continue;
        }

        slices.push(AllocationSlice {
            kind: kind.as_str().to_owned(),
            label: kind_plural(kind).to_owned(),
            percent: to_f64(percent),
            value: money(value, currency),
        });
    }

    if !snapshot.cash.is_zero() {
        slices.push(AllocationSlice {
            kind: "cash".to_owned(),
            label: "Liquidités".to_owned(),
            percent: to_f64(
                money::percent(snapshot.cash, snapshot.total_value).unwrap_or(Decimal::ZERO),
            ),
            value: money(snapshot.cash, currency),
        });
    }

    slices.sort_by(|a, b| b.percent.total_cmp(&a.percent));
    slices
}

/// The line that has gained the most, if any line has gained at all.
fn best_position(snapshot: &safe_invest_core::model::PortfolioSnapshot) -> Option<BestPosition> {
    let (position, percent) = snapshot
        .positions
        .iter()
        .filter_map(|position| {
            position
                .unrealized_pnl_percent
                .map(|percent| (position, percent))
        })
        .max_by(|left, right| left.1.cmp(&right.1))?;

    Some(BestPosition {
        symbol: position.asset.symbol.clone(),
        name: position.asset.name.clone(),
        pnl_percent: signed_percent(percent),
        direction: direction_of(Some(percent)),
    })
}

pub fn position(view: &PositionView, currency: &str) -> PositionCard {
    PositionCard {
        symbol: view.asset.symbol.clone(),
        name: view.asset.name.clone(),
        kind: view.asset.kind.as_str().to_owned(),
        quantity: quantity(view.quantity),
        average_cost: money(view.average_cost, currency),
        price: view.price.map(|p| money(p, currency)),
        price_raw: view.price.map(to_f64),
        quantity_raw: to_f64(view.quantity),
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
        kind: trade.asset.kind.as_str().to_owned(),
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

pub fn asset_view(report: &crate::ops::AssetReport) -> AssetView {
    use rust_decimal::prelude::ToPrimitive;

    let currency = &report.currency;
    let quote = report.quote.as_ref();
    let history: Vec<f64> = report
        .history
        .iter()
        .filter_map(|p| p.price.to_f64())
        .collect();

    // The move across the whole window, computed from the ends of the curve
    // that is actually drawn — so the figure and the shape always agree.
    let period = match (history.first(), history.last()) {
        (Some(first), Some(last)) if *first > 0.0 => Some((last - first) / first * 100.0),
        _ => None,
    };

    AssetView {
        symbol: report.asset.symbol.clone(),
        name: report.asset.name.clone(),
        kind: report.asset.kind.as_str().to_owned(),
        kind_label: kind_label(report.asset.kind).to_owned(),
        price: quote.map(|q| money(q.price, currency)),
        price_raw: quote.and_then(|q| q.price.to_f64()),
        change_percent_24h: quote.and_then(|q| q.change_percent_24h).map(signed_percent),
        direction: quote.map_or(0, Quote::direction),
        source_id: quote.map(|q| q.source_id.clone()),
        is_simulated: quote.is_some_and(|q| q.is_simulated),
        quoted_at: quote.map(|q| datetime(q.as_of)),
        history,
        history_days: report.history_days,
        period_change: period.map(|p| {
            let sign = if p > 0.0 { "+" } else { "" };
            format!("{sign}{p:.2} %").replace('.', ",")
        }),
        period_direction: match period {
            Some(p) if p > 0.0 => 1,
            Some(p) if p < 0.0 => -1,
            _ => 0,
        },
        currency: currency.clone(),
        cash: money(report.cash, currency),
        fee_percent: format!("{} %", report.fee_percent.normalize()),
        held_quantity: report.position.as_ref().map(|h| quantity(h.quantity)),
        held_value: report.position.as_ref().and_then(|h| {
            quote.map(|q| money(crate::view::held_value(h.quantity, q.price), currency))
        }),
        held_average_cost: report
            .position
            .as_ref()
            .map(|h| money(h.average_cost, currency)),
        observer_mode: report.observer_mode,
        primer: primer_for(report.asset.kind).to_owned(),
    }
}

fn held_value(quantity: Decimal, price: Decimal) -> Decimal {
    quantity.checked_mul(price).unwrap_or(Decimal::ZERO)
}

fn kind_label(kind: AssetKind) -> &'static str {
    match kind {
        AssetKind::Crypto => "Cryptomonnaie",
        AssetKind::Stock => "Action",
        AssetKind::Etf => "ETF",
    }
}

/// The same word in the plural, for the allocation bar's legend.
fn kind_plural(kind: AssetKind) -> &'static str {
    match kind {
        AssetKind::Crypto => "Cryptos",
        AssetKind::Stock => "Actions",
        AssetKind::Etf => "ETF",
    }
}

/// One sentence on what this kind of asset actually is.
///
/// The point of the whole program is that someone leaves knowing more than they
/// arrived with, and the moment they are about to buy something is the moment
/// they are most likely to read it.
fn primer_for(kind: safe_invest_core::model::AssetKind) -> &'static str {
    use safe_invest_core::model::AssetKind;
    match kind {
        AssetKind::Crypto => {
            "Une cryptomonnaie n'a ni chiffre d'affaires ni bénéfices : son cours ne tient \
             qu'à ce que d'autres acceptent de payer. C'est ce qui la rend très volatile."
        }
        AssetKind::Stock => {
            "Une action est une part d'entreprise. Sa valeur suit les résultats de \
             l'entreprise, mais aussi ce que le marché anticipe de son avenir."
        }
        AssetKind::Etf => {
            "Un ETF contient des centaines d'entreprises d'un coup. Une seule ligne suffit \
             donc à être diversifié — c'est le placement le plus simple à comprendre."
        }
    }
}

pub fn market_row(asset: &Asset, quote: Option<&Quote>, currency: &str) -> MarketRow {
    MarketRow {
        symbol: asset.symbol.clone(),
        name: asset.name.clone(),
        kind: asset.kind.as_str().to_owned(),
        price: quote.map(|q| money(q.price, currency)),
        price_raw: quote.map(|q| to_f64(q.price)),
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

/// How far back the curve reaches, said the way a person would say it.
///
/// "sur les 121 derniers relevés" is a number about the program; "sur les
/// 30 derniers jours" is a number about the portfolio.
fn history_span(points: &[safe_invest_core::model::ValuePoint]) -> String {
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return String::new();
    };

    let seconds = last.at.as_second().saturating_sub(first.at.as_second());
    let hours = seconds / 3_600;
    let days = seconds / 86_400;

    match (days, hours) {
        (0, 0) => "depuis le début de la partie".to_owned(),
        (0, 1) => "sur la dernière heure".to_owned(),
        (0, hours) => format!("sur les {hours} dernières heures"),
        (1, _) => "sur les dernières 24 heures".to_owned(),
        (days, _) => format!("sur les {days} derniers jours"),
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
    fn the_curve_span_is_said_the_way_a_person_would_say_it() {
        use safe_invest_core::model::ValuePoint;

        let at = |seconds: i64| ValuePoint {
            at: Timestamp::from_second(seconds).unwrap(),
            total_value: Decimal::ONE,
        };

        assert_eq!(history_span(&[]), "");
        assert_eq!(history_span(&[at(0)]), "depuis le début de la partie");
        assert_eq!(
            history_span(&[at(0), at(3 * 3600)]),
            "sur les 3 dernières heures"
        );
        assert_eq!(
            history_span(&[at(0), at(86_400)]),
            "sur les dernières 24 heures"
        );
        assert_eq!(
            history_span(&[at(0), at(30 * 86_400)]),
            "sur les 30 derniers jours"
        );
    }

    #[test]
    fn direction_is_zero_when_there_is_nothing_to_compare() {
        assert_eq!(direction_of(None), 0);
        assert_eq!(direction_of(Some(Decimal::ZERO)), 0);
        assert_eq!(direction_of(Some(d("0.01"))), 1);
        assert_eq!(direction_of(Some(d("-0.01"))), -1);
    }
}
