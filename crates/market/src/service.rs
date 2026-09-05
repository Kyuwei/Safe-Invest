//! The fall-through chain: ask each source in turn until every asset has a
//! price, and never come back empty-handed.

use crate::cache::TtlCache;
use crate::error::ProviderError;
use crate::fx::FxRates;
use crate::http::HttpClient;
use crate::providers::{
    PricePoint, QuoteProvider, coingecko::CoinGeckoProvider, coinmarketcap::CoinMarketCapProvider,
    finnhub::FinnhubProvider, scrape::ScrapeProvider, simulated::SimulatedProvider,
    yahoo::YahooProvider,
};
use jiff::Timestamp;
use safe_invest_core::model::{Asset, AssetKind, Quote};
use safe_invest_core::settings::{AppSettings, SettingsService};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How a source is currently faring, for the Settings screen's status lights.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderStatus {
    pub id: String,
    pub label: String,
    pub kinds: Vec<AssetKind>,
    pub configured: bool,
    pub is_simulated: bool,
    /// `None` until the source has been tried at least once.
    pub healthy: Option<bool>,
    pub detail: Option<String>,
    pub last_used: Option<Timestamp>,
}

/// What one round of quoting produced, and which sources produced it.
#[derive(Debug, Clone, Default)]
pub struct QuoteBatch {
    /// Keyed by [`Asset::key`].
    pub quotes: HashMap<String, Quote>,
    /// Symbols no source could price.
    pub unpriced: Vec<String>,
}

/// The knobs the chain reads, separated from where they were stored so tests
/// can build a service without a settings file.
#[derive(Debug, Clone)]
pub struct ChainOptions {
    /// Provider ids to try, per asset kind, best first.
    pub order: HashMap<AssetKind, Vec<String>>,
    /// Ignore every real source and play against the simulator.
    pub force_simulated: bool,
    pub cache_ttl: Duration,
}

impl ChainOptions {
    pub fn from_settings(settings: &AppSettings) -> Self {
        let mut order = HashMap::new();
        order.insert(AssetKind::Crypto, settings.crypto_provider_order.clone());
        order.insert(AssetKind::Stock, settings.stock_provider_order.clone());
        order.insert(AssetKind::Etf, settings.stock_provider_order.clone());

        Self {
            order,
            force_simulated: settings.force_simulated_mode,
            cache_ttl: Duration::from_secs(settings.quote_cache_seconds.max(5)),
        }
    }
}

#[derive(Debug)]
pub struct MarketDataService {
    providers: Vec<Arc<dyn QuoteProvider>>,
    order: HashMap<AssetKind, Vec<String>>,
    force_simulated: bool,
    cache: TtlCache<String, Quote>,
    fx: FxRates,
    status: Mutex<HashMap<String, ProviderStatus>>,
}

impl MarketDataService {
    /// Builds the chain described by `settings`, reading any API keys through
    /// `secrets`.
    pub fn new(settings: &AppSettings, secrets: &SettingsService) -> Result<Self, reqwest::Error> {
        let http = HttpClient::new()?;
        let key = |id: &str| secrets.api_key(settings, id);

        let providers: Vec<Arc<dyn QuoteProvider>> = vec![
            Arc::new(CoinGeckoProvider::new(http.clone(), key("coingecko"))),
            Arc::new(CoinMarketCapProvider::new(
                http.clone(),
                key("coinmarketcap"),
            )),
            Arc::new(YahooProvider::new(http.clone())),
            Arc::new(FinnhubProvider::new(http.clone(), key("finnhub"))),
            Arc::new(ScrapeProvider::new(http.clone())),
            Arc::new(SimulatedProvider::new()),
        ];

        Ok(Self::with_providers(
            providers,
            ChainOptions::from_settings(settings),
            FxRates::new(http),
        ))
    }

    /// Builds a service over an explicit provider list.
    ///
    /// This is the constructor the tests use to stand a chain up out of fakes,
    /// which is how the fall-through behaviour is covered without a network.
    pub fn with_providers(
        providers: Vec<Arc<dyn QuoteProvider>>,
        options: ChainOptions,
        fx: FxRates,
    ) -> Self {
        let status = providers
            .iter()
            .map(|p| {
                (
                    p.id().to_owned(),
                    ProviderStatus {
                        id: p.id().to_owned(),
                        label: p.label().to_owned(),
                        kinds: AssetKind::ALL
                            .into_iter()
                            .filter(|k| p.supports(*k))
                            .collect(),
                        configured: p.is_configured(),
                        is_simulated: p.is_simulated(),
                        healthy: None,
                        detail: None,
                        last_used: None,
                    },
                )
            })
            .collect();

        Self {
            providers,
            order: options.order,
            force_simulated: options.force_simulated,
            cache: TtlCache::new(options.cache_ttl),
            fx,
            status: Mutex::new(status),
        }
    }

    /// The ordered sources to try for `kind`.
    ///
    /// Unconfigured sources are dropped rather than called for a 401, and the
    /// simulator is always appended: the app must never be unable to show a
    /// number, only unable to show a *real* one.
    fn chain_for(&self, kind: AssetKind) -> Vec<Arc<dyn QuoteProvider>> {
        if self.force_simulated {
            return self
                .providers
                .iter()
                .filter(|p| p.is_simulated())
                .map(Arc::clone)
                .collect();
        }

        let preferred = self.order.get(&kind);
        let mut chain: Vec<Arc<dyn QuoteProvider>> = Vec::new();

        if let Some(ids) = preferred {
            for id in ids {
                if let Some(provider) = self
                    .providers
                    .iter()
                    .find(|p| p.id() == id && p.supports(kind) && p.is_configured())
                {
                    chain.push(Arc::clone(provider));
                }
            }
        }

        if !chain.iter().any(|p| p.is_simulated())
            && let Some(simulator) = self.providers.iter().find(|p| p.is_simulated())
        {
            chain.push(Arc::clone(simulator));
        }
        chain
    }

    /// Prices `assets` in `currency`, trying each source in turn.
    ///
    /// Never returns an error: a source that fails is recorded and skipped.
    pub async fn quotes(&self, assets: &[Asset], currency: &str) -> QuoteBatch {
        let currency = currency.to_uppercase();
        let mut batch = QuoteBatch::default();

        let mut pending: Vec<Asset> = Vec::new();
        for asset in assets {
            let key = asset.key();
            match self.cache.get(&cache_key(&key, &currency)) {
                Some(quote) => {
                    batch.quotes.insert(key, quote);
                }
                None => pending.push(asset.clone()),
            }
        }

        for kind in AssetKind::ALL {
            let mut remaining: Vec<Asset> =
                pending.iter().filter(|a| a.kind == kind).cloned().collect();
            if remaining.is_empty() {
                continue;
            }

            for provider in self.chain_for(kind) {
                if remaining.is_empty() {
                    break;
                }

                match provider.quotes(&remaining, &currency).await {
                    Ok(quotes) => {
                        self.record(provider.id(), Ok(()));
                        for mut quote in quotes {
                            // A price we cannot express in the game's currency is
                            // worse than no price: the engine would refuse it, or
                            // worse, accept a dollar figure as euros.
                            if !self.fx.convert(&mut quote, &currency).await {
                                tracing::debug!(
                                    symbol = %quote.symbol,
                                    from = %quote.currency,
                                    "conversion impossible, source suivante"
                                );
                                continue;
                            }
                            let key = quote.key();
                            self.cache.insert(cache_key(&key, &currency), quote.clone());
                            remaining.retain(|a| a.key() != key);
                            batch.quotes.insert(key, quote);
                        }
                    }
                    Err(error) => {
                        tracing::debug!(provider = provider.id(), %error, "source indisponible");
                        self.record(provider.id(), Err(&error));
                    }
                }
            }

            batch
                .unpriced
                .extend(remaining.into_iter().map(|a| a.symbol));
        }

        batch
    }

    /// Searches every source that can answer, keeping the first result for each
    /// symbol so the preferred source wins ties.
    pub async fn search(&self, query: &str, kind: Option<AssetKind>) -> Vec<Asset> {
        let mut found: Vec<Asset> = crate::catalog::search(query, kind);
        let mut seen: std::collections::HashSet<String> = found.iter().map(Asset::key).collect();

        let kinds: Vec<AssetKind> = kind.map_or_else(|| AssetKind::ALL.to_vec(), |k| vec![k]);
        for kind in kinds {
            for provider in self.chain_for(kind) {
                if provider.is_simulated() {
                    continue;
                }
                match provider.search(query, Some(kind)).await {
                    Ok(results) => {
                        self.record(provider.id(), Ok(()));
                        for asset in results {
                            if seen.insert(asset.key()) {
                                found.push(asset);
                            }
                        }
                        // One working source per kind is enough for a search box.
                        break;
                    }
                    Err(error) => self.record(provider.id(), Err(&error)),
                }
            }
        }

        found
    }

    /// Daily closes for a sparkline. Falls through the chain like quoting does.
    pub async fn history(&self, asset: &Asset, days: u16, currency: &str) -> Vec<PricePoint> {
        for provider in self.chain_for(asset.kind) {
            match provider.history(asset, days, currency).await {
                Ok(points) if !points.is_empty() => {
                    self.record(provider.id(), Ok(()));
                    return points;
                }
                Ok(_) => {}
                Err(error) => self.record(provider.id(), Err(&error)),
            }
        }
        Vec::new()
    }

    /// The status lights shown in Settings, in chain order.
    pub fn statuses(&self) -> Vec<ProviderStatus> {
        let Ok(status) = self.status.lock() else {
            return Vec::new();
        };
        self.providers
            .iter()
            .filter_map(|p| status.get(p.id()).cloned())
            .collect()
    }

    /// The exchange-rate table, so a caller can pre-seed a known rate.
    pub fn fx(&self) -> &FxRates {
        &self.fx
    }

    /// Drops every cached quote, so the next refresh really goes out.
    pub fn invalidate(&self) {
        self.cache.clear();
    }

    fn record(&self, id: &str, outcome: Result<(), &ProviderError>) {
        let Ok(mut status) = self.status.lock() else {
            return;
        };
        let Some(entry) = status.get_mut(id) else {
            return;
        };

        entry.last_used = Some(Timestamp::now());
        match outcome {
            Ok(()) => {
                entry.healthy = Some(true);
                entry.detail = None;
            }
            Err(error) => {
                entry.healthy = Some(false);
                entry.detail = Some(error.to_string());
            }
        }
    }
}

fn cache_key(asset_key: &str, currency: &str) -> String {
    format!("{asset_key}@{currency}")
}
