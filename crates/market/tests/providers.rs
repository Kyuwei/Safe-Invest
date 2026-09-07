//! Each provider against a recorded payload from its real API.
//!
//! These are the tests that catch a source quietly changing its JSON — the
//! failure that otherwise shows up as an empty dashboard.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "a test that trips is a test that failed"
)]

mod support;

use rust_decimal::Decimal;
use safe_invest_core::model::{Asset, AssetKind};
use safe_invest_market::http::HttpClient;
use safe_invest_market::providers::QuoteProvider;
use safe_invest_market::providers::coingecko::CoinGeckoProvider;
use safe_invest_market::providers::coinmarketcap::CoinMarketCapProvider;
use safe_invest_market::providers::finnhub::FinnhubProvider;
use safe_invest_market::providers::yahoo::YahooProvider;
use std::str::FromStr;
use support::{FakeApi, Reply, routes};

fn d(text: &str) -> Decimal {
    Decimal::from_str(text).unwrap()
}

fn http() -> HttpClient {
    HttpClient::new().unwrap()
}

// ------------------------------------------------------------- CoinGecko

const COINGECKO_PRICE: &str = r#"{
  "bitcoin": { "eur": 61234.5, "eur_market_cap": 1.2e12, "eur_24h_vol": 2.5e10, "eur_24h_change": -1.8452 },
  "ethereum": { "eur": 2890.12, "eur_market_cap": 3.4e11, "eur_24h_vol": 9.0e9, "eur_24h_change": 2.301 }
}"#;

#[tokio::test]
async fn coingecko_reads_price_change_and_market_cap() {
    let api = FakeApi::start(routes(&[("/simple/price", Reply::ok(COINGECKO_PRICE))])).await;
    let provider = CoinGeckoProvider::new(http(), None).with_base(&api.origin);

    let assets = vec![
        Asset::new("BTC", "Bitcoin", AssetKind::Crypto),
        Asset::new("ETH", "Ethereum", AssetKind::Crypto),
    ];
    let quotes = provider.quotes(&assets, "EUR").await.unwrap();

    assert_eq!(quotes.len(), 2);
    let btc = quotes.iter().find(|q| q.symbol == "BTC").unwrap();
    assert_eq!(btc.price, d("61234.5"));
    assert_eq!(btc.currency, "EUR");
    assert_eq!(btc.change_percent_24h, Some(d("-1.85")));
    assert!(!btc.is_simulated);
    assert_eq!(btc.source_id, "coingecko");
    assert!(btc.market_cap.is_some());
}

#[tokio::test]
async fn coingecko_ignores_a_coin_the_payload_does_not_carry() {
    let api = FakeApi::start(routes(&[(
        "/simple/price",
        Reply::ok(r#"{"bitcoin":{"eur":61234.5}}"#),
    )]))
    .await;
    let provider = CoinGeckoProvider::new(http(), None).with_base(&api.origin);

    let assets = vec![
        Asset::new("BTC", "Bitcoin", AssetKind::Crypto),
        Asset::new("ETH", "Ethereum", AssetKind::Crypto),
    ];
    let quotes = provider.quotes(&assets, "EUR").await.unwrap();

    assert_eq!(
        quotes.len(),
        1,
        "un actif absent ne doit pas inventer de cours"
    );
    assert_eq!(quotes[0].symbol, "BTC");
}

#[tokio::test]
async fn coingecko_reports_a_rate_limit_rather_than_a_generic_failure() {
    let api = FakeApi::start(routes(&[("/simple/price", Reply::status(429))])).await;
    let provider = CoinGeckoProvider::new(http(), None).with_base(&api.origin);

    let error = provider
        .quotes(&[Asset::new("BTC", "Bitcoin", AssetKind::Crypto)], "EUR")
        .await
        .unwrap_err();

    assert!(error.is_transient(), "un 429 est temporaire : {error}");
}

#[tokio::test]
async fn coingecko_search_maps_results_to_assets_with_their_slug() {
    let body =
        r#"{"coins":[{"id":"solana","symbol":"sol","name":"Solana","thumb":"https://x/y.png"}]}"#;
    let api = FakeApi::start(routes(&[("/search", Reply::ok(body))])).await;
    let provider = CoinGeckoProvider::new(http(), None).with_base(&api.origin);

    let found = provider.search("sol", None).await.unwrap();

    assert_eq!(found.len(), 1);
    assert_eq!(found[0].symbol, "SOL");
    assert_eq!(found[0].provider_id.as_deref(), Some("solana"));
    assert_eq!(found[0].kind, AssetKind::Crypto);
}

#[tokio::test]
async fn coingecko_history_is_parsed_from_millisecond_pairs() {
    let body = r#"{"prices":[[1750000000000,58000.0],[1750086400000,59500.25]]}"#;
    let api = FakeApi::start(routes(&[("/coins/", Reply::ok(body))])).await;
    let provider = CoinGeckoProvider::new(http(), None).with_base(&api.origin);

    let points = provider
        .history(&Asset::new("BTC", "Bitcoin", AssetKind::Crypto), 2, "EUR")
        .await
        .unwrap();

    assert_eq!(points.len(), 2);
    assert_eq!(points[1].price, d("59500.25"));
    assert!(points[0].at < points[1].at);
}

// ----------------------------------------------------------------- Yahoo

const YAHOO_CHART: &str = r#"{
  "chart": { "result": [ {
      "meta": { "currency": "USD", "symbol": "MSFT", "longName": "Microsoft Corporation",
                "regularMarketPrice": 421.5, "chartPreviousClose": 415.0, "regularMarketVolume": 18000000 },
      "timestamp": [1750000000, 1750086400],
      "indicators": { "quote": [ { "close": [415.0, 421.5] } ] }
  } ], "error": null }
}"#;

#[tokio::test]
async fn yahoo_reads_the_price_and_computes_the_day_change() {
    let api = FakeApi::start(routes(&[("/v8/finance/chart", Reply::ok(YAHOO_CHART))])).await;
    let provider = YahooProvider::new(http()).with_base(&api.origin);

    let quotes = provider
        .quotes(&[Asset::new("MSFT", "Microsoft", AssetKind::Stock)], "EUR")
        .await
        .unwrap();

    assert_eq!(quotes.len(), 1);
    assert_eq!(quotes[0].price, d("421.5"));
    // 415 → 421.5 is +1.57 %.
    assert_eq!(quotes[0].change_percent_24h, Some(d("1.57")));
}

#[tokio::test]
async fn yahoo_reports_its_own_currency_rather_than_the_one_asked_for() {
    // The whole point: claiming these dollars are euros would silently inflate
    // the player's portfolio by whatever the exchange rate happens to be.
    let api = FakeApi::start(routes(&[("/v8/finance/chart", Reply::ok(YAHOO_CHART))])).await;
    let provider = YahooProvider::new(http()).with_base(&api.origin);

    let quotes = provider
        .quotes(&[Asset::new("MSFT", "Microsoft", AssetKind::Stock)], "EUR")
        .await
        .unwrap();

    assert_eq!(quotes[0].currency, "USD");
}

#[tokio::test]
async fn yahoo_history_pairs_timestamps_with_closes() {
    let api = FakeApi::start(routes(&[("/v8/finance/chart", Reply::ok(YAHOO_CHART))])).await;
    let provider = YahooProvider::new(http()).with_base(&api.origin);

    let points = provider
        .history(
            &Asset::new("MSFT", "Microsoft", AssetKind::Stock),
            30,
            "EUR",
        )
        .await
        .unwrap();

    assert_eq!(points.len(), 2);
    assert_eq!(points[0].price, d("415.0"));
}

#[tokio::test]
async fn yahoo_skips_a_non_trading_day_instead_of_pricing_it_at_zero() {
    let body = r#"{"chart":{"result":[{"meta":{"currency":"USD","regularMarketPrice":10},
        "timestamp":[1750000000,1750086400,1750172800],
        "indicators":{"quote":[{"close":[9.5,null,10.0]}]}}]}}"#;
    let api = FakeApi::start(routes(&[("/v8/finance/chart", Reply::ok(body))])).await;
    let provider = YahooProvider::new(http()).with_base(&api.origin);

    let points = provider
        .history(&Asset::new("X", "X", AssetKind::Stock), 30, "EUR")
        .await
        .unwrap();

    assert_eq!(points.len(), 2, "un jour férié n'est pas un cours à zéro");
}

#[tokio::test]
async fn yahoo_search_keeps_only_shares_and_trackers() {
    let body = r#"{"quotes":[
        {"symbol":"MSFT","quoteType":"EQUITY","longname":"Microsoft"},
        {"symbol":"SPY","quoteType":"ETF","shortname":"SPDR S&P 500"},
        {"symbol":"BTC-USD","quoteType":"CRYPTOCURRENCY","shortname":"Bitcoin"}
    ]}"#;
    let api = FakeApi::start(routes(&[("/v1/finance/search", Reply::ok(body))])).await;
    let provider = YahooProvider::new(http()).with_base(&api.origin);

    let found = provider.search("micro", None).await.unwrap();

    assert_eq!(found.len(), 2);
    assert!(found.iter().all(|a| a.kind.is_equity()));
}

// --------------------------------------------------------- CoinMarketCap

#[tokio::test]
async fn coinmarketcap_handles_both_the_array_and_object_payload_shapes() {
    let body = r#"{"data":{
        "BTC":[{"name":"Bitcoin","quote":{"EUR":{"price":61000.5,"percent_change_24h":-1.2,"market_cap":1.1e12,"volume_24h":2.0e10}}}],
        "ETH":{"name":"Ethereum","quote":{"EUR":{"price":2900.0,"percent_change_24h":3.4}}}
    }}"#;
    let api = FakeApi::start(routes(&[("/v2/cryptocurrency", Reply::ok(body))])).await;
    let provider = CoinMarketCapProvider::new(http(), Some("key".into())).with_base(&api.origin);

    let assets = vec![
        Asset::new("BTC", "Bitcoin", AssetKind::Crypto),
        Asset::new("ETH", "Ethereum", AssetKind::Crypto),
    ];
    let quotes = provider.quotes(&assets, "EUR").await.unwrap();

    assert_eq!(quotes.len(), 2);
    assert_eq!(
        quotes.iter().find(|q| q.symbol == "BTC").unwrap().price,
        d("61000.5")
    );
    assert_eq!(
        quotes.iter().find(|q| q.symbol == "ETH").unwrap().price,
        d("2900.0")
    );
}

#[tokio::test]
async fn coinmarketcap_without_a_key_says_so_before_making_a_request() {
    let provider = CoinMarketCapProvider::new(http(), None);
    assert!(!provider.is_configured());

    let error = provider
        .quotes(&[Asset::new("BTC", "Bitcoin", AssetKind::Crypto)], "EUR")
        .await
        .unwrap_err();
    assert!(
        !error.is_transient(),
        "une clé manquante ne se répare pas en réessayant"
    );
}

// ---------------------------------------------------------------- Finnhub

#[tokio::test]
async fn finnhub_reads_a_quote() {
    let api = FakeApi::start(routes(&[(
        "/quote",
        Reply::ok(r#"{"c":210.55,"d":2.1,"dp":1.0084,"pc":208.45}"#),
    )]))
    .await;
    let provider = FinnhubProvider::new(http(), Some("key".into())).with_base(&api.origin);

    let quotes = provider
        .quotes(&[Asset::new("AAPL", "Apple", AssetKind::Stock)], "EUR")
        .await
        .unwrap();

    assert_eq!(quotes.len(), 1);
    assert_eq!(quotes[0].price, d("210.55"));
    assert_eq!(quotes[0].change_percent_24h, Some(d("1.01")));
    assert_eq!(quotes[0].currency, "USD");
}

#[tokio::test]
async fn finnhub_treats_an_all_zero_answer_as_unknown_not_as_worthless() {
    let api = FakeApi::start(routes(&[(
        "/quote",
        Reply::ok(r#"{"c":0,"d":null,"dp":null,"pc":0}"#),
    )]))
    .await;
    let provider = FinnhubProvider::new(http(), Some("key".into())).with_base(&api.origin);

    let quotes = provider
        .quotes(&[Asset::new("NOPE", "Inconnu", AssetKind::Stock)], "EUR")
        .await
        .unwrap();

    assert!(quotes.is_empty(), "un cours à zéro n'est pas un cours");
}

// ------------------------------------------------------------ HTTP rules

#[tokio::test]
async fn a_response_that_is_not_json_is_reported_as_malformed_not_as_a_price() {
    let api = FakeApi::start(routes(&[(
        "/simple/price",
        Reply::ok("<html>Nous sommes en maintenance</html>"),
    )]))
    .await;
    let provider = CoinGeckoProvider::new(http(), None).with_base(&api.origin);

    let error = provider
        .quotes(&[Asset::new("BTC", "Bitcoin", AssetKind::Crypto)], "EUR")
        .await
        .unwrap_err();

    assert!(!error.is_transient());
    assert!(error.to_string().contains("illisible"));
}

#[tokio::test]
async fn a_plain_http_url_outside_the_loopback_is_refused() {
    let provider = CoinGeckoProvider::new(http(), None).with_base("http://example.com/api");

    let error = provider
        .quotes(&[Asset::new("BTC", "Bitcoin", AssetKind::Crypto)], "EUR")
        .await
        .unwrap_err();

    assert!(error.to_string().contains("HTTPS"));
}

#[tokio::test]
async fn an_error_message_never_carries_the_url_that_held_the_api_key() {
    // The URL contains `token=super-secret`; a leaked message would put it in
    // a log file or a UI toast.
    let provider = FinnhubProvider::new(http(), Some("super-secret".into()))
        .with_base("https://127.0.0.1:1/api");

    let error = provider
        .quotes(&[Asset::new("AAPL", "Apple", AssetKind::Stock)], "EUR")
        .await
        .unwrap_err();

    assert!(
        !error.to_string().contains("super-secret"),
        "clé fuitée : {error}"
    );
}
