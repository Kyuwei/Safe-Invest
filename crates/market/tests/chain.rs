//! The fall-through chain, driven by fake sources.
//!
//! This is where the promise of the crate is checked: whatever fails, the
//! player still gets a number, and that number never claims to be more real
//! than it is.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a test that trips is a test that failed"
)]

use async_trait::async_trait;
use jiff::Timestamp;
use rust_decimal::Decimal;
use safe_invest_core::model::{Asset, AssetKind, Quote};
use safe_invest_market::error::{ProviderError, ProviderResult};
use safe_invest_market::fx::FxRates;
use safe_invest_market::http::HttpClient;
use safe_invest_market::providers::{QuoteProvider, simulated::SimulatedProvider};
use safe_invest_market::service::{ChainOptions, MarketDataService};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

fn d(text: &str) -> Decimal {
    Decimal::from_str(text).unwrap()
}

/// A source that answers with fixed prices, or refuses, and counts its calls.
#[derive(Debug)]
struct FakeSource {
    id: &'static str,
    currency: &'static str,
    prices: HashMap<String, Decimal>,
    failure: Option<ProviderError>,
    calls: AtomicUsize,
}

impl FakeSource {
    fn answering(id: &'static str, currency: &'static str, prices: &[(&str, &str)]) -> Arc<Self> {
        Arc::new(Self {
            id,
            currency,
            prices: prices
                .iter()
                .map(|(symbol, price)| ((*symbol).to_owned(), d(price)))
                .collect(),
            failure: None,
            calls: AtomicUsize::new(0),
        })
    }

    fn failing(id: &'static str, failure: ProviderError) -> Arc<Self> {
        Arc::new(Self {
            id,
            currency: "EUR",
            prices: HashMap::new(),
            failure: Some(failure),
            calls: AtomicUsize::new(0),
        })
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl QuoteProvider for FakeSource {
    fn id(&self) -> &'static str {
        self.id
    }

    fn label(&self) -> &'static str {
        "Source de test"
    }

    fn supports(&self, _kind: AssetKind) -> bool {
        true
    }

    async fn quotes(&self, assets: &[Asset], _currency: &str) -> ProviderResult<Vec<Quote>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(failure) = &self.failure {
            return Err(match failure {
                ProviderError::RateLimited { provider } => ProviderError::RateLimited { provider },
                other => ProviderError::Transport {
                    provider: self.id,
                    detail: other.to_string(),
                },
            });
        }

        Ok(assets
            .iter()
            .filter_map(|asset| {
                Some(Quote {
                    symbol: asset.symbol.clone(),
                    kind: asset.kind,
                    price: *self.prices.get(&asset.symbol)?,
                    currency: self.currency.to_owned(),
                    as_of: Timestamp::now(),
                    source_id: self.id.to_owned(),
                    is_simulated: false,
                    name: Some(asset.name.clone()),
                    change_percent_24h: None,
                    market_cap: None,
                    volume_24h: None,
                })
            })
            .collect())
    }
}

fn service(providers: Vec<Arc<dyn QuoteProvider>>, order: &[&str]) -> MarketDataService {
    let ids: Vec<String> = order.iter().map(|s| (*s).to_owned()).collect();
    let mut by_kind = HashMap::new();
    for kind in AssetKind::ALL {
        by_kind.insert(kind, ids.clone());
    }

    MarketDataService::with_providers(
        providers,
        ChainOptions {
            order: by_kind,
            force_simulated: false,
            cache_ttl: Duration::from_secs(60),
        },
        FxRates::new(HttpClient::new().unwrap()),
    )
}

fn btc() -> Asset {
    Asset::new("BTC", "Bitcoin", AssetKind::Crypto)
}

#[tokio::test]
async fn the_first_working_source_wins() {
    let primary = FakeSource::answering("primary", "EUR", &[("BTC", "60000")]);
    let backup = FakeSource::answering("backup", "EUR", &[("BTC", "1")]);
    let market = service(
        vec![primary.clone(), backup.clone()],
        &["primary", "backup"],
    );

    let batch = market.quotes(&[btc()], "EUR").await;

    assert_eq!(batch.quotes["crypto:BTC"].price, d("60000"));
    assert_eq!(
        backup.calls(),
        0,
        "la source de secours ne doit pas être appelée pour rien"
    );
}

#[tokio::test]
async fn a_failing_source_hands_over_to_the_next_one() {
    let broken = FakeSource::failing("broken", ProviderError::RateLimited { provider: "broken" });
    let backup = FakeSource::answering("backup", "EUR", &[("BTC", "59000")]);
    let market = service(vec![broken.clone(), backup.clone()], &["broken", "backup"]);

    let batch = market.quotes(&[btc()], "EUR").await;

    assert_eq!(batch.quotes["crypto:BTC"].source_id, "backup");
    assert_eq!(broken.calls(), 1);
    assert_eq!(backup.calls(), 1);
}

#[tokio::test]
async fn when_every_real_source_fails_the_simulator_still_answers_and_admits_it() {
    let broken = FakeSource::failing("broken", ProviderError::RateLimited { provider: "broken" });
    let market = service(
        vec![broken, Arc::new(SimulatedProvider::new())],
        &["broken", "simulated"],
    );

    let batch = market.quotes(&[btc()], "EUR").await;

    let quote = &batch.quotes["crypto:BTC"];
    assert!(
        quote.is_simulated,
        "le repli doit être signalé comme simulé"
    );
    assert_eq!(quote.source_id, "simulated");
    assert!(batch.unpriced.is_empty());
}

#[tokio::test]
async fn the_simulator_is_appended_even_when_the_settings_forgot_it() {
    let broken = FakeSource::failing("broken", ProviderError::RateLimited { provider: "broken" });
    // Note the order lists only the broken source.
    let market = service(
        vec![broken, Arc::new(SimulatedProvider::new())],
        &["broken"],
    );

    let batch = market.quotes(&[btc()], "EUR").await;

    assert!(
        batch.quotes.contains_key("crypto:BTC"),
        "l'app ne doit jamais rester muette"
    );
}

#[tokio::test]
async fn a_partial_answer_is_completed_by_the_next_source() {
    let partial = FakeSource::answering("partial", "EUR", &[("BTC", "60000")]);
    let rest = FakeSource::answering("rest", "EUR", &[("ETH", "2900")]);
    let market = service(vec![partial, rest], &["partial", "rest"]);

    let batch = market
        .quotes(
            &[btc(), Asset::new("ETH", "Ethereum", AssetKind::Crypto)],
            "EUR",
        )
        .await;

    assert_eq!(batch.quotes.len(), 2);
    assert_eq!(batch.quotes["crypto:BTC"].source_id, "partial");
    assert_eq!(batch.quotes["crypto:ETH"].source_id, "rest");
}

#[tokio::test]
async fn a_price_in_another_currency_is_converted_before_it_is_used() {
    let usd = FakeSource::answering("usd-source", "USD", &[("BTC", "60000")]);
    let market = service(vec![usd], &["usd-source"]);
    // No network: the rate is seeded, as it would be from a previous lookup.
    market_fx(&market).preload("USD", "EUR", d("0.9"));

    let batch = market.quotes(&[btc()], "EUR").await;

    let quote = &batch.quotes["crypto:BTC"];
    assert_eq!(quote.currency, "EUR");
    assert_eq!(quote.price, d("54000"));
}

#[tokio::test]
async fn a_second_lookup_is_served_from_the_cache() {
    let source = FakeSource::answering("primary", "EUR", &[("BTC", "60000")]);
    let market = service(vec![source.clone()], &["primary"]);

    market.quotes(&[btc()], "EUR").await;
    market.quotes(&[btc()], "EUR").await;

    assert_eq!(source.calls(), 1, "le cache doit éviter le second appel");

    market.invalidate();
    market.quotes(&[btc()], "EUR").await;
    assert_eq!(
        source.calls(),
        2,
        "après invalidation, l'appel doit repartir"
    );
}

#[tokio::test]
async fn demo_mode_ignores_every_real_source() {
    let real = FakeSource::answering("real", "EUR", &[("BTC", "60000")]);
    let mut by_kind = HashMap::new();
    for kind in AssetKind::ALL {
        by_kind.insert(kind, vec!["real".to_owned()]);
    }

    let market = MarketDataService::with_providers(
        vec![real.clone(), Arc::new(SimulatedProvider::new())],
        ChainOptions {
            order: by_kind,
            force_simulated: true,
            cache_ttl: Duration::from_secs(60),
        },
        FxRates::new(HttpClient::new().unwrap()),
    );

    let batch = market.quotes(&[btc()], "EUR").await;

    assert_eq!(real.calls(), 0);
    assert!(batch.quotes["crypto:BTC"].is_simulated);
}

#[tokio::test]
async fn a_source_that_failed_is_reported_as_unhealthy_with_a_reason() {
    let broken = FakeSource::failing("broken", ProviderError::RateLimited { provider: "broken" });
    let market = service(
        vec![broken, Arc::new(SimulatedProvider::new())],
        &["broken", "simulated"],
    );

    market.quotes(&[btc()], "EUR").await;

    let statuses = market.statuses();
    let broken = statuses.iter().find(|s| s.id == "broken").unwrap();
    assert_eq!(broken.healthy, Some(false));
    assert!(broken.detail.is_some());

    let simulator = statuses.iter().find(|s| s.id == "simulated").unwrap();
    assert_eq!(simulator.healthy, Some(true));
    assert!(simulator.is_simulated);
}

/// Reaches the service's rate table. Kept in one place so the tests read
/// cleanly and the accessor stays out of the public API.
fn market_fx(market: &MarketDataService) -> &FxRates {
    market.fx()
}
