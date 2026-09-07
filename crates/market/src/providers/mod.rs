//! The quote sources, behind one interface.

pub mod coingecko;
pub mod coinmarketcap;
pub mod finnhub;
pub mod scrape;
pub mod simulated;
pub mod yahoo;

use crate::error::{ProviderError, ProviderResult};
use async_trait::async_trait;
use jiff::Timestamp;
use rust_decimal::Decimal;
use safe_invest_core::model::{Asset, AssetKind, Quote};
use std::future::Future;

/// One point on a price history.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PricePoint {
    pub at: Timestamp,
    pub price: Decimal,
}

/// A source of prices.
///
/// Deliberately fallible in a boring way: every method may fail, and the
/// service above treats a failure as "try the next source" rather than as an
/// error to show the player.
#[async_trait]
pub trait QuoteProvider: Send + Sync + std::fmt::Debug {
    /// Stable identifier, used in settings, in `Quote::source_id` and in the UI.
    fn id(&self) -> &'static str;

    /// What to call it on screen.
    fn label(&self) -> &'static str;

    fn supports(&self, kind: AssetKind) -> bool;

    /// True when the source cannot be used at all without a key. The chain
    /// drops these rather than calling them and collecting a 401.
    fn is_configured(&self) -> bool {
        true
    }

    /// Whether prices from this source are invented. Only the simulator says
    /// yes, and the answer rides on every quote it produces.
    fn is_simulated(&self) -> bool {
        false
    }

    /// Quotes for as many of `assets` as this source can price. Returning a
    /// partial list is normal and expected; the chain asks the next source for
    /// the rest.
    async fn quotes(&self, assets: &[Asset], currency: &str) -> ProviderResult<Vec<Quote>>;

    /// Free-text search. An empty result is not an error.
    async fn search(&self, query: &str, kind: Option<AssetKind>) -> ProviderResult<Vec<Asset>> {
        let _ = (query, kind);
        Ok(Vec::new())
    }

    /// Daily closes over `days`, oldest first.
    async fn history(
        &self,
        asset: &Asset,
        days: u16,
        currency: &str,
    ) -> ProviderResult<Vec<PricePoint>> {
        let _ = (asset, days, currency);
        Ok(Vec::new())
    }
}

/// Fetches one quote per asset, a few at a time, and keeps what came back.
///
/// Two rules, both learned from the shape of the failure they prevent:
///
/// A source that answers for some symbols and not others must return what it
/// has. The chain asks the next source for the rest, so discarding six good
/// prices because a seventh symbol was unknown would cost those six lines
/// their price for nothing.
///
/// But a source that answered *nothing* and hit an error must say so, rather
/// than report an empty success — otherwise the status light stays green while
/// the source is down.
pub(crate) async fn collect_quotes<Fut>(
    fetches: Vec<Fut>,
    concurrency: usize,
) -> ProviderResult<Vec<Quote>>
where
    Fut: Future<Output = ProviderResult<Option<Quote>>>,
{
    use futures_util::StreamExt as _;

    let outcomes: Vec<ProviderResult<Option<Quote>>> = futures_util::stream::iter(fetches)
        // One request per symbol is what these endpoints allow; issuing a few
        // at once turns a twenty-symbol refresh from seconds into fractions of
        // one, without exceeding any provider's stated budget.
        .buffered(concurrency.max(1))
        .collect()
        .await;

    let mut quotes = Vec::with_capacity(outcomes.len());
    let mut failure: Option<ProviderError> = None;

    for outcome in outcomes {
        match outcome {
            Ok(Some(quote)) => quotes.push(quote),
            Ok(None) => {}
            Err(error) => {
                if failure.is_none() {
                    failure = Some(error);
                }
            }
        }
    }

    match failure {
        Some(error) if quotes.is_empty() => Err(error),
        _ => Ok(quotes),
    }
}

/// Parses a JSON number or numeric string into a `Decimal` without going
/// through `f64` when it can be avoided.
pub(crate) fn decimal_from_json(value: &serde_json::Value) -> Option<Decimal> {
    use rust_decimal::prelude::FromPrimitive;
    use std::str::FromStr;

    match value {
        serde_json::Value::String(text) => Decimal::from_str(text.trim()).ok(),
        serde_json::Value::Number(number) => {
            // Exact for integers; only the fractional case needs the f64 path,
            // and a price with more than 28 significant digits is not real.
            if let Some(int) = number.as_i64() {
                return Some(Decimal::from(int));
            }
            // `Number`'s Display is the shortest text that round-trips the
            // value, so parsing it keeps every digit JSON actually carried.
            Decimal::from_str(&number.to_string())
                .ok()
                .or_else(|| number.as_f64().and_then(Decimal::from_f64))
        }
        _ => None,
    }
}
