//! Currency conversion.
//!
//! Yahoo quotes a Paris listing in euros and a New York one in dollars;
//! Finnhub quotes everything in dollars. A game is played in one currency, so
//! every price has to land there before it reaches the engine — which refuses a
//! quote in the wrong currency precisely so this step can never be skipped.

use crate::cache::TtlCache;
use crate::error::ProviderResult;
use crate::http::HttpClient;
use crate::providers::decimal_from_json;
use rust_decimal::Decimal;
use safe_invest_core::model::Quote;
use std::time::Duration;

const FRANKFURTER: &str = "https://api.frankfurter.dev/v1/latest";
const YAHOO_FX: &str = "https://query1.finance.yahoo.com/v8/finance/chart";
const ID: &str = "fx";

/// Exchange rates move slowly enough that an hour-old one is fine, and this
/// keeps a portfolio refresh from calling the rate service every minute.
const TTL: Duration = Duration::from_secs(3600);

#[derive(Debug)]
pub struct FxRates {
    http: HttpClient,
    cache: TtlCache<(String, String), Decimal>,
}

impl FxRates {
    pub fn new(http: HttpClient) -> Self {
        Self {
            http,
            cache: TtlCache::new(TTL),
        }
    }

    /// Seeds a rate directly, bypassing the network.
    ///
    /// Used by the tests, and by nothing else — a rate that was never fetched
    /// should never reach a player's portfolio.
    pub fn preload(&self, from: &str, to: &str, rate: Decimal) {
        self.cache
            .insert((from.to_uppercase(), to.to_uppercase()), rate);
    }

    /// How many `to` one `from` buys. `None` when no source could say — the
    /// caller then leaves the price in its original currency rather than
    /// guessing.
    pub async fn rate(&self, from: &str, to: &str) -> Option<Decimal> {
        let from = from.to_uppercase();
        let to = to.to_uppercase();
        if from == to {
            return Some(Decimal::ONE);
        }

        let key = (from.clone(), to.clone());
        if let Some(cached) = self.cache.get(&key) {
            return Some(cached);
        }

        let rate = match self.via_frankfurter(&from, &to).await {
            Ok(Some(rate)) => Some(rate),
            other => {
                if let Err(error) = other {
                    tracing::debug!(%error, "taux de change : repli sur Yahoo");
                }
                self.via_yahoo(&from, &to).await.ok().flatten()
            }
        }?;

        if rate <= Decimal::ZERO {
            return None;
        }
        self.cache.insert(key, rate);
        Some(rate)
    }

    /// Converts a quote in place, or leaves it untouched if no rate is known.
    ///
    /// Percentages are *not* converted: a share up 2 % is up 2 % in every
    /// currency, and multiplying that by a rate would be a nonsense the player
    /// would see as a wrong colour.
    pub async fn convert(&self, quote: &mut Quote, target: &str) -> bool {
        if quote.currency.eq_ignore_ascii_case(target) {
            quote.currency = target.to_uppercase();
            return true;
        }

        let Some(rate) = self.rate(&quote.currency, target).await else {
            return false;
        };
        let Some(price) = quote.price.checked_mul(rate) else {
            return false;
        };

        quote.price = price.round_dp(if price < Decimal::ONE { 8 } else { 4 });
        quote.market_cap = quote.market_cap.and_then(|v| v.checked_mul(rate));
        quote.volume_24h = quote.volume_24h.and_then(|v| v.checked_mul(rate));
        quote.currency = target.to_uppercase();
        true
    }

    /// The European Central Bank's daily reference rates, via Frankfurter. No
    /// key, no quota, and an authoritative source for a teaching tool.
    async fn via_frankfurter(&self, from: &str, to: &str) -> ProviderResult<Option<Decimal>> {
        let url = format!("{FRANKFURTER}?base={from}&symbols={to}");
        let body: serde_json::Value = self.http.get_json(ID, &url, &[]).await?;
        Ok(body
            .pointer(&format!("/rates/{to}"))
            .and_then(decimal_from_json))
    }

    async fn via_yahoo(&self, from: &str, to: &str) -> ProviderResult<Option<Decimal>> {
        let url = format!("{YAHOO_FX}/{from}{to}=X?range=1d&interval=1d");
        let body: serde_json::Value = self
            .http
            .get_json(ID, &url, &[("accept", "application/json")])
            .await?;
        Ok(body
            .pointer("/chart/result/0/meta/regularMarketPrice")
            .and_then(decimal_from_json))
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::unchecked_time_subtraction,
    reason = "a test that trips is a test that failed"
)]
mod tests {
    use super::*;
    use jiff::Timestamp;
    use safe_invest_core::model::AssetKind;

    fn quote(currency: &str, price: &str, change: &str) -> Quote {
        use std::str::FromStr;
        Quote {
            symbol: "AAPL".into(),
            kind: AssetKind::Stock,
            price: Decimal::from_str(price).unwrap(),
            currency: currency.into(),
            as_of: Timestamp::now(),
            source_id: "test".into(),
            is_simulated: false,
            name: None,
            change_percent_24h: Some(Decimal::from_str(change).unwrap()),
            market_cap: None,
            volume_24h: None,
        }
    }

    #[tokio::test]
    async fn converting_to_the_same_currency_is_a_no_op_and_needs_no_network() {
        let fx = FxRates::new(HttpClient::new().unwrap());
        let mut q = quote("eur", "100", "2.5");
        assert!(fx.convert(&mut q, "EUR").await);
        assert_eq!(q.price, Decimal::from(100));
        assert_eq!(q.currency, "EUR");
    }

    #[tokio::test]
    async fn a_cached_rate_is_applied_to_the_price_but_never_to_the_percentage() {
        use std::str::FromStr;
        let fx = FxRates::new(HttpClient::new().unwrap());
        fx.cache.insert(
            ("USD".into(), "EUR".into()),
            Decimal::from_str("0.9").unwrap(),
        );

        let mut q = quote("USD", "200", "2.5");
        assert!(fx.convert(&mut q, "EUR").await);

        assert_eq!(q.price, Decimal::from(180));
        assert_eq!(q.currency, "EUR");
        assert_eq!(
            q.change_percent_24h,
            Some(Decimal::from_str("2.5").unwrap()),
            "une variation en pourcentage ne se convertit pas"
        );
    }

    #[tokio::test]
    async fn the_identity_rate_needs_no_lookup() {
        let fx = FxRates::new(HttpClient::new().unwrap());
        assert_eq!(fx.rate("EUR", "eur").await, Some(Decimal::ONE));
    }
}
