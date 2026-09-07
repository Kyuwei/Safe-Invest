//! CoinGecko — the default crypto source, usable with no key at all.

use crate::error::{ProviderError, ProviderResult};
use crate::http::HttpClient;
use crate::providers::{PricePoint, QuoteProvider, decimal_from_json};
use crate::ratelimit::TokenBucket;
use async_trait::async_trait;
use jiff::Timestamp;
use safe_invest_core::model::{Asset, AssetKind, Quote};
use serde_json::Value;

pub const ID: &str = "coingecko";

const BASE: &str = "https://api.coingecko.com/api/v3";

/// The public tier tolerates roughly 5–15 calls a minute; a free demo key
/// raises that to about 30. Both stay well inside their band here.
const KEYLESS_PER_MINUTE: u32 = 5;
const KEYED_PER_MINUTE: u32 = 30;

#[derive(Debug)]
pub struct CoinGeckoProvider {
    http: HttpClient,
    api_key: Option<String>,
    limiter: TokenBucket,
    base: String,
}

impl CoinGeckoProvider {
    pub fn new(http: HttpClient, api_key: Option<String>) -> Self {
        let per_minute = if api_key.is_some() {
            KEYED_PER_MINUTE
        } else {
            KEYLESS_PER_MINUTE
        };
        Self {
            http,
            api_key,
            limiter: TokenBucket::per_minute(per_minute),
            base: BASE.to_owned(),
        }
    }

    /// Points the provider at another origin. The tests use it to serve
    /// recorded payloads from localhost, so the JSON parsing is covered
    /// without ever touching the real API.
    #[must_use]
    pub fn with_base(mut self, base: impl Into<String>) -> Self {
        self.base = base.into();
        self
    }

    fn headers(&self) -> Vec<(&str, &str)> {
        match &self.api_key {
            Some(key) => vec![("x-cg-demo-api-key", key.as_str())],
            None => Vec::new(),
        }
    }

    async fn budget(&self) -> ProviderResult<()> {
        self.limiter
            .try_take()
            .await
            .map_err(|_| ProviderError::RateLimited { provider: ID })
    }

    /// CoinGecko is addressed by slug (`bitcoin`), not ticker. The catalogue
    /// carries the slug; anything outside it falls back to a lowercased symbol,
    /// which is right often enough to be worth trying.
    fn slug_of(asset: &Asset) -> String {
        asset
            .provider_id
            .clone()
            .or_else(|| crate::catalog::lookup(AssetKind::Crypto, &asset.symbol)?.provider_id)
            .unwrap_or_else(|| asset.symbol.to_lowercase())
    }
}

#[async_trait]
impl QuoteProvider for CoinGeckoProvider {
    fn id(&self) -> &'static str {
        ID
    }

    fn label(&self) -> &'static str {
        "CoinGecko"
    }

    fn supports(&self, kind: AssetKind) -> bool {
        kind == AssetKind::Crypto
    }

    async fn quotes(&self, assets: &[Asset], currency: &str) -> ProviderResult<Vec<Quote>> {
        let crypto: Vec<&Asset> = assets
            .iter()
            .filter(|a| a.kind == AssetKind::Crypto)
            .collect();
        if crypto.is_empty() {
            return Ok(Vec::new());
        }
        self.budget().await?;

        let slugs: Vec<String> = crypto.iter().map(|a| Self::slug_of(a)).collect();
        let vs = currency.to_lowercase();
        let base = &self.base;
        let url = format!(
            "{base}/simple/price?ids={}&vs_currencies={vs}&include_24hr_change=true&include_market_cap=true&include_24hr_vol=true",
            slugs.join(",")
        );

        let body: Value = self.http.get_json(ID, &url, &self.headers()).await?;
        let now = Timestamp::now();

        let quotes = crypto
            .iter()
            .zip(&slugs)
            .filter_map(|(asset, slug)| {
                let entry = body.get(slug)?;
                Some(Quote {
                    symbol: asset.symbol.clone(),
                    kind: AssetKind::Crypto,
                    price: decimal_from_json(entry.get(&vs)?)?,
                    currency: currency.to_uppercase(),
                    as_of: now,
                    source_id: ID.to_owned(),
                    is_simulated: false,
                    name: Some(asset.name.clone()),
                    change_percent_24h: entry
                        .get(format!("{vs}_24h_change"))
                        .and_then(decimal_from_json)
                        .map(|d| d.round_dp(2)),
                    market_cap: entry
                        .get(format!("{vs}_market_cap"))
                        .and_then(decimal_from_json),
                    volume_24h: entry
                        .get(format!("{vs}_24h_vol"))
                        .and_then(decimal_from_json),
                })
            })
            .collect();

        Ok(quotes)
    }

    async fn search(&self, query: &str, kind: Option<AssetKind>) -> ProviderResult<Vec<Asset>> {
        if kind.is_some_and(|k| k != AssetKind::Crypto) || query.trim().is_empty() {
            return Ok(Vec::new());
        }
        self.budget().await?;

        let url = format!("{}/search?query={}", self.base, urlencode(query.trim()));
        let body: Value = self.http.get_json(ID, &url, &self.headers()).await?;

        let coins = body.get("coins").and_then(Value::as_array).ok_or_else(|| {
            ProviderError::Malformed {
                provider: ID,
                detail: "champ « coins » absent".into(),
            }
        })?;

        Ok(coins
            .iter()
            .take(25)
            .filter_map(|coin| {
                Some(Asset {
                    symbol: Asset::normalize(coin.get("symbol")?.as_str()?),
                    name: coin.get("name")?.as_str()?.to_owned(),
                    kind: AssetKind::Crypto,
                    provider_id: coin
                        .get("id")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    logo_url: coin
                        .get("thumb")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                })
            })
            .collect())
    }

    async fn history(
        &self,
        asset: &Asset,
        days: u16,
        currency: &str,
    ) -> ProviderResult<Vec<PricePoint>> {
        if asset.kind != AssetKind::Crypto {
            return Ok(Vec::new());
        }
        self.budget().await?;

        let url = format!(
            "{}/coins/{}/market_chart?vs_currency={}&days={days}&interval=daily",
            self.base,
            urlencode(&Self::slug_of(asset)),
            currency.to_lowercase()
        );
        let body: Value = self.http.get_json(ID, &url, &self.headers()).await?;

        let prices = body
            .get("prices")
            .and_then(Value::as_array)
            .ok_or_else(|| ProviderError::Malformed {
                provider: ID,
                detail: "champ « prices » absent".into(),
            })?;

        Ok(prices
            .iter()
            .filter_map(|point| {
                let pair = point.as_array()?;
                let millis = pair.first()?.as_i64()?;
                Some(PricePoint {
                    at: Timestamp::from_millisecond(millis).ok()?,
                    price: decimal_from_json(pair.get(1)?)?,
                })
            })
            .collect())
    }
}

/// Percent-encodes the handful of characters a symbol or search term could
/// legitimately contain. Small enough not to justify a dependency.
pub(crate) fn urlencode(value: &str) -> String {
    use std::fmt::Write as _;

    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(char::from(byte));
            }
            _ => {
                let _ = write!(out, "%{byte:02X}");
            }
        }
    }
    out
}
