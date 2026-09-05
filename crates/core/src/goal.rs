//! Scoring a game against "reach X by date Y".

use crate::model::{GameSession, GoalProgress, GoalStatus, PortfolioSnapshot};
use crate::money;
use jiff::Timestamp;
use rust_decimal::Decimal;
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};

const DAYS_PER_YEAR: f64 = 365.25;
const MILLIS_PER_DAY: f64 = 86_400_000.0;
const MILLIS_PER_DAY_U64: u64 = 86_400_000;

/// Under roughly a day, an annualised rate explodes into meaningless numbers.
const MIN_YEARS: f64 = 0.0027;

/// Displayed rates are capped: "+9 999 %/an" says "impossible" as well as
/// "+4e12 %" does, and it does not break the layout.
const MAX_RATE: f64 = 99.99;

/// Returns `None` when the session has no goal set.
pub fn evaluate(
    session: &GameSession,
    snapshot: &PortfolioSnapshot,
    now: Timestamp,
) -> Option<GoalProgress> {
    let goal = session.goal?;
    let current = snapshot.total_value;

    let remaining = money::sub(goal.target_amount, current)
        .map(money::round_money)
        .unwrap_or(Decimal::ZERO);

    // Progress is measured over the ground actually to be covered — from the
    // starting cash to the target, not from zero. Otherwise every game opens at
    // 80 % done and the ring means nothing.
    let span = money::sub(goal.target_amount, session.starting_cash).unwrap_or(Decimal::ZERO);
    let progress = if span <= Decimal::ZERO {
        Decimal::ONE_HUNDRED
    } else {
        money::sub(current, session.starting_cash)
            .and_then(|covered| money::percent(covered, span))
            .unwrap_or(Decimal::ZERO)
            .clamp(Decimal::ZERO, Decimal::ONE_HUNDRED)
    };

    let years_left = years_between(now, goal.deadline);
    let years_elapsed = years_between(session.created_at, now);
    let full_horizon = years_between(session.created_at, goal.deadline);

    let required = annualised(current, goal.target_amount, years_left);
    let achieved = annualised(session.starting_cash, current, years_elapsed);
    let required_from_start = annualised(session.starting_cash, goal.target_amount, full_horizon);

    let status = if current >= goal.target_amount {
        GoalStatus::Achieved
    } else if now > goal.deadline {
        GoalStatus::Expired
    } else {
        match (achieved, required_from_start) {
            // Too early to judge a trend — do not scare the player on day one.
            (None, _) | (_, None) => GoalStatus::OnTrack,
            (Some(a), Some(r)) if a >= r => GoalStatus::OnTrack,
            _ => GoalStatus::Behind,
        }
    };

    Some(GoalProgress {
        target_amount: goal.target_amount,
        deadline: goal.deadline,
        current_value: current,
        starting_cash: session.starting_cash,
        progress_percent: progress,
        amount_remaining: remaining.max(Decimal::ZERO),
        days_remaining: whole_days_until(now, goal.deadline),
        status,
        required_annualised_return_percent: required,
        achieved_annualised_return_percent: achieved,
    })
}

/// Compound annual growth rate, in percent.
///
/// `None` when the maths would be meaningless: no elapsed time, or a
/// non-positive starting value.
pub fn annualised(from: Decimal, to: Decimal, years: f64) -> Option<Decimal> {
    if from <= Decimal::ZERO || years <= MIN_YEARS {
        return None;
    }
    let ratio = money::div(to, from).ok()?.to_f64()?;
    if ratio <= 0.0 {
        return None;
    }

    let rate = ratio.powf(1.0 / years) - 1.0;
    if !rate.is_finite() {
        return None;
    }

    let capped = rate.clamp(-1.0, MAX_RATE) * 100.0;
    Decimal::from_f64(capped).map(money::round_money)
}

/// Days remaining, rounded up: a deadline three hours away still shows "1 jour".
fn whole_days_until(now: Timestamp, deadline: Timestamp) -> i64 {
    let millis = deadline
        .as_millisecond()
        .saturating_sub(now.as_millisecond());
    if millis <= 0 {
        return 0;
    }
    // Unsigned, because `div_ceil` is only stable for unsigned integers and
    // `millis` is known positive here.
    i64::try_from(millis.unsigned_abs().div_ceil(MILLIS_PER_DAY_U64)).unwrap_or(i64::MAX)
}

#[expect(
    clippy::cast_precision_loss,
    reason = "timestamps are milliseconds since 1970; f64 is exact well past year 10000"
)]
fn days_between(from: Timestamp, to: Timestamp) -> f64 {
    let millis = to.as_millisecond().saturating_sub(from.as_millisecond());
    millis as f64 / MILLIS_PER_DAY
}

fn years_between(from: Timestamp, to: Timestamp) -> f64 {
    days_between(from, to).max(0.0) / DAYS_PER_YEAR
}
