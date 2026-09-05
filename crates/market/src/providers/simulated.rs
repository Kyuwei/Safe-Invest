//! The source of last resort: a deterministic pretend market.
//!
//! It exists so the app is never dead — offline, rate-limited, or on a CI
//! runner with no network, the player still gets a working game. Every quote it
//! produces is flagged `is_simulated`, and that flag is carried all the way to
//! the badge in the interface. Teaching someone with a number they believe is
//! real when it is not would be the worst bug this program could have.

use crate::error::ProviderResult;
use crate::providers::{PricePoint, QuoteProvider};
use async_trait::async_trait;
use jiff::Timestamp;
use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;
use safe_invest_core::model::{Asset, AssetKind, Quote};
use sha2::{Digest, Sha256};

pub const ID: &str = "simulated";

/// Anchors so a simulated Bitcoin looks like a Bitcoin. Without these the walk
/// starts near 1 and the lesson becomes surreal.
const ANCHORS: &[(&str, f64)] = &[
    ("crypto:BTC", 68_000.0),
    ("crypto:ETH", 3_200.0),
    ("crypto:SOL", 145.0),
    ("crypto:XRP", 0.62),
    ("crypto:ADA", 0.45),
    ("crypto:DOGE", 0.14),
    ("stock:AAPL", 210.0),
    ("stock:MSFT", 420.0),
    ("stock:GOOGL", 175.0),
    ("stock:AMZN", 185.0),
    ("stock:TSLA", 240.0),
    ("stock:NVDA", 118.0),
    ("etf:CW8", 520.0),
    ("etf:VWCE", 122.0),
    ("etf:SPY", 545.0),
];

const MINUTES_PER_DAY: f64 = 1_440.0;
const MINUTES_PER_WEEK: f64 = 10_080.0;
const MINUTES_PER_QUARTER: f64 = 131_400.0;

#[derive(Debug, Default)]
pub struct SimulatedProvider;

impl SimulatedProvider {
    pub fn new() -> Self {
        Self
    }

    #[expect(
        clippy::cast_precision_loss,
        reason = "a Unix second fits f64 exactly for another 285 million years"
    )]
    /// A smooth, repeatable walk: three sine waves at different periods, seeded
    /// from the asset key. The same symbol at the same minute always gives the
    /// same price, so screenshots and tests are stable, and two processes
    /// looking at one game agree on what things are worth.
    fn price_at(key: &str, at: Timestamp) -> (f64, f64) {
        let seed = seed_of(key);
        let anchor = ANCHORS
            .iter()
            .find(|(k, _)| *k == key)
            .map_or_else(|| 10.0 + f64::from(seed % 400), |(_, price)| *price);

        let minutes = at.as_second() as f64 / 60.0;
        let phase = f64::from(seed % 360);

        let price = anchor * (1.0 + drift_at(minutes, phase));
        let yesterday = anchor * (1.0 + drift_at(minutes - MINUTES_PER_DAY, phase));

        let change_percent = if yesterday.abs() < f64::EPSILON {
            0.0
        } else {
            (price - yesterday) / yesterday * 100.0
        };

        (price.max(0.000_001), change_percent)
    }

    fn quote_for(asset: &Asset, currency: &str, at: Timestamp) -> Option<Quote> {
        let (price, change) = Self::price_at(&asset.key(), at);
        Some(Quote {
            symbol: asset.symbol.clone(),
            kind: asset.kind,
            price: Decimal::from_f64(price)?.round_dp(if price < 1.0 { 6 } else { 2 }),
            currency: currency.to_owned(),
            as_of: at,
            source_id: ID.to_owned(),
            is_simulated: true,
            name: Some(asset.name.clone()),
            change_percent_24h: Decimal::from_f64(change).map(|d| d.round_dp(2)),
            market_cap: None,
            volume_24h: None,
        })
    }
}

#[async_trait]
impl QuoteProvider for SimulatedProvider {
    fn id(&self) -> &'static str {
        ID
    }

    fn label(&self) -> &'static str {
        "Marché simulé"
    }

    fn supports(&self, _kind: AssetKind) -> bool {
        true
    }

    fn is_simulated(&self) -> bool {
        true
    }

    async fn quotes(&self, assets: &[Asset], currency: &str) -> ProviderResult<Vec<Quote>> {
        let now = Timestamp::now();
        Ok(assets
            .iter()
            .filter_map(|asset| Self::quote_for(asset, currency, now))
            .collect())
    }

    async fn search(&self, query: &str, kind: Option<AssetKind>) -> ProviderResult<Vec<Asset>> {
        Ok(crate::catalog::search(query, kind))
    }

    async fn history(
        &self,
        asset: &Asset,
        days: u16,
        _currency: &str,
    ) -> ProviderResult<Vec<PricePoint>> {
        let now = Timestamp::now();
        let key = asset.key();
        let mut points = Vec::with_capacity(days as usize);

        for day in (0..days).rev() {
            // Hours, not days: a `Timestamp` has no calendar, so a span in days
            // is rejected outright rather than assumed to be 24 hours.
            let Some(at) = now
                .checked_sub(jiff::Span::new().hours(i64::from(day) * 24))
                .ok()
            else {
                continue;
            };
            let (price, _) = Self::price_at(&key, at);
            if let Some(price) = Decimal::from_f64(price) {
                points.push(PricePoint {
                    at,
                    price: price.round_dp(if price < Decimal::ONE { 6 } else { 2 }),
                });
            }
        }
        Ok(points)
    }
}

/// Day, week and quarter rhythms combined, so the line looks alive whether the
/// player is watching a sparkline or a year of history.
fn drift_at(minutes: f64, phase: f64) -> f64 {
    let wave = |period: f64, amplitude: f64| ((minutes / period + phase).sin()) * amplitude;
    wave(MINUTES_PER_DAY, 0.06) + wave(MINUTES_PER_WEEK, 0.12) + wave(MINUTES_PER_QUARTER, 0.20)
}

fn seed_of(key: &str) -> u32 {
    let digest = Sha256::digest(key.as_bytes());
    u32::from_be_bytes([
        digest.first().copied().unwrap_or_default(),
        digest.get(1).copied().unwrap_or_default(),
        digest.get(2).copied().unwrap_or_default(),
        digest.get(3).copied().unwrap_or_default(),
    ])
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

    fn asset(symbol: &str, kind: AssetKind) -> Asset {
        Asset::new(symbol, symbol, kind)
    }

    #[tokio::test]
    async fn a_simulated_bitcoin_costs_roughly_what_a_bitcoin_costs() {
        let quotes = SimulatedProvider
            .quotes(&[asset("BTC", AssetKind::Crypto)], "EUR")
            .await
            .unwrap();

        let price = quotes.first().unwrap().price;
        assert!(
            price > Decimal::from(30_000) && price < Decimal::from(120_000),
            "un bitcoin simulé à {price} € casserait la leçon"
        );
    }

    #[test]
    fn the_same_asset_at_the_same_instant_always_gives_the_same_price() {
        let at: Timestamp = "2026-03-01T10:00:00Z".parse().unwrap();
        let (first, _) = SimulatedProvider::price_at("crypto:BTC", at);
        let (second, _) = SimulatedProvider::price_at("crypto:BTC", at);
        assert!((first - second).abs() < f64::EPSILON);
    }

    #[test]
    fn different_assets_do_not_move_in_lockstep() {
        let at: Timestamp = "2026-03-01T10:00:00Z".parse().unwrap();
        let (_, btc) = SimulatedProvider::price_at("crypto:BTC", at);
        let (_, eth) = SimulatedProvider::price_at("crypto:ETH", at);
        assert!(
            (btc - eth).abs() > 1e-9,
            "toutes les courbes seraient identiques"
        );
    }

    #[tokio::test]
    async fn every_simulated_quote_admits_that_it_is_simulated() {
        let quotes = SimulatedProvider
            .quotes(
                &[
                    asset("BTC", AssetKind::Crypto),
                    asset("MSFT", AssetKind::Stock),
                ],
                "EUR",
            )
            .await
            .unwrap();

        assert_eq!(quotes.len(), 2);
        assert!(quotes.iter().all(|q| q.is_simulated));
        assert!(quotes.iter().all(|q| q.source_id == ID));
    }

    #[tokio::test]
    async fn a_price_is_never_zero_or_negative() {
        let quotes = SimulatedProvider
            .quotes(&[asset("ZZZZ", AssetKind::Stock)], "EUR")
            .await
            .unwrap();
        assert!(quotes.first().unwrap().price > Decimal::ZERO);
    }

    #[tokio::test]
    async fn history_is_ordered_oldest_first() {
        let points = SimulatedProvider
            .history(&asset("BTC", AssetKind::Crypto), 30, "EUR")
            .await
            .unwrap();

        assert_eq!(points.len(), 30);
        assert!(points.windows(2).all(|w| w[0].at < w[1].at));
    }
}
