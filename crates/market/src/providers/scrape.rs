//! The fallback of last resort before the simulator: read the price off a
//! public web page.
//!
//! This is the "partie web" of the brief. It exists for the afternoon when an
//! API is down or a free quota has run out, and it is deliberately the
//! second-to-last link in the chain: page markup changes without warning, so a
//! scraped price is a stopgap, never a foundation. Quotes it produces still
//! carry `source_id = "scraper"`, which the interface shows.

use crate::error::{ProviderError, ProviderResult};
use crate::http::HttpClient;
use crate::providers::coingecko::urlencode;
use crate::providers::collect_quotes;
use crate::ratelimit::TokenBucket;
use async_trait::async_trait;
use jiff::Timestamp;
use rust_decimal::Decimal;
use safe_invest_core::model::{Asset, AssetKind, Quote};
use scraper::{Html, Selector};
use std::str::FromStr;

pub const ID: &str = "scraper";

/// Scraping is the polite-guest path: one page a second at most.
const PER_MINUTE: u32 = 30;

/// Where to look, and what to read once there.
struct Recipe {
    url: String,
    /// Tried in order; the first that yields a parsable number wins.
    selectors: &'static [&'static str],
    currency: &'static str,
}

#[derive(Debug)]
pub struct ScrapeProvider {
    http: HttpClient,
    limiter: TokenBucket,
}

impl ScrapeProvider {
    pub fn new(http: HttpClient) -> Self {
        Self {
            http,
            limiter: TokenBucket::per_minute(PER_MINUTE),
        }
    }

    fn recipe_for(asset: &Asset) -> Option<Recipe> {
        match asset.kind {
            AssetKind::Crypto => {
                let slug = asset.provider_id.clone().or_else(|| {
                    crate::catalog::lookup(AssetKind::Crypto, &asset.symbol)?.provider_id
                })?;
                Some(Recipe {
                    url: format!("https://coinmarketcap.com/currencies/{}/", urlencode(&slug)),
                    selectors: &[
                        r#"[data-test="text-cdp-price-display"]"#,
                        ".priceValue",
                        r#"span[data-role="price-value"]"#,
                    ],
                    currency: "USD",
                })
            }
            AssetKind::Stock | AssetKind::Etf => {
                // stockanalysis.com wants the plain ticker, without the exchange
                // suffix Yahoo uses.
                let ticker = asset.symbol.split('.').next().unwrap_or(&asset.symbol);
                let section = if asset.kind == AssetKind::Etf {
                    "etf"
                } else {
                    "stocks"
                };
                Some(Recipe {
                    url: format!(
                        "https://stockanalysis.com/{section}/{}/",
                        urlencode(&ticker.to_lowercase())
                    ),
                    selectors: &["div.text-4xl", "div[class*='text-4xl']"],
                    currency: "USD",
                })
            }
        }
    }

    async fn budget(&self) -> ProviderResult<()> {
        self.limiter
            .try_take()
            .await
            .map_err(|_| ProviderError::RateLimited { provider: ID })
    }
}

#[async_trait]
impl crate::providers::QuoteProvider for ScrapeProvider {
    fn id(&self) -> &'static str {
        ID
    }

    fn label(&self) -> &'static str {
        "Lecture de page web (secours)"
    }

    fn supports(&self, _kind: AssetKind) -> bool {
        true
    }

    async fn quotes(&self, assets: &[Asset], _currency: &str) -> ProviderResult<Vec<Quote>> {
        // Deliberately one page at a time: scraping a public site is done as a
        // polite guest, not as a crawler.
        let fetches: Vec<_> = assets
            .iter()
            .map(|asset| async move {
                let Some(recipe) = Self::recipe_for(asset) else {
                    return Ok(None);
                };
                self.budget().await?;

                let html = self
                    .http
                    .get_text(ID, &recipe.url, &[("accept", "text/html")])
                    .await?;

                Ok(extract_price(&html, recipe.selectors).map(|price| Quote {
                    symbol: asset.symbol.clone(),
                    kind: asset.kind,
                    price,
                    currency: recipe.currency.to_owned(),
                    as_of: Timestamp::now(),
                    source_id: ID.to_owned(),
                    is_simulated: false,
                    name: Some(asset.name.clone()),
                    // A scraped page gives a price and little else that can be
                    // trusted; inventing a 24 h change would be worse than none.
                    change_percent_24h: None,
                    market_cap: None,
                    volume_24h: None,
                }))
            })
            .collect();

        collect_quotes(fetches, 1).await
    }
}

/// Pulls the first parsable price out of `html` using `selectors` in order.
pub(crate) fn extract_price(html: &str, selectors: &[&str]) -> Option<Decimal> {
    let document = Html::parse_document(html);

    for pattern in selectors {
        let Ok(selector) = Selector::parse(pattern) else {
            continue;
        };
        for element in document.select(&selector) {
            let text: String = element.text().collect();
            if let Some(price) = parse_price(&text) {
                return Some(price);
            }
        }
    }
    None
}

/// Turns "$68,432.10", "1 234,56 €" or "€1.234,56" into a `Decimal`.
///
/// Written by hand because the shapes are few and a mis-parse here shows the
/// player a wrong price — the failure mode worth being fussy about.
pub(crate) fn parse_price(text: &str) -> Option<Decimal> {
    let digits: String = text
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == ',')
        .collect();
    if digits.is_empty() {
        return None;
    }

    let last_dot = digits.rfind('.');
    let last_comma = digits.rfind(',');

    // Whichever separator comes last is the decimal point; the other groups
    // thousands. A lone separator followed by exactly three digits is a
    // thousands group ("1,234"), not a fraction.
    let normalised = match (last_dot, last_comma) {
        (Some(dot), Some(comma)) if dot > comma => digits.replace(',', ""),
        (Some(_), Some(_)) => digits.replace('.', "").replace(',', "."),
        (Some(dot), None) => {
            if digits.len().saturating_sub(dot) == 4 && digits.matches('.').count() == 1 {
                digits.replace('.', "")
            } else {
                digits.replace(',', "")
            }
        }
        (None, Some(comma)) => {
            if digits.len().saturating_sub(comma) == 4 && digits.matches(',').count() == 1 {
                digits.replace(',', "")
            } else {
                digits.replace(',', ".")
            }
        }
        (None, None) => digits,
    };

    Decimal::from_str(&normalised)
        .ok()
        .filter(|price| *price > Decimal::ZERO)
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

    #[test]
    fn reads_the_shapes_a_price_actually_comes_in() {
        assert_eq!(
            parse_price("$68,432.10").unwrap(),
            Decimal::from_str("68432.10").unwrap()
        );
        assert_eq!(
            parse_price("1 234,56 €").unwrap(),
            Decimal::from_str("1234.56").unwrap()
        );
        assert_eq!(
            parse_price("€1.234,56").unwrap(),
            Decimal::from_str("1234.56").unwrap()
        );
        assert_eq!(
            parse_price("210.55 USD").unwrap(),
            Decimal::from_str("210.55").unwrap()
        );
        assert_eq!(
            parse_price("0.000042").unwrap(),
            Decimal::from_str("0.000042").unwrap()
        );
    }

    #[test]
    fn a_lone_thousands_separator_is_not_read_as_a_fraction() {
        // "1,234" is one thousand two hundred, not one point two three four.
        assert_eq!(parse_price("$1,234").unwrap(), Decimal::from(1234));
        assert_eq!(parse_price("1.234 €").unwrap(), Decimal::from(1234));
    }

    #[test]
    fn nonsense_yields_nothing_rather_than_a_wrong_price() {
        assert!(parse_price("indisponible").is_none());
        assert!(parse_price("").is_none());
        assert!(
            parse_price("0.00").is_none(),
            "un prix nul n'est pas un prix"
        );
    }

    #[test]
    fn falls_through_the_selector_list_until_one_matches() {
        let html = r#"<html><body><span class="priceValue">$4 200,50</span></body></html>"#;
        let price = extract_price(html, &[r#"[data-test="missing"]"#, ".priceValue"]).unwrap();
        assert_eq!(price, Decimal::from_str("4200.50").unwrap());
    }

    #[test]
    fn a_page_that_changed_its_markup_yields_nothing_not_a_panic() {
        let html = "<html><body><p>Nous avons tout refait !</p></body></html>";
        assert!(extract_price(html, &[".priceValue", "div.text-4xl"]).is_none());
    }

    #[test]
    fn a_malformed_selector_is_skipped_not_fatal() {
        let html = r#"<div class="text-4xl">99.50</div>"#;
        let price = extract_price(html, &["!!not a selector!!", "div.text-4xl"]).unwrap();
        assert_eq!(price, Decimal::from_str("99.50").unwrap());
    }
}
