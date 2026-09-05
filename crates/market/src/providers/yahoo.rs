//! Yahoo Finance — shares and trackers, with no key.
//!
//! The `v8/finance/chart` endpoint is the one that still answers without a
//! crumb or a cookie. It is undocumented and can change without notice, which
//! is exactly why the chain behind it exists.

use crate::error::{ProviderError, ProviderResult};
use crate::http::HttpClient;
use crate::providers::coingecko::urlencode;
use crate::providers::{PricePoint, QuoteProvider, decimal_from_json};
use crate::ratelimit::TokenBucket;
use async_trait::async_trait;
use jiff::Timestamp;
use safe_invest_core::model::{Asset, AssetKind, Quote};
use serde_json::Value;

pub const ID: &str = "yahoo";

const CHART: &str = "https://query1.finance.yahoo.com/v8/finance/chart";
const SEARCH: &str = "https://query1.finance.yahoo.com/v1/finance/search";
const PER_MINUTE: u32 = 60;

/// Yahoo turns away callers that look like a script. This is the minimum set of
/// headers that gets a normal answer.
fn browser_headers() -> [(&'static str, &'static str); 3] {
    [
        ("accept", "application/json,text/plain,*/*"),
        ("accept-language", "fr-FR,fr;q=0.9,en;q=0.8"),
        ("referer", "https://finance.yahoo.com/"),
    ]
}

#[derive(Debug)]
pub struct YahooProvider {
    http: HttpClient,
    limiter: TokenBucket,
    chart_base: String,
    search_base: String,
}

impl YahooProvider {
    pub fn new(http: HttpClient) -> Self {
        Self {
            http,
            limiter: TokenBucket::per_minute(PER_MINUTE),
            chart_base: CHART.to_owned(),
            search_base: SEARCH.to_owned(),
        }
    }

    /// Points the provider at another origin, for tests served from localhost.
    #[must_use]
    pub fn with_base(mut self, origin: &str) -> Self {
        self.chart_base = format!("{origin}/v8/finance/chart");
        self.search_base = format!("{origin}/v1/finance/search");
        self
    }

    fn ticker_of(asset: &Asset) -> String {
        asset
            .provider_id
            .clone()
            .unwrap_or_else(|| asset.symbol.clone())
    }

    async fn budget(&self) -> ProviderResult<()> {
        self.limiter
            .try_take()
            .await
            .map_err(|_| ProviderError::RateLimited { provider: ID })
    }

    async fn chart(&self, ticker: &str, range: &str, interval: &str) -> ProviderResult<Value> {
        let url = format!(
            "{}/{}?range={range}&interval={interval}",
            self.chart_base,
            urlencode(ticker)
        );
        let body: Value = self.http.get_json(ID, &url, &browser_headers()).await?;

        body.pointer("/chart/result/0")
            .cloned()
            .ok_or_else(|| ProviderError::NotSupported {
                provider: ID,
                symbol: ticker.to_owned(),
            })
    }
}

#[async_trait]
impl QuoteProvider for YahooProvider {
    fn id(&self) -> &'static str {
        ID
    }

    fn label(&self) -> &'static str {
        "Yahoo Finance"
    }

    fn supports(&self, kind: AssetKind) -> bool {
        kind.is_equity()
    }

    async fn quotes(&self, assets: &[Asset], _currency: &str) -> ProviderResult<Vec<Quote>> {
        let mut quotes = Vec::new();

        // One request per symbol: the batch endpoint is the one that needs a
        // crumb. A dashboard holds a handful of lines, and the connection pool
        // keeps the cost to one handshake.
        for asset in assets.iter().filter(|a| a.kind.is_equity()) {
            self.budget().await?;
            let ticker = Self::ticker_of(asset);
            let result = self.chart(&ticker, "2d", "1d").await?;

            let Some(meta) = result.get("meta") else {
                continue;
            };
            let Some(price) = meta.get("regularMarketPrice").and_then(decimal_from_json) else {
                continue;
            };
            let previous = meta
                .get("chartPreviousClose")
                .or_else(|| meta.get("previousClose"))
                .and_then(decimal_from_json);

            let change_percent = previous.filter(|p| !p.is_zero()).and_then(|previous| {
                price
                    .checked_sub(previous)?
                    .checked_div(previous)?
                    .checked_mul(rust_decimal::Decimal::ONE_HUNDRED)
                    .map(|d| d.round_dp(2))
            });

            quotes.push(Quote {
                symbol: asset.symbol.clone(),
                kind: asset.kind,
                price,
                // Yahoo quotes in the listing's own currency. The service
                // converts; claiming the requested currency here would be a lie
                // that silently multiplies the player's portfolio.
                currency: meta
                    .get("currency")
                    .and_then(Value::as_str)
                    .unwrap_or("USD")
                    .to_uppercase(),
                as_of: Timestamp::now(),
                source_id: ID.to_owned(),
                is_simulated: false,
                name: meta
                    .get("longName")
                    .or_else(|| meta.get("shortName"))
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned)
                    .or_else(|| Some(asset.name.clone())),
                change_percent_24h: change_percent,
                market_cap: None,
                volume_24h: meta.get("regularMarketVolume").and_then(decimal_from_json),
            });
        }

        Ok(quotes)
    }

    async fn search(&self, query: &str, kind: Option<AssetKind>) -> ProviderResult<Vec<Asset>> {
        if kind.is_some_and(|k| !k.is_equity()) || query.trim().is_empty() {
            return Ok(Vec::new());
        }
        self.budget().await?;

        let url = format!(
            "{}?q={}&quotesCount=20&newsCount=0",
            self.search_base,
            urlencode(query.trim())
        );
        let body: Value = self.http.get_json(ID, &url, &browser_headers()).await?;

        let quotes = body
            .get("quotes")
            .and_then(Value::as_array)
            .ok_or_else(|| ProviderError::Malformed {
                provider: ID,
                detail: "champ « quotes » absent".into(),
            })?;

        Ok(quotes
            .iter()
            .filter_map(|entry| {
                let symbol = entry.get("symbol")?.as_str()?;
                let kind = match entry.get("quoteType")?.as_str()? {
                    "ETF" | "MUTUALFUND" => AssetKind::Etf,
                    "EQUITY" => AssetKind::Stock,
                    _ => return None,
                };
                Some(Asset {
                    symbol: Asset::normalize(symbol),
                    name: entry
                        .get("longname")
                        .or_else(|| entry.get("shortname"))
                        .and_then(Value::as_str)
                        .unwrap_or(symbol)
                        .to_owned(),
                    kind,
                    provider_id: Some(symbol.to_owned()),
                    logo_url: None,
                })
            })
            .filter(|asset| kind.is_none_or(|k| k == asset.kind))
            .collect())
    }

    async fn history(
        &self,
        asset: &Asset,
        days: u16,
        _currency: &str,
    ) -> ProviderResult<Vec<PricePoint>> {
        if !asset.kind.is_equity() {
            return Ok(Vec::new());
        }
        self.budget().await?;

        let range = match days {
            0..=7 => "5d",
            8..=31 => "1mo",
            32..=93 => "3mo",
            94..=186 => "6mo",
            187..=372 => "1y",
            _ => "5y",
        };
        let result = self.chart(&Self::ticker_of(asset), range, "1d").await?;

        let stamps = result
            .get("timestamp")
            .and_then(Value::as_array)
            .ok_or_else(|| ProviderError::Malformed {
                provider: ID,
                detail: "champ « timestamp » absent".into(),
            })?;
        let closes = result
            .pointer("/indicators/quote/0/close")
            .and_then(Value::as_array)
            .ok_or_else(|| ProviderError::Malformed {
                provider: ID,
                detail: "champ « close » absent".into(),
            })?;

        Ok(stamps
            .iter()
            .zip(closes)
            // A null close is a non-trading day, not a price of zero.
            .filter_map(|(stamp, close)| {
                Some(PricePoint {
                    at: Timestamp::from_second(stamp.as_i64()?).ok()?,
                    price: decimal_from_json(close)?,
                })
            })
            .collect())
    }
}
