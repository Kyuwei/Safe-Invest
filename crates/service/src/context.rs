//! What every operation needs: the save files, the market, the settings.

use crate::error::ServiceResult;
use safe_invest_core::paths::Paths;
use safe_invest_core::settings::{AppSettings, SettingsService};
use safe_invest_core::store::GameStore;
use safe_invest_market::MarketDataService;
use std::sync::Arc;
use tokio::sync::RwLock;

/// How to build a [`Context`].
#[derive(Debug, Clone, Default)]
pub struct ContextConfig {
    /// Overrides the data directory. `None` uses the standard per-user path.
    pub data_dir: Option<std::path::PathBuf>,
    /// Forces the simulator whatever the settings say — the `--demo` flag and
    /// what CI runs, so a test never depends on a live API.
    pub force_simulated: bool,
}

/// The shared runtime state.
///
/// Cheap to clone: everything inside is behind an `Arc`, so the window's timer,
/// its command handlers and the MCP server all share one market cache and one
/// view of the store.
#[derive(Debug, Clone)]
pub struct Context {
    store: GameStore,
    settings: SettingsService,
    market: Arc<RwLock<Arc<MarketDataService>>>,
    force_simulated: bool,
}

impl Context {
    pub fn new(config: &ContextConfig) -> ServiceResult<Self> {
        let paths = config
            .data_dir
            .clone()
            .map_or_else(Paths::discover, Paths::at);

        let store = GameStore::new(paths.clone())
            .map_err(|e| crate::ServiceError::Storage(e.to_string()))?;
        let settings = SettingsService::new(paths);

        let market = build_market(&settings, config.force_simulated)?;

        Ok(Self {
            store,
            settings,
            market: Arc::new(RwLock::new(Arc::new(market))),
            force_simulated: config.force_simulated,
        })
    }

    pub fn store(&self) -> &GameStore {
        &self.store
    }

    pub fn settings_service(&self) -> &SettingsService {
        &self.settings
    }

    pub fn settings(&self) -> AppSettings {
        let mut settings = self.settings.load();
        if self.force_simulated {
            settings.force_simulated_mode = true;
        }
        settings
    }

    /// The market service as it is configured right now.
    pub async fn market(&self) -> Arc<MarketDataService> {
        Arc::clone(&*self.market.read().await)
    }

    /// Rebuilds the provider chain after the settings changed.
    ///
    /// Swapping the whole service rather than mutating it keeps every in-flight
    /// request working against the configuration it started with, instead of
    /// half-applying a new key mid-refresh.
    pub async fn reload_market(&self) -> ServiceResult<()> {
        let rebuilt = build_market(&self.settings, self.force_simulated)?;
        *self.market.write().await = Arc::new(rebuilt);
        Ok(())
    }
}

fn build_market(
    settings: &SettingsService,
    force_simulated: bool,
) -> ServiceResult<MarketDataService> {
    let mut config = settings.load();
    if force_simulated {
        config.force_simulated_mode = true;
    }
    MarketDataService::new(&config, settings)
        .map_err(|e| crate::ServiceError::Storage(format!("client HTTP indisponible : {e}")))
}
