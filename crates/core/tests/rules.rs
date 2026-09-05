//! The rules of the game, pinned down.
//!
//! Every test here describes something a player could notice going wrong: money
//! appearing out of nowhere, a portfolio that cannot be sold in full, an AI that
//! trades without saying why.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a test that trips is a test that failed"
)]

use jiff::Timestamp;
use rust_decimal::Decimal;
use safe_invest_core::engine::{self, TradeAmount, TradeError};
use safe_invest_core::factory::{self, NewGame, NewGameError};
use safe_invest_core::model::{
    Asset, AssetKind, GameSession, Goal, GoalStatus, PlayerKind, Quote, TradeSide,
};
use safe_invest_core::{goal, valuation};
use std::collections::HashMap;
use std::str::FromStr;

fn d(text: &str) -> Decimal {
    Decimal::from_str(text).unwrap()
}

fn at(text: &str) -> Timestamp {
    text.parse().unwrap()
}

fn now() -> Timestamp {
    at("2026-01-01T12:00:00Z")
}

fn btc() -> Asset {
    Asset::new("btc", "Bitcoin", AssetKind::Crypto)
}

fn quote(asset: &Asset, price: &str) -> Quote {
    Quote {
        symbol: asset.symbol.clone(),
        kind: asset.kind,
        price: d(price),
        currency: "EUR".into(),
        as_of: now(),
        source_id: "test".into(),
        is_simulated: false,
        name: Some(asset.name.clone()),
        change_percent_24h: Some(d("1.5")),
        market_cap: None,
        volume_24h: None,
    }
}

fn game(kind: PlayerKind, cash: &str, fee_percent: &str) -> GameSession {
    let mut session = factory::create(
        NewGame {
            player_name: "Testeur".into(),
            player_kind: kind,
            currency: "EUR".into(),
            starting_cash: d(cash),
            fee_percent: d(fee_percent),
            goal: None,
        },
        now(),
    )
    .unwrap();
    session.created_at = now();
    session
}

// ---------------------------------------------------------------- new game

#[test]
fn a_new_game_starts_with_all_its_cash_and_nothing_else() {
    let session = game(PlayerKind::Human, "10000", "0");
    assert_eq!(session.cash, d("10000"));
    assert_eq!(session.starting_cash, d("10000"));
    assert!(session.holdings.is_empty());
    assert!(session.trades.is_empty());
}

#[test]
fn a_game_cannot_start_on_nonsense() {
    let base = || NewGame {
        player_name: "Testeur".into(),
        player_kind: PlayerKind::Human,
        currency: "EUR".into(),
        starting_cash: d("1000"),
        fee_percent: Decimal::ZERO,
        goal: None,
    };

    let empty_name = NewGame {
        player_name: "   ".into(),
        ..base()
    };
    assert_eq!(
        factory::create(empty_name, now()),
        Err(NewGameError::PlayerName)
    );

    let bad_currency = NewGame {
        currency: "EUROS".into(),
        ..base()
    };
    assert_eq!(
        factory::create(bad_currency, now()),
        Err(NewGameError::Currency)
    );

    let no_money = NewGame {
        starting_cash: Decimal::ZERO,
        ..base()
    };
    assert_eq!(
        factory::create(no_money, now()),
        Err(NewGameError::StartingCash)
    );

    let absurd_fees = NewGame {
        fee_percent: d("20"),
        ..base()
    };
    assert_eq!(
        factory::create(absurd_fees, now()),
        Err(NewGameError::FeePercent)
    );
}

#[test]
fn a_goal_must_be_ahead_in_both_money_and_time() {
    let base = || NewGame {
        player_name: "Testeur".into(),
        player_kind: PlayerKind::Ai,
        currency: "EUR".into(),
        starting_cash: d("1000"),
        fee_percent: Decimal::ZERO,
        goal: None,
    };

    let already_there = NewGame {
        goal: Some(Goal {
            target_amount: d("900"),
            deadline: at("2027-01-01T00:00:00Z"),
        }),
        ..base()
    };
    assert_eq!(
        factory::create(already_there, now()),
        Err(NewGameError::GoalTooLow)
    );

    let in_the_past = NewGame {
        goal: Some(Goal {
            target_amount: d("5000"),
            deadline: at("2020-01-01T00:00:00Z"),
        }),
        ..base()
    };
    assert_eq!(
        factory::create(in_the_past, now()),
        Err(NewGameError::GoalDeadline)
    );
}

// ------------------------------------------------------------------- buying

#[test]
fn buying_by_quantity_moves_exactly_the_expected_cash() {
    let mut session = game(PlayerKind::Human, "10000", "0");
    let asset = btc();

    let trade = engine::buy(
        &mut session,
        &asset,
        &quote(&asset, "50000"),
        TradeAmount::Units(d("0.1")),
        None,
        now(),
    )
    .unwrap();

    assert_eq!(trade.side, TradeSide::Buy);
    assert_eq!(trade.total, d("5000.00"));
    assert_eq!(session.cash, d("5000.00"));
    assert_eq!(session.holdings.len(), 1);
    assert_eq!(session.holdings[0].quantity, d("0.1"));
    assert_eq!(session.holdings[0].average_cost, d("50000"));
}

#[test]
fn buying_by_amount_never_overspends_even_with_fees() {
    // The classic off-by-a-cent: fees must come out of the budget, not on top.
    let mut session = game(PlayerKind::Human, "1000", "1");
    let asset = btc();

    let trade = engine::buy(
        &mut session,
        &asset,
        &quote(&asset, "37777.77"),
        TradeAmount::Cash(d("1000")),
        None,
        now(),
    )
    .unwrap();

    assert!(
        trade.total <= d("1000"),
        "dépensé {} pour un budget de 1000",
        trade.total
    );
    assert!(session.cash >= Decimal::ZERO);
    assert!(trade.fees > Decimal::ZERO);
}

#[test]
fn buying_more_than_the_cash_allows_is_refused() {
    let mut session = game(PlayerKind::Human, "100", "0");
    let asset = btc();

    let error = engine::buy(
        &mut session,
        &asset,
        &quote(&asset, "50000"),
        TradeAmount::Units(d("1")),
        None,
        now(),
    )
    .unwrap_err();

    assert!(matches!(error, TradeError::Rejected(_)));
    assert_eq!(session.cash, d("100"), "un refus ne doit rien débiter");
    assert!(session.holdings.is_empty());
    assert!(session.trades.is_empty());
}

#[test]
fn two_buys_average_their_cost() {
    let mut session = game(PlayerKind::Human, "10000", "0");
    let asset = btc();

    engine::buy(
        &mut session,
        &asset,
        &quote(&asset, "100"),
        TradeAmount::Units(d("10")),
        None,
        now(),
    )
    .unwrap();
    engine::buy(
        &mut session,
        &asset,
        &quote(&asset, "200"),
        TradeAmount::Units(d("10")),
        None,
        now(),
    )
    .unwrap();

    assert_eq!(session.holdings[0].quantity, d("20"));
    assert_eq!(session.holdings[0].average_cost, d("150"));
}

// ------------------------------------------------------------------ selling

#[test]
fn selling_everything_closes_the_position_completely() {
    let mut session = game(PlayerKind::Human, "10000", "0");
    let asset = btc();
    engine::buy(
        &mut session,
        &asset,
        &quote(&asset, "3333.33"),
        TradeAmount::Cash(d("1000")),
        None,
        now(),
    )
    .unwrap();

    engine::sell(
        &mut session,
        &asset,
        &quote(&asset, "3333.33"),
        TradeAmount::All,
        None,
        now(),
    )
    .unwrap();

    assert!(
        session.holdings.is_empty(),
        "il reste de la poussière : {:?}",
        session.holdings
    );
}

#[test]
fn selling_more_than_is_held_is_refused() {
    let mut session = game(PlayerKind::Human, "10000", "0");
    let asset = btc();
    engine::buy(
        &mut session,
        &asset,
        &quote(&asset, "100"),
        TradeAmount::Units(d("1")),
        None,
        now(),
    )
    .unwrap();

    let error = engine::sell(
        &mut session,
        &asset,
        &quote(&asset, "100"),
        TradeAmount::Units(d("2")),
        None,
        now(),
    )
    .unwrap_err();

    assert!(matches!(error, TradeError::Rejected(_)));
    assert_eq!(session.holdings[0].quantity, d("1"));
}

#[test]
fn selling_something_never_bought_is_refused() {
    let mut session = game(PlayerKind::Human, "10000", "0");
    let asset = btc();

    let error = engine::sell(
        &mut session,
        &asset,
        &quote(&asset, "100"),
        TradeAmount::All,
        None,
        now(),
    )
    .unwrap_err();

    assert!(matches!(error, TradeError::Rejected(_)));
}

#[test]
fn a_profitable_round_trip_books_the_gain_minus_fees() {
    let mut session = game(PlayerKind::Human, "10000", "1");
    let asset = btc();

    engine::buy(
        &mut session,
        &asset,
        &quote(&asset, "100"),
        TradeAmount::Units(d("10")),
        None,
        now(),
    )
    .unwrap();
    let sale = engine::sell(
        &mut session,
        &asset,
        &quote(&asset, "150"),
        TradeAmount::All,
        None,
        now(),
    )
    .unwrap();

    // Gain 50 × 10 = 500, minus the 15 € fee on a 1 500 € sale.
    assert_eq!(sale.realized_pnl, Some(d("485.00")));
    assert_eq!(session.realized_pnl(), d("485.00"));
}

#[test]
fn a_round_trip_at_a_flat_price_with_no_fees_returns_the_exact_starting_cash() {
    // Money must not be created or destroyed by rounding.
    let mut session = game(PlayerKind::Human, "10000", "0");
    let asset = btc();

    engine::buy(
        &mut session,
        &asset,
        &quote(&asset, "137.77"),
        TradeAmount::Cash(d("5000")),
        None,
        now(),
    )
    .unwrap();
    engine::sell(
        &mut session,
        &asset,
        &quote(&asset, "137.77"),
        TradeAmount::All,
        None,
        now(),
    )
    .unwrap();

    let lost = d("10000") - session.cash;
    assert!(
        lost.abs() <= d("0.01"),
        "un aller-retour à cours constant a coûté {lost}"
    );
}

// ------------------------------------------------------------------ AI rules

#[test]
fn an_ai_cannot_trade_without_saying_why() {
    let mut session = game(PlayerKind::Ai, "10000", "0");
    let asset = btc();

    let error = engine::buy(
        &mut session,
        &asset,
        &quote(&asset, "100"),
        TradeAmount::Units(d("1")),
        None,
        now(),
    )
    .unwrap_err();
    assert!(matches!(error, TradeError::Rejected(_)));

    let blank = engine::buy(
        &mut session,
        &asset,
        &quote(&asset, "100"),
        TradeAmount::Units(d("1")),
        Some("   "),
        now(),
    )
    .unwrap_err();
    assert!(
        matches!(blank, TradeError::Rejected(_)),
        "un commentaire vide ne compte pas"
    );

    assert!(session.trades.is_empty());
}

#[test]
fn an_ai_trade_keeps_its_justification_in_the_history() {
    let mut session = game(PlayerKind::Ai, "10000", "0");
    let asset = btc();

    engine::buy(
        &mut session,
        &asset,
        &quote(&asset, "100"),
        TradeAmount::Units(d("1")),
        Some("  Diversification vers la crypto  "),
        now(),
    )
    .unwrap();

    assert_eq!(
        session.trades[0].rationale.as_deref(),
        Some("Diversification vers la crypto"),
        "la justification doit être conservée, sans espaces parasites"
    );
    assert_eq!(session.trades[0].actor_kind, PlayerKind::Ai);
}

#[test]
fn a_human_may_trade_in_silence() {
    let mut session = game(PlayerKind::Human, "10000", "0");
    let asset = btc();
    engine::buy(
        &mut session,
        &asset,
        &quote(&asset, "100"),
        TradeAmount::Units(d("1")),
        None,
        now(),
    )
    .unwrap();
    assert_eq!(session.trades.len(), 1);
}

// ------------------------------------------------------------ quote sanity

#[test]
fn a_quote_in_the_wrong_currency_is_refused() {
    let mut session = game(PlayerKind::Human, "10000", "0");
    let asset = btc();
    let mut usd = quote(&asset, "100");
    usd.currency = "USD".into();

    let error = engine::buy(
        &mut session,
        &asset,
        &usd,
        TradeAmount::Units(d("1")),
        None,
        now(),
    )
    .unwrap_err();
    assert!(matches!(error, TradeError::Rejected(_)));
}

#[test]
fn a_quote_for_another_asset_is_refused() {
    let mut session = game(PlayerKind::Human, "10000", "0");
    let asset = btc();
    let eth = Asset::new("eth", "Ethereum", AssetKind::Crypto);

    let error = engine::buy(
        &mut session,
        &asset,
        &quote(&eth, "100"),
        TradeAmount::Units(d("1")),
        None,
        now(),
    )
    .unwrap_err();
    assert!(matches!(error, TradeError::Rejected(_)));
}

#[test]
fn a_zero_or_negative_price_is_refused() {
    let mut session = game(PlayerKind::Human, "10000", "0");
    let asset = btc();

    for price in ["0", "-10"] {
        let error = engine::buy(
            &mut session,
            &asset,
            &quote(&asset, price),
            TradeAmount::Units(d("1")),
            None,
            now(),
        )
        .unwrap_err();
        assert!(
            matches!(error, TradeError::Rejected(_)),
            "cours {price} accepté à tort"
        );
    }
}

// -------------------------------------------------------------- valuation

#[test]
fn an_unpriced_holding_is_reported_not_valued_at_zero() {
    let mut session = game(PlayerKind::Human, "10000", "0");
    let asset = btc();
    engine::buy(
        &mut session,
        &asset,
        &quote(&asset, "100"),
        TradeAmount::Units(d("10")),
        None,
        now(),
    )
    .unwrap();

    let snapshot = valuation::snapshot(&session, &HashMap::new(), now());

    assert_eq!(snapshot.unpriced_symbols, vec!["BTC".to_owned()]);
    assert_eq!(snapshot.market_value, Decimal::ZERO);
    assert!(snapshot.positions[0].price.is_none());
    assert_eq!(
        snapshot.total_value, session.cash,
        "un actif non coté ne vaut pas zéro, il ne vaut rien de connu"
    );
}

#[test]
fn a_simulated_price_is_flagged_all_the_way_up_to_the_snapshot() {
    let mut session = game(PlayerKind::Human, "10000", "0");
    let asset = btc();
    let mut fake = quote(&asset, "100");
    fake.is_simulated = true;
    fake.source_id = "simulated".into();

    engine::buy(
        &mut session,
        &asset,
        &fake,
        TradeAmount::Units(d("10")),
        None,
        now(),
    )
    .unwrap();

    let quotes = HashMap::from([(asset.key(), fake)]);
    let snapshot = valuation::snapshot(&session, &quotes, now());

    assert!(snapshot.contains_simulated_prices);
    assert!(snapshot.positions[0].is_simulated);
    assert!(session.trades[0].quote_was_simulated);
}

#[test]
fn weights_add_up_to_the_invested_share() {
    let mut session = game(PlayerKind::Human, "1000", "0");
    let btc_asset = btc();
    let eth_asset = Asset::new("eth", "Ethereum", AssetKind::Crypto);

    engine::buy(
        &mut session,
        &btc_asset,
        &quote(&btc_asset, "100"),
        TradeAmount::Units(d("3")),
        None,
        now(),
    )
    .unwrap();
    engine::buy(
        &mut session,
        &eth_asset,
        &quote(&eth_asset, "100"),
        TradeAmount::Units(d("2")),
        None,
        now(),
    )
    .unwrap();

    let quotes = HashMap::from([
        (btc_asset.key(), quote(&btc_asset, "100")),
        (eth_asset.key(), quote(&eth_asset, "100")),
    ]);
    let snapshot = valuation::snapshot(&session, &quotes, now());

    assert_eq!(snapshot.total_value, d("1000.00"));
    assert_eq!(snapshot.positions[0].weight_percent, d("30"));
    assert_eq!(snapshot.positions[1].weight_percent, d("20"));
}

// ------------------------------------------------------------------- goals

#[test]
fn no_goal_means_no_progress_to_report() {
    let session = game(PlayerKind::Human, "1000", "0");
    let snapshot = valuation::snapshot(&session, &HashMap::new(), now());
    assert!(goal::evaluate(&session, &snapshot, now()).is_none());
}

#[test]
fn progress_is_measured_from_the_starting_cash_not_from_zero() {
    let mut session = game(PlayerKind::Ai, "1000", "0");
    session.goal = Some(Goal {
        target_amount: d("2000"),
        deadline: at("2027-01-01T00:00:00Z"),
    });
    session.cash = d("1500");

    let snapshot = valuation::snapshot(&session, &HashMap::new(), now());
    let progress = goal::evaluate(&session, &snapshot, now()).unwrap();

    // Halfway from 1000 to 2000 is 50 %, not 75 %.
    assert_eq!(progress.progress_percent, d("50"));
    assert_eq!(progress.amount_remaining, d("500.00"));
}

#[test]
fn reaching_the_target_is_reported_as_achieved_even_before_the_deadline() {
    let mut session = game(PlayerKind::Ai, "1000", "0");
    session.goal = Some(Goal {
        target_amount: d("2000"),
        deadline: at("2027-01-01T00:00:00Z"),
    });
    session.cash = d("2500");

    let snapshot = valuation::snapshot(&session, &HashMap::new(), now());
    let progress = goal::evaluate(&session, &snapshot, now()).unwrap();

    assert_eq!(progress.status, GoalStatus::Achieved);
    assert_eq!(progress.progress_percent, d("100"));
    assert_eq!(progress.amount_remaining, Decimal::ZERO);
}

#[test]
fn a_missed_deadline_is_expired_not_merely_behind() {
    let mut session = game(PlayerKind::Ai, "1000", "0");
    session.goal = Some(Goal {
        target_amount: d("2000"),
        deadline: at("2025-01-01T00:00:00Z"),
    });

    let snapshot = valuation::snapshot(&session, &HashMap::new(), now());
    let progress = goal::evaluate(&session, &snapshot, now()).unwrap();

    assert_eq!(progress.status, GoalStatus::Expired);
    assert_eq!(progress.days_remaining, 0);
}

#[test]
fn a_fresh_game_is_never_told_it_is_already_behind() {
    let mut session = game(PlayerKind::Ai, "1000", "0");
    session.goal = Some(Goal {
        target_amount: d("2000"),
        deadline: at("2027-01-01T00:00:00Z"),
    });

    let snapshot = valuation::snapshot(&session, &HashMap::new(), now());
    let progress = goal::evaluate(&session, &snapshot, now()).unwrap();

    assert_eq!(
        progress.status,
        GoalStatus::OnTrack,
        "juger une tendance le premier jour n'a pas de sens"
    );
}

#[test]
fn annualised_return_is_capped_rather_than_absurd() {
    // Doubling in an hour is a real result the maths turns into ~1e100 %/an.
    let rate = goal::annualised(d("1000"), d("2000"), 0.0002);
    assert!(
        rate.is_none(),
        "moins d'un jour ne doit pas produire de taux"
    );

    let over_a_year = goal::annualised(d("1000"), d("1000000000"), 1.0).unwrap();
    assert_eq!(
        over_a_year,
        d("9999.00"),
        "le taux affiché doit rester lisible"
    );
}

#[test]
fn annualised_return_matches_a_hand_computed_case() {
    // 1 000 → 1 210 in two years is 10 %/an compounded.
    let rate = goal::annualised(d("1000"), d("1210"), 2.0).unwrap();
    assert_eq!(rate, d("10.00"));
}

// -------------------------------------------------------- value history

#[test]
fn the_curve_starts_where_the_game_starts() {
    let session = game(PlayerKind::Human, "10000", "0");
    assert_eq!(
        session.value_history.len(),
        1,
        "le premier jour ne doit pas être une page blanche"
    );
    assert_eq!(session.value_history[0].total_value, d("10000"));
}

#[test]
fn readings_are_kept_at_most_once_a_quarter_hour() {
    let mut session = game(PlayerKind::Human, "10000", "0");
    let start = now();

    // Offered a minute later: too soon, nothing kept.
    assert!(!session.record_value(start + jiff::Span::new().minutes(1), d("10500")));
    assert_eq!(session.value_history.len(), 1);

    // Offered a quarter of an hour later: kept.
    assert!(session.record_value(start + jiff::Span::new().minutes(15), d("10500")));
    assert_eq!(session.value_history.len(), 2);
    assert_eq!(session.value_history[1].total_value, d("10500"));
}

#[test]
fn the_history_does_not_grow_without_bound() {
    let mut session = game(PlayerKind::Human, "10000", "0");
    let start = now();

    // Far more readings than the cap, each well past the interval.
    for step in 1..=(GameSession::MAX_VALUE_POINTS + 50) {
        let at = start + jiff::Span::new().minutes(i64::try_from(step).unwrap() * 20);
        assert!(session.record_value(at, d("10000")));
    }

    assert_eq!(session.value_history.len(), GameSession::MAX_VALUE_POINTS);
    // The oldest go, not the newest: a curve must end at the present.
    let last = session.value_history.last().unwrap().at;
    assert!(last > start, "le relevé le plus récent doit être conservé");
}

#[test]
fn a_reading_offered_out_of_order_is_refused_rather_than_scrambling_the_curve() {
    let mut session = game(PlayerKind::Human, "10000", "0");
    let start = now();
    session.record_value(start + jiff::Span::new().hours(1), d("11000"));

    assert!(
        !session.record_value(start + jiff::Span::new().minutes(30), d("9000")),
        "un relevé antérieur au dernier ne doit pas être inséré"
    );
    assert!(
        session.value_history.windows(2).all(|w| w[0].at <= w[1].at),
        "la courbe doit rester ordonnée dans le temps"
    );
}
