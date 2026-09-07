//! CoinMarketCap — crypto, with a free key (about 15 000 credits a month).

use crate::error::{ProviderError, ProviderResult};
use crate::http::HttpClient;
use crate::providers::coingecko::urlencode;
use crate::providers::{QuoteProvider, decimal_from_json};
use crate::ratelimit::TokenBucket;
use async_trait::async_trait;
use jiff::Timestamp;
use safe_invest_core::model::{Asset, AssetKind, Quote};
use serde_json::Value;

pub const ID: &str = "coinmarketcap";

const BASE: &str = "https://pro-api.coinmarketcap.com";
const PER_MINUTE: u32 = 20;

#[derive(Debug)]
pub struct CoinMarketCapProvider {
    http: HttpClient,
    api_key: Option<String>,
    limiter: TokenBucket,
    base: String,
}

impl CoinMarketCapProvider {
    pub fn new(http: HttpClient, api_key: Option<String>) -> Self {
        Self {
            http,
            api_key,
            limiter: TokenBucket::per_minute(PER_MINUTE),
            base: BASE.to_owned(),
        }
    }

    /// Points the provider at another origin, for tests served from localhost.
    #[must_use]
    pub fn with_base(mut self, base: impl Into<String>) -> Self {
        self.base = base.into();
        self
    }

    fn key(&self) -> ProviderResult<&str> {
        self.api_key
            .as_deref()
            .ok_or(ProviderError::MissingKey { provider: ID })
    }

    async fn budget(&self) -> ProviderResult<()> {
        self.limiter
            .try_take()
            .await
            .map_err(|_| ProviderError::RateLimited { provider: ID })
    }
}

#[async_trait]
impl QuoteProvider for CoinMarketCapProvider {
    fn id(&self) -> &'static str {
        ID
    }

    fn label(&self) -> &'static str {
        "CoinMarketCap"
    }

    fn supports(&self, kind: AssetKind) -> bool {
        kind == AssetKind::Crypto
    }

    fn is_configured(&self) -> bool {
        self.api_key.is_some()
    }

    async fn quotes(&self, assets: &[Asset], currency: &str) -> ProviderResult<Vec<Quote>> {
        let key = self.key()?;
        let crypto: Vec<&Asset> = assets
            .iter()
            .filter(|a| a.kind == AssetKind::Crypto)
            .collect();
        if crypto.is_empty() {
            return Ok(Vec::new());
        }
        self.budget().await?;

        let symbols: Vec<String> = crypto.iter().map(|a| a.symbol.clone()).collect();
        let convert = currency.to_uppercase();
        let url = format!(
            "{}/v2/cryptocurrency/quotes/latest?symbol={}&convert={convert}",
            self.base,
            urlencode(&symbols.join(","))
        );

        let body: Value = self
            .http
            .get_json(
                ID,
                &url,
                &[("X-CMC_PRO_API_KEY", key), ("Accept", "application/json")],
            )
            .await?;

        let data = body.get("data").ok_or_else(|| ProviderError::Malformed {
            provider: ID,
            detail: "champ « data » absent".into(),
        })?;
        let now = Timestamp::now();

        Ok(crypto
            .iter()
            .filter_map(|asset| {
                // v2 returns an array per symbol (several coins can share a
                // ticker); v1 returned a bare object. Accept both.
                let entry = match data.get(&asset.symbol) {
                    Some(Value::Array(list)) => list.first()?,
                    Some(object @ Value::Object(_)) => object,
                    _ => return None,
                };
                let quote = entry.pointer(&format!("/quote/{convert}"))?;

                Some(Quote {
                    symbol: asset.symbol.clone(),
                    kind: AssetKind::Crypto,
                    price: decimal_from_json(quote.get("price")?)?,
                    currency: convert.clone(),
                    as_of: now,
                    source_id: ID.to_owned(),
                    is_simulated: false,
                    name: entry
                        .get("name")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned)
                        .or_else(|| Some(asset.name.clone())),
                    change_percent_24h: quote
                        .get("percent_change_24h")
                        .and_then(decimal_from_json)
                        .map(|d| d.round_dp(2)),
                    market_cap: quote.get("market_cap").and_then(decimal_from_json),
                    volume_24h: quote.get("volume_24h").and_then(decimal_from_json),
                })
            })
            .collect())
    }

    async fn search(&self, query: &str, kind: Option<AssetKind>) -> ProviderResult<Vec<Asset>> {
        if kind.is_some_and(|k| k != AssetKind::Crypto) || query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let key = self.key()?;
        self.budget().await?;

        let url = format!(
            "{}/v1/cryptocurrency/map?symbol={}&limit=20",
            self.base,
            urlencode(query.trim())
        );
        let body: Value = self
            .http
            .get_json(
                ID,
                &url,
                &[("X-CMC_PRO_API_KEY", key), ("Accept", "application/json")],
            )
            .await?;

        let Some(list) = body.get("data").and_then(Value::as_array) else {
            return Ok(Vec::new());
        };

        Ok(list
            .iter()
            .filter_map(|entry| {
                Some(Asset {
                    symbol: Asset::normalize(entry.get("symbol")?.as_str()?),
                    name: entry.get("name")?.as_str()?.to_owned(),
                    kind: AssetKind::Crypto,
                    provider_id: entry
                        .get("slug")
                        .and_then(Value::as_str)
                        .map(ToOwned::to_owned),
                    logo_url: None,
                })
            })
            .collect())
    }

    // History is a paid endpoint on CoinMarketCap. Returning nothing lets the
    // chain fall through to a source that can answer, rather than failing.
}
