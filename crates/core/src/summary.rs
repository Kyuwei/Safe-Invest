//! What a finished game amounted to.
//!
//! Every figure here is read back out of the trade log and the outcome that was
//! written when the game stopped. Nothing is re-valued at today's prices: the
//! point of a summary is to say what happened, and a number that drifts every
//! time the page is opened is not a record of anything.

use crate::model::{AssetKind, EndReason, GameSession, TradeSide};
use crate::money;
use jiff::Timestamp;
use rust_decimal::Decimal;
use serde::Serialize;

/// One trade worth naming, best or worst.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeResult {
    pub symbol: String,
    pub name: String,
    pub realized_pnl: Decimal,
}

/// Something the player did, worth saying out loud.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Badge {
    pub id: &'static str,
    pub label: &'static str,
    /// Why it was or was not earned, in one line.
    pub note: &'static str,
    pub earned: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub reason: EndReason,
    pub started_at: Timestamp,
    pub ended_at: Timestamp,
    pub days: i64,

    pub starting_cash: Decimal,
    pub final_value: Decimal,
    pub profit: Decimal,
    pub profit_percent: Decimal,

    pub trade_count: usize,
    pub buy_count: usize,
    pub sell_count: usize,
    pub volume: Decimal,

    pub best: Option<TradeResult>,
    pub worst: Option<TradeResult>,

    /// How many sales made money, out of how many sales.
    ///
    /// Only *closed* positions count. A purchase has no result yet, and folding
    /// buys into the denominator would halve the figure for reasons that have
    /// nothing to do with how well anyone chose.
    pub closed_count: usize,
    pub winning_count: usize,
    pub win_rate_percent: Option<Decimal>,

    pub badges: Vec<Badge>,
}

/// Builds the summary of a finished game, or `None` if it is still running.
pub fn of(session: &GameSession) -> Option<Summary> {
    let outcome = session.outcome?;

    let profit = money::sub(outcome.final_value, session.starting_cash)
        .map(money::round_money)
        .unwrap_or(Decimal::ZERO);
    let profit_percent = money::percent(profit, session.starting_cash).unwrap_or(Decimal::ZERO);

    let seconds = outcome
        .ended_at
        .as_second()
        .saturating_sub(session.created_at.as_second());

    let buy_count = session
        .trades
        .iter()
        .filter(|trade| trade.side == TradeSide::Buy)
        .count();

    let volume = session.trades.iter().fold(Decimal::ZERO, |sum, trade| {
        money::add(sum, trade.total).unwrap_or(sum)
    });

    let closed: Vec<(&crate::model::Trade, Decimal)> = session
        .trades
        .iter()
        .filter_map(|trade| trade.realized_pnl.map(|pnl| (trade, pnl)))
        .collect();

    let winning_count = closed
        .iter()
        .filter(|(_, pnl)| *pnl > Decimal::ZERO)
        .count();

    let win_rate_percent = (!closed.is_empty()).then(|| {
        money::percent(
            Decimal::from(winning_count as u64),
            Decimal::from(closed.len() as u64),
        )
        .unwrap_or(Decimal::ZERO)
    });

    let named = |(trade, pnl): &(&crate::model::Trade, Decimal)| TradeResult {
        symbol: trade.asset.symbol.clone(),
        name: trade.asset.name.clone(),
        realized_pnl: *pnl,
    };

    let best = closed
        .iter()
        .max_by(|left, right| left.1.cmp(&right.1))
        .filter(|(_, pnl)| *pnl > Decimal::ZERO)
        .map(named);

    let worst = closed
        .iter()
        .min_by(|left, right| left.1.cmp(&right.1))
        .filter(|(_, pnl)| *pnl < Decimal::ZERO)
        .map(named);

    Some(Summary {
        reason: outcome.reason,
        started_at: session.created_at,
        ended_at: outcome.ended_at,
        days: seconds / 86_400,

        starting_cash: session.starting_cash,
        final_value: outcome.final_value,
        profit,
        profit_percent,

        trade_count: session.trades.len(),
        buy_count,
        sell_count: session.trades.len().saturating_sub(buy_count),
        volume,

        best,
        worst,

        closed_count: closed.len(),
        winning_count,
        win_rate_percent,

        badges: badges(session, &outcome, profit, seconds),
    })
}

/// The badges, each one a fact about the game rather than a flourish.
fn badges(
    session: &GameSession,
    outcome: &crate::model::Outcome,
    profit: Decimal,
    seconds: i64,
) -> Vec<Badge> {
    let touched = |kind: AssetKind| session.trades.iter().any(|trade| trade.asset.kind == kind);

    vec![
        Badge {
            id: "goal",
            label: "Objectif tenu",
            note: "Le montant visé a été atteint avant la date limite.",
            earned: outcome.reason == EndReason::GoalReached,
        },
        Badge {
            id: "profit",
            label: "Dans le vert",
            note: "La partie se termine au-dessus du capital de départ.",
            earned: profit > Decimal::ZERO,
        },
        Badge {
            id: "classes",
            label: "Les trois classes",
            note: "Une crypto, une action et un ETF ont été achetés au moins une fois.",
            earned: touched(AssetKind::Crypto)
                && touched(AssetKind::Stock)
                && touched(AssetKind::Etf),
        },
        Badge {
            id: "patience",
            label: "Un mois de patience",
            note: "La partie a duré au moins trente jours.",
            earned: seconds >= 30 * 86_400,
        },
    ]
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "a test that trips is a test that failed"
)]
mod tests {
    use super::*;
    use crate::model::{Asset, PlayerKind, Trade};
    use std::str::FromStr;
    use uuid::Uuid;

    fn d(text: &str) -> Decimal {
        Decimal::from_str(text).unwrap()
    }

    fn at(text: &str) -> Timestamp {
        text.parse().unwrap()
    }

    fn session() -> GameSession {
        crate::factory::create(
            crate::factory::NewGame {
                player_name: "Testeur".into(),
                player_kind: PlayerKind::Human,
                currency: "EUR".into(),
                starting_cash: d("10000"),
                fee_percent: Decimal::ZERO,
                goal: None,
            },
            at("2026-01-01T00:00:00Z"),
        )
        .unwrap()
    }

    fn sale(symbol: &str, kind: AssetKind, total: &str, realized: &str) -> Trade {
        Trade {
            id: Uuid::new_v4(),
            timestamp: at("2026-01-05T00:00:00Z"),
            side: TradeSide::Sell,
            asset: Asset::new(symbol, symbol, kind),
            quantity: Decimal::ONE,
            unit_price: d(total),
            fees: Decimal::ZERO,
            total: d(total),
            realized_pnl: Some(d(realized)),
            rationale: None,
            actor_kind: PlayerKind::Human,
            quote_source_id: None,
            quote_was_simulated: false,
        }
    }

    fn purchase(symbol: &str, kind: AssetKind, total: &str) -> Trade {
        Trade {
            side: TradeSide::Buy,
            realized_pnl: None,
            ..sale(symbol, kind, total, "0")
        }
    }

    #[test]
    fn a_running_game_has_no_summary() {
        assert!(of(&session()).is_none());
    }

    #[test]
    fn the_result_is_measured_against_the_starting_cash() {
        let mut game = session();
        game.finish(EndReason::Stopped, d("12500"), at("2026-02-01T00:00:00Z"));

        let summary = of(&game).unwrap();
        assert_eq!(summary.final_value, d("12500"));
        assert_eq!(summary.profit, d("2500"));
        assert_eq!(summary.profit_percent, d("25"));
        assert_eq!(summary.days, 31);
    }

    /// A purchase has not made or lost anything yet. Counting it as a loss —
    /// which is what putting it in the denominator does — would report 33 %
    /// for a player whose every sale was a winner.
    #[test]
    fn the_win_rate_counts_sales_and_not_purchases() {
        let mut game = session();
        game.trades = vec![
            purchase("BTC", AssetKind::Crypto, "1000"),
            purchase("AAPL", AssetKind::Stock, "1000"),
            sale("BTC", AssetKind::Crypto, "1200", "200"),
        ];
        game.finish(EndReason::Stopped, d("10200"), at("2026-01-10T00:00:00Z"));

        let summary = of(&game).unwrap();
        assert_eq!(summary.trade_count, 3);
        assert_eq!(summary.buy_count, 2);
        assert_eq!(summary.sell_count, 1);
        assert_eq!(summary.closed_count, 1);
        assert_eq!(summary.winning_count, 1);
        assert_eq!(summary.win_rate_percent, Some(d("100")));
    }

    #[test]
    fn the_best_and_worst_are_only_named_when_they_exist() {
        let mut game = session();
        game.trades = vec![
            sale("BTC", AssetKind::Crypto, "1200", "200"),
            sale("AAPL", AssetKind::Stock, "800", "-150"),
            sale("CW8", AssetKind::Etf, "1000", "40"),
        ];
        game.finish(EndReason::Stopped, d("10090"), at("2026-01-10T00:00:00Z"));

        let summary = of(&game).unwrap();
        assert_eq!(summary.best.as_ref().unwrap().symbol, "BTC");
        assert_eq!(summary.worst.as_ref().unwrap().symbol, "AAPL");
    }

    /// A game where nothing lost money must not name a "worst trade": the least
    /// good winner is still a winner, and labelling it the worst is a lie.
    #[test]
    fn a_game_that_never_lost_has_no_worst_trade() {
        let mut game = session();
        game.trades = vec![
            sale("BTC", AssetKind::Crypto, "1200", "200"),
            sale("CW8", AssetKind::Etf, "1000", "40"),
        ];
        game.finish(EndReason::Stopped, d("10240"), at("2026-01-10T00:00:00Z"));

        let summary = of(&game).unwrap();
        assert_eq!(summary.best.as_ref().unwrap().symbol, "BTC");
        assert!(summary.worst.is_none());
    }

    #[test]
    fn badges_report_what_actually_happened() {
        let mut game = session();
        game.trades = vec![
            purchase("BTC", AssetKind::Crypto, "1000"),
            purchase("AAPL", AssetKind::Stock, "1000"),
            purchase("CW8", AssetKind::Etf, "1000"),
        ];
        game.finish(
            EndReason::GoalReached,
            d("15000"),
            at("2026-03-01T00:00:00Z"),
        );

        let summary = of(&game).unwrap();
        let earned = |id: &str| {
            summary
                .badges
                .iter()
                .find(|badge| badge.id == id)
                .unwrap()
                .earned
        };

        assert!(earned("goal"));
        assert!(earned("profit"));
        assert!(earned("classes"));
        assert!(earned("patience"));
    }

    #[test]
    fn badges_are_not_handed_out_for_a_game_that_did_none_of_it() {
        let mut game = session();
        game.trades = vec![purchase("BTC", AssetKind::Crypto, "1000")];
        game.finish(EndReason::Stopped, d("9000"), at("2026-01-03T00:00:00Z"));

        let summary = of(&game).unwrap();
        assert!(summary.badges.iter().all(|badge| !badge.earned));
    }
}
