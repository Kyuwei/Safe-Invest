//! Finnhub — shares and trackers, with a free key (60 calls a minute).

use crate::error::{ProviderError, ProviderResult};
use crate::http::HttpClient;
use crate::providers::coingecko::urlencode;
use crate::providers::{QuoteProvider, collect_quotes, decimal_from_json};
use crate::ratelimit::TokenBucket;
use async_trait::async_trait;
use jiff::Timestamp;
use rust_decimal::Decimal;
use safe_invest_core::model::{Asset, AssetKind, Quote};
use serde_json::Value;

pub const ID: &str = "finnhub";

const BASE: &str = "https://finnhub.io/api/v1";
const PER_MINUTE: u32 = 55;
const CONCURRENCY: usize = 6;

#[derive(Debug)]
pub struct FinnhubProvider {
    http: HttpClient,
    api_key: Option<String>,
    limiter: TokenBucket,
    base: String,
}

impl FinnhubProvider {
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

    /// Finnhub answers `{"c":0,"pc":0,...}` for an unknown ticker rather than a
    /// 404. A zero price means "not found", never "worthless".
    fn quote_from(asset: &Asset, body: &Value) -> Option<Quote> {
        let price = body.get("c").and_then(decimal_from_json)?;
        if price <= Decimal::ZERO {
            return None;
        }

        Some(Quote {
            symbol: asset.symbol.clone(),
            kind: asset.kind,
            price,
            // Finnhub's free tier quotes US listings, in dollars.
            currency: "USD".to_owned(),
            as_of: Timestamp::now(),
            source_id: ID.to_owned(),
            is_simulated: false,
            name: Some(asset.name.clone()),
            change_percent_24h: body
                .get("dp")
                .and_then(decimal_from_json)
                .map(|d| d.round_dp(2)),
            market_cap: None,
            volume_24h: None,
        })
    }
}

#[async_trait]
impl QuoteProvider for FinnhubProvider {
    fn id(&self) -> &'static str {
        ID
    }

    fn label(&self) -> &'static str {
        "Finnhub"
    }

    fn supports(&self, kind: AssetKind) -> bool {
        kind.is_equity()
    }

    fn is_configured(&self) -> bool {
        self.api_key.is_some()
    }

    async fn quotes(&self, assets: &[Asset], _currency: &str) -> ProviderResult<Vec<Quote>> {
        let key = self.key()?;

        let fetches: Vec<_> = assets
            .iter()
            .filter(|asset| asset.kind.is_equity())
            .map(|asset| async move {
                self.budget().await?;
                let symbol = asset
                    .provider_id
                    .clone()
                    .unwrap_or_else(|| asset.symbol.clone());
                let url = format!(
                    "{}/quote?symbol={}&token={}",
                    self.base,
                    urlencode(&symbol),
                    urlencode(key)
                );
                let body: Value = self.http.get_json(ID, &url, &[]).await?;
                Ok(Self::quote_from(asset, &body))
            })
            .collect();

        collect_quotes(fetches, CONCURRENCY).await
    }

    async fn search(&self, query: &str, kind: Option<AssetKind>) -> ProviderResult<Vec<Asset>> {
        if kind.is_some_and(|k| !k.is_equity()) || query.trim().is_empty() {
            return Ok(Vec::new());
        }
        let key = self.key()?;
        self.budget().await?;

        let url = format!(
            "{}/search?q={}&token={}",
            self.base,
            urlencode(query.trim()),
            urlencode(key)
        );
        let body: Value = self.http.get_json(ID, &url, &[]).await?;

        let Some(list) = body.get("result").and_then(Value::as_array) else {
            return Ok(Vec::new());
        };

        Ok(list
            .iter()
            .take(20)
            .filter_map(|entry| {
                let symbol = entry.get("symbol")?.as_str()?;
                Some(Asset {
                    symbol: Asset::normalize(symbol),
                    name: entry.get("description")?.as_str()?.to_owned(),
                    kind: match entry.get("type").and_then(Value::as_str) {
                        Some("ETP" | "ETF") => AssetKind::Etf,
                        _ => AssetKind::Stock,
                    },
                    provider_id: Some(symbol.to_owned()),
                    logo_url: None,
                })
            })
            .collect())
    }
}
