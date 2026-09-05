//! User preferences and API keys.

use crate::paths::Paths;
use crate::secret::{self, Sealed};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Defaults chosen so the app is fully usable with no key and no configuration:
/// keyless sources first, the simulator last, fees off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub crypto_provider_order: Vec<String>,
    pub stock_provider_order: Vec<String>,
    /// Provider id → sealed key. Never contains a plaintext key on Windows.
    pub protected_api_keys: BTreeMap<String, String>,
    pub quote_cache_seconds: u64,
    pub refresh_interval_seconds: u64,
    pub default_currency: String,
    pub default_fee_percent: Decimal,
    pub default_starting_cash: Decimal,
    /// Force the simulator everywhere — demo mode, and what CI runs.
    pub force_simulated_mode: bool,
    /// Blue/orange instead of green/red.
    pub colour_blind_palette: bool,
    pub theme: String,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            crypto_provider_order: vec![
                "coingecko".into(),
                "coinmarketcap".into(),
                "scraper".into(),
                "simulated".into(),
            ],
            stock_provider_order: vec![
                "yahoo".into(),
                "finnhub".into(),
                "scraper".into(),
                "simulated".into(),
            ],
            protected_api_keys: BTreeMap::new(),
            quote_cache_seconds: 60,
            refresh_interval_seconds: 60,
            default_currency: "EUR".into(),
            default_fee_percent: Decimal::ZERO,
            default_starting_cash: Decimal::from(10_000),
            force_simulated_mode: false,
            colour_blind_palette: false,
            theme: "system".into(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("erreur disque : {0}")]
    Io(#[from] std::io::Error),
    #[error("réglages illisibles : {0}")]
    Parse(#[from] serde_json::Error),
    #[error(transparent)]
    Secret(#[from] secret::SecretError),
}

#[derive(Debug, Clone)]
pub struct SettingsService {
    paths: Paths,
}

impl SettingsService {
    pub fn new(paths: Paths) -> Self {
        Self { paths }
    }

    /// A missing or unreadable settings file yields the defaults: a corrupted
    /// preference must never stop the app from opening.
    pub fn load(&self) -> AppSettings {
        let path = self.paths.settings_file();
        match std::fs::read_to_string(&path) {
            Ok(text) => serde_json::from_str(&text).unwrap_or_else(|error| {
                tracing::warn!(%error, path = %path.display(), "réglages illisibles, valeurs par défaut appliquées");
                AppSettings::default()
            }),
            Err(_) => AppSettings::default(),
        }
    }

    pub fn save(&self, settings: &AppSettings) -> Result<(), SettingsError> {
        self.paths.ensure_created()?;
        let bytes = serde_json::to_vec_pretty(settings)?;
        std::fs::write(self.paths.settings_file(), bytes)?;
        Ok(())
    }

    /// Stores a key for `provider_id`, sealed. An empty value clears it.
    pub fn set_api_key(&self, provider_id: &str, key: &str) -> Result<(), SettingsError> {
        let mut settings = self.load();
        let trimmed = key.trim();
        if trimmed.is_empty() {
            settings.protected_api_keys.remove(provider_id);
        } else {
            settings.protected_api_keys.insert(
                provider_id.to_owned(),
                secret::seal(trimmed)?.as_str().to_owned(),
            );
        }
        self.save(&settings)
    }

    /// The key for `provider_id`, from the settings file or from the
    /// environment. The environment wins nothing — it is only consulted when
    /// nothing is stored — so a CI variable cannot quietly shadow a key the
    /// user typed in.
    pub fn api_key(&self, settings: &AppSettings, provider_id: &str) -> Option<String> {
        if let Some(stored) = settings.protected_api_keys.get(provider_id) {
            match secret::unseal(&Sealed::from_stored(stored.clone())) {
                Ok(key) => return Some(key),
                Err(error) => {
                    tracing::warn!(provider = provider_id, %error, "clé API illisible");
                }
            }
        }
        std::env::var(env_var_for(provider_id))
            .ok()
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
    }
}

/// `coinmarketcap` → `SAFEINVEST_COINMARKETCAP_KEY`.
pub fn env_var_for(provider_id: &str) -> String {
    format!(
        "SAFEINVEST_{}_KEY",
        provider_id.to_uppercase().replace('-', "_")
    )
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "a failed unwrap is a failed test")]
mod tests {
    use super::*;

    #[test]
    fn defaults_put_keyless_sources_first_and_the_simulator_last() {
        let settings = AppSettings::default();
        assert_eq!(settings.crypto_provider_order.first().unwrap(), "coingecko");
        assert_eq!(settings.crypto_provider_order.last().unwrap(), "simulated");
        assert_eq!(settings.stock_provider_order.first().unwrap(), "yahoo");
    }

    #[test]
    fn api_keys_round_trip_and_are_never_stored_in_the_clear_form() {
        let dir = tempfile::tempdir().unwrap();
        let service = SettingsService::new(Paths::at(dir.path()));
        service
            .set_api_key("coinmarketcap", "  super-secret  ")
            .unwrap();

        let settings = service.load();
        let stored = settings.protected_api_keys.get("coinmarketcap").unwrap();
        assert!(!stored.contains("super-secret"));
        assert_eq!(
            service.api_key(&settings, "coinmarketcap").unwrap(),
            "super-secret"
        );
    }

    #[test]
    fn an_empty_key_clears_the_entry() {
        let dir = tempfile::tempdir().unwrap();
        let service = SettingsService::new(Paths::at(dir.path()));
        service.set_api_key("finnhub", "abc").unwrap();
        service.set_api_key("finnhub", "   ").unwrap();
        assert!(service.load().protected_api_keys.is_empty());
    }

    #[test]
    fn env_var_names_match_the_documented_ones() {
        assert_eq!(env_var_for("coinmarketcap"), "SAFEINVEST_COINMARKETCAP_KEY");
        assert_eq!(env_var_for("coingecko"), "SAFEINVEST_COINGECKO_KEY");
    }

    #[test]
    fn a_corrupt_settings_file_falls_back_to_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::at(dir.path());
        paths.ensure_created().unwrap();
        std::fs::write(paths.settings_file(), b"{ not json").unwrap();
        assert_eq!(SettingsService::new(paths).load(), AppSettings::default());
    }
}
