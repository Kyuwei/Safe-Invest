//! Rounding and overflow rules for every amount in the game.
//!
//! Money is `Decimal`, never `f64`: a portfolio that loses a cent per refresh
//! is worse than useless in a teaching tool. All arithmetic goes through the
//! checked helpers below so an absurd input (a scraped price of 1e28, a
//! quantity pasted with twenty digits) surfaces as an error instead of a panic
//! inside the trading engine.

use rust_decimal::{Decimal, RoundingStrategy};

/// Cents. Everything the player sees as an amount is rounded to this.
pub const MONEY_DECIMALS: u32 = 2;

/// Enough precision for fractional crypto — 1 satoshi is 1e-8 BTC.
pub const QUANTITY_DECIMALS: u32 = 8;

/// A computation that could not be represented as a `Decimal`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("dépassement de capacité lors d'un calcul monétaire")]
pub struct Overflow;

type Result<T> = core::result::Result<T, Overflow>;

/// Rounds to the cent, halves away from zero — the convention a bank statement uses.
pub fn round_money(value: Decimal) -> Decimal {
    value.round_dp_with_strategy(MONEY_DECIMALS, RoundingStrategy::MidpointAwayFromZero)
}

/// Rounds a quantity *down*, always.
///
/// "Buy as much as 100 € allows" must never round up into spending 100.01 €,
/// and "sell everything" must never try to sell more than is held.
pub fn round_quantity_down(value: Decimal) -> Decimal {
    value.round_dp_with_strategy(QUANTITY_DECIMALS, RoundingStrategy::ToZero)
}

pub fn add(a: Decimal, b: Decimal) -> Result<Decimal> {
    a.checked_add(b).ok_or(Overflow)
}

pub fn sub(a: Decimal, b: Decimal) -> Result<Decimal> {
    a.checked_sub(b).ok_or(Overflow)
}

pub fn mul(a: Decimal, b: Decimal) -> Result<Decimal> {
    a.checked_mul(b).ok_or(Overflow)
}

/// Division that also refuses a zero divisor rather than panicking on it.
pub fn div(a: Decimal, b: Decimal) -> Result<Decimal> {
    a.checked_div(b).ok_or(Overflow)
}

/// Multiply, then round to the cent.
pub fn money_mul(a: Decimal, b: Decimal) -> Result<Decimal> {
    Ok(round_money(mul(a, b)?))
}

/// `part` as a percentage of `whole`, to four decimals. A zero `whole` yields
/// zero rather than an error: a portfolio worth nothing is 0 % of itself, and
/// that is the sensible thing to draw.
pub fn percent(part: Decimal, whole: Decimal) -> Result<Decimal> {
    if whole.is_zero() {
        return Ok(Decimal::ZERO);
    }
    let ratio = mul(div(part, whole)?, Decimal::ONE_HUNDRED)?;
    Ok(ratio.round_dp_with_strategy(4, RoundingStrategy::MidpointAwayFromZero))
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a failed unwrap is a failed test")]
mod tests {
    use super::*;
    use rust_decimal::prelude::FromPrimitive;
    use std::str::FromStr;

    fn d(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    #[test]
    fn money_rounds_half_away_from_zero() {
        assert_eq!(round_money(d("1.005")), d("1.01"));
        assert_eq!(round_money(d("-1.005")), d("-1.01"));
        assert_eq!(round_money(d("2.344")), d("2.34"));
    }

    #[test]
    fn quantities_always_round_down() {
        assert_eq!(round_quantity_down(d("0.999999999")), d("0.99999999"));
        assert_eq!(round_quantity_down(d("1.000000009")), d("1.0"));
    }

    #[test]
    fn percent_of_nothing_is_zero_not_an_error() {
        assert_eq!(percent(d("50"), Decimal::ZERO).unwrap(), Decimal::ZERO);
    }

    #[test]
    fn percent_keeps_four_decimals() {
        assert_eq!(percent(d("1"), d("3")).unwrap(), d("33.3333"));
    }

    #[test]
    fn overflow_is_reported_not_panicked() {
        let huge = Decimal::MAX;
        assert_eq!(mul(huge, d("2")), Err(Overflow));
        assert_eq!(add(huge, huge), Err(Overflow));
        assert_eq!(div(d("1"), Decimal::ZERO), Err(Overflow));
    }

    #[test]
    fn f64_is_never_needed_for_a_price() {
        // Guards the reason this module exists: 0.1 + 0.2 is exact here.
        let sum = add(
            Decimal::from_f64(0.1).unwrap().round_dp(1),
            Decimal::from_f64(0.2).unwrap().round_dp(1),
        )
        .unwrap();
        assert_eq!(sum, d("0.3"));
    }
}
