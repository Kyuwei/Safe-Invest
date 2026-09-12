//! User preferences and API keys.

use crate::journal;
use crate::paths::Paths;
use crate::secret::{self, Sealed};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The loopback port the MCP server listens on when nothing else is asked for.
///
/// A TCP port is sixteen bits, so 65 535 is the ceiling — 98 000 cannot be
/// expressed at all. This is the nearest thing that fits, clear of the
/// registered range and of what local tooling usually grabs.
pub const DEFAULT_MCP_PORT: u16 = 9800;

/// Below this, ports are reserved and need privileges on Unix.
pub const MIN_MCP_PORT: u16 = 1024;

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

    /// Serve MCP on a loopback port as well as on stdin/stdout.
    ///
    /// Off until somebody turns it on. Opening a port is a decision about a
    /// machine, and not one a program should make for a person at first launch.
    pub mcp_http_enabled: bool,
    pub mcp_http_port: u16,
    /// The sealed bearer token for that port. Written the first time the server
    /// is switched on, and never stored in the clear on Windows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub protected_mcp_token: Option<String>,
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
            mcp_http_enabled: false,
            mcp_http_port: DEFAULT_MCP_PORT,
            protected_mcp_token: None,
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
            journal::keep_out(trimmed);
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
                Ok(key) => {
                    // Registered on the way out, not on the way in: this is the
                    // one function every caller goes through to obtain a key in
                    // the clear, so the journal learns about it here or nowhere.
                    journal::keep_out(&key);
                    return Some(key);
                }
                Err(error) => {
                    tracing::warn!(provider = provider_id, %error, "clé API illisible");
                }
            }
        }
        let from_env = std::env::var(env_var_for(provider_id))
            .ok()
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty());
        if let Some(key) = from_env.as_deref() {
            journal::keep_out(key);
        }
        from_env
    }

    /// The bearer token for the MCP port, minting one the first time it is needed.
    ///
    /// Two version-4 UUIDs, hex, no dashes. That is 244 bits from the same
    /// system entropy source a key would come from — enough that guessing is
    /// not a strategy — and it costs no dependency the program did not already
    /// have.
    pub fn ensure_mcp_token(&self) -> Result<String, SettingsError> {
        let settings = self.load();
        if let Some(token) = self.mcp_token(&settings) {
            return Ok(token);
        }
        self.regenerate_mcp_token()
    }

    /// Mints a new token, invalidating the old one.
    pub fn regenerate_mcp_token(&self) -> Result<String, SettingsError> {
        let token = format!(
            "{:032x}{:032x}",
            uuid::Uuid::new_v4().as_u128(),
            uuid::Uuid::new_v4().as_u128()
        );
        let mut settings = self.load();
        settings.protected_mcp_token = Some(secret::seal(&token)?.as_str().to_owned());
        self.save(&settings)?;
        journal::keep_out(&token);
        Ok(token)
    }

    /// The stored token, or `None` when none has been minted or it cannot be
    /// unsealed — a token that will not open the lock is not a token.
    pub fn mcp_token(&self, settings: &AppSettings) -> Option<String> {
        let stored = settings.protected_mcp_token.as_ref()?;
        match secret::unseal(&Sealed::from_stored(stored.clone())) {
            Ok(token) => {
                journal::keep_out(&token);
                Some(token)
            }
            Err(error) => {
                tracing::warn!(%error, "jeton MCP illisible, il sera régénéré");
                None
            }
        }
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

    /// The journal must not be able to learn a key by accident. Reading one is
    /// the moment it becomes plaintext, so that is the moment it is registered.
    #[test]
    fn reading_a_key_teaches_the_journal_to_hide_it() {
        let dir = tempfile::tempdir().unwrap();
        let service = SettingsService::new(Paths::at(dir.path()));
        service
            .set_api_key("coingecko", "cle-coingecko-a-masquer")
            .unwrap();

        let settings = service.load();
        service.api_key(&settings, "coingecko").unwrap();

        assert!(
            !journal::scrub("requête avec cle-coingecko-a-masquer")
                .contains("cle-coingecko-a-masquer")
        );
    }

    #[test]
    fn the_mcp_token_is_hidden_from_the_journal_too() {
        let dir = tempfile::tempdir().unwrap();
        let service = SettingsService::new(Paths::at(dir.path()));
        let token = service.ensure_mcp_token().unwrap();

        assert!(!journal::scrub(&format!("Bearer {token}")).contains(&token));
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

    #[test]
    fn the_mcp_token_is_minted_once_and_kept() {
        let dir = tempfile::tempdir().unwrap();
        let service = SettingsService::new(Paths::at(dir.path()));

        let first = service.ensure_mcp_token().unwrap();
        assert_eq!(first.len(), 64, "244 bits en hexadécimal");
        assert!(first.chars().all(|c| c.is_ascii_hexdigit()));

        // Asking again must return the same token: a client that stored it
        // yesterday has to still get in today.
        assert_eq!(service.ensure_mcp_token().unwrap(), first);
    }

    #[test]
    fn regenerating_the_token_invalidates_the_previous_one() {
        let dir = tempfile::tempdir().unwrap();
        let service = SettingsService::new(Paths::at(dir.path()));

        let first = service.ensure_mcp_token().unwrap();
        let second = service.regenerate_mcp_token().unwrap();

        assert_ne!(first, second);
        assert_eq!(service.ensure_mcp_token().unwrap(), second);
    }

    #[test]
    fn the_token_is_never_written_in_the_clear() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::at(dir.path());
        let service = SettingsService::new(paths.clone());

        let token = service.ensure_mcp_token().unwrap();
        let on_disk = std::fs::read_to_string(paths.settings_file()).unwrap();

        assert!(
            !on_disk.contains(&token),
            "le jeton est en clair dans le fichier"
        );
    }

    /// A settings file written before this feature existed has no port and no
    /// token, and must still load — with the port defaulted, not zero.
    #[test]
    fn an_older_settings_file_gains_the_default_port() {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::at(dir.path());
        paths.ensure_created().unwrap();
        std::fs::write(paths.settings_file(), br#"{"defaultCurrency":"USD"}"#).unwrap();

        let settings = SettingsService::new(paths).load();
        assert_eq!(settings.default_currency, "USD");
        assert_eq!(settings.mcp_http_port, DEFAULT_MCP_PORT);
        assert!(!settings.mcp_http_enabled);
        assert!(settings.protected_mcp_token.is_none());
    }
}
