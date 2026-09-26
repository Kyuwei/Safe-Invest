//! User preferences and API keys.

use crate::fsx::{LockGuard, write_atomic};
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

/// Every quote source a settings file may name.
pub const PROVIDER_IDS: &[&str] = &[
    "coingecko",
    "coinmarketcap",
    "yahoo",
    "finnhub",
    "scraper",
    "simulated",
];

/// The sources that take an API key.
pub const KEYED_PROVIDERS: &[&str] = &["coingecko", "coinmarketcap", "finnhub"];

/// Longer than any real key. A paste that runs past this is not a key.
const MAX_KEY_LEN: usize = 256;

/// Display themes the interface knows how to draw.
const THEMES: &[&str] = &["system", "light", "dark"];

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

impl AppSettings {
    /// The same settings, with every value brought back inside what the
    /// program can act on.
    ///
    /// Applied on every write, whoever wrote. The settings screen is not the
    /// only author — a hand-edited file, an older version — and a refresh
    /// interval of zero or a port of 80 should come back as something sensible
    /// rather than as a loop that hammers a free API or a server that cannot
    /// start.
    #[must_use]
    pub fn sanitized(mut self) -> Self {
        let defaults = Self::default();

        if self.mcp_http_port < MIN_MCP_PORT {
            self.mcp_http_port = defaults.mcp_http_port;
        }
        self.refresh_interval_seconds = self.refresh_interval_seconds.clamp(15, 3600);
        self.quote_cache_seconds = self.quote_cache_seconds.clamp(5, 3600);
        self.default_fee_percent = self
            .default_fee_percent
            .clamp(Decimal::ZERO, Decimal::from(5));
        if self.default_starting_cash <= Decimal::ZERO
            || self.default_starting_cash > Decimal::from(1_000_000_000_000_i64)
        {
            self.default_starting_cash = defaults.default_starting_cash;
        }

        let currency = self.default_currency.trim().to_ascii_uppercase();
        self.default_currency =
            if currency.len() == 3 && currency.chars().all(|c| c.is_ascii_alphabetic()) {
                currency
            } else {
                defaults.default_currency
            };

        if !THEMES.contains(&self.theme.as_str()) {
            self.theme = defaults.theme;
        }

        self.crypto_provider_order =
            known_providers(self.crypto_provider_order, defaults.crypto_provider_order);
        self.stock_provider_order =
            known_providers(self.stock_provider_order, defaults.stock_provider_order);
        self.protected_api_keys
            .retain(|id, _| KEYED_PROVIDERS.contains(&id.as_str()));
        self
    }

    /// What the settings screen may see: everything but the secrets.
    pub fn preferences(&self) -> Preferences {
        Preferences {
            crypto_provider_order: self.crypto_provider_order.clone(),
            stock_provider_order: self.stock_provider_order.clone(),
            quote_cache_seconds: self.quote_cache_seconds,
            refresh_interval_seconds: self.refresh_interval_seconds,
            default_currency: self.default_currency.clone(),
            default_fee_percent: self.default_fee_percent,
            default_starting_cash: self.default_starting_cash,
            force_simulated_mode: self.force_simulated_mode,
            colour_blind_palette: self.colour_blind_palette,
            theme: self.theme.clone(),
            mcp_http_enabled: self.mcp_http_enabled,
            mcp_http_port: self.mcp_http_port,
        }
    }
}

/// Keeps the ids the program knows, once each, in the order given.
fn known_providers(order: Vec<String>, fallback: Vec<String>) -> Vec<String> {
    let mut kept: Vec<String> = Vec::with_capacity(order.len());
    for id in order {
        let id = id.trim().to_ascii_lowercase();
        if PROVIDER_IDS.contains(&id.as_str()) && !kept.contains(&id) {
            kept.push(id);
        }
    }
    if kept.is_empty() { fallback } else { kept }
}

/// The settings as the settings screen shows them.
///
/// A separate type rather than `AppSettings` with fields skipped, because the
/// file needs the sealed secrets and the page must never see them — not even
/// sealed. What the page is handed, it can send back; what it never had, it
/// cannot overwrite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preferences {
    pub crypto_provider_order: Vec<String>,
    pub stock_provider_order: Vec<String>,
    pub quote_cache_seconds: u64,
    pub refresh_interval_seconds: u64,
    pub default_currency: String,
    pub default_fee_percent: Decimal,
    pub default_starting_cash: Decimal,
    pub force_simulated_mode: bool,
    pub colour_blind_palette: bool,
    pub theme: String,
    pub mcp_http_enabled: bool,
    pub mcp_http_port: u16,
}

/// A change to the preferences: only the fields that are present change.
///
/// The settings screen used to read the whole file, change one box and write
/// the whole file back — secrets included. Anything written in between, a key
/// saved from another panel or a token minted by the MCP server, was put back
/// the way it had been. Sending only what changed makes that impossible, and
/// refusing unknown fields keeps the secrets out of reach of this path.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PreferencesPatch {
    pub crypto_provider_order: Option<Vec<String>>,
    pub stock_provider_order: Option<Vec<String>>,
    pub quote_cache_seconds: Option<u64>,
    pub refresh_interval_seconds: Option<u64>,
    pub default_currency: Option<String>,
    pub default_fee_percent: Option<Decimal>,
    pub default_starting_cash: Option<Decimal>,
    pub force_simulated_mode: Option<bool>,
    pub colour_blind_palette: Option<bool>,
    pub theme: Option<String>,
    pub mcp_http_enabled: Option<bool>,
    pub mcp_http_port: Option<u16>,
}

impl PreferencesPatch {
    /// Writes the fields that are present into `settings`.
    pub fn apply(self, settings: &mut AppSettings) {
        fn set<T>(slot: &mut T, value: Option<T>) {
            if let Some(value) = value {
                *slot = value;
            }
        }
        set(
            &mut settings.crypto_provider_order,
            self.crypto_provider_order,
        );
        set(
            &mut settings.stock_provider_order,
            self.stock_provider_order,
        );
        set(&mut settings.quote_cache_seconds, self.quote_cache_seconds);
        set(
            &mut settings.refresh_interval_seconds,
            self.refresh_interval_seconds,
        );
        set(&mut settings.default_currency, self.default_currency);
        set(&mut settings.default_fee_percent, self.default_fee_percent);
        set(
            &mut settings.default_starting_cash,
            self.default_starting_cash,
        );
        set(
            &mut settings.force_simulated_mode,
            self.force_simulated_mode,
        );
        set(
            &mut settings.colour_blind_palette,
            self.colour_blind_palette,
        );
        set(&mut settings.theme, self.theme);
        set(&mut settings.mcp_http_enabled, self.mcp_http_enabled);
        set(&mut settings.mcp_http_port, self.mcp_http_port);
    }

    /// Whether the change concerns where prices come from.
    ///
    /// Only then is the market rebuilt. Rebuilding it for a colour change
    /// used to throw away every cached quote and every rate-limit budget, and
    /// the next refresh spent a free tier's worth of calls finding out again.
    pub fn touches_market(&self) -> bool {
        self.crypto_provider_order.is_some()
            || self.stock_provider_order.is_some()
            || self.quote_cache_seconds.is_some()
            || self.force_simulated_mode.is_some()
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
    #[error("{0}")]
    Rejected(String),
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
            Ok(text) => serde_json::from_str::<AppSettings>(&text).map_or_else(
                |error| {
                    tracing::warn!(%error, path = %path.display(), "réglages illisibles, valeurs par défaut appliquées");
                    AppSettings::default()
                },
                AppSettings::sanitized,
            ),
            Err(_) => AppSettings::default(),
        }
    }

    /// Reads, changes and writes the settings under one lock.
    ///
    /// Every write goes through here. The window and the MCP server share the
    /// file, and a plain load-then-save from each would let one silently undo
    /// the other — a token minted by the server, erased by a checkbox ticked
    /// in the window a moment later. The file is replaced atomically, so a
    /// reader never sees half of it, and every value is brought within bounds
    /// on the way out.
    pub fn update<T>(
        &self,
        change: impl FnOnce(&mut AppSettings) -> Result<T, SettingsError>,
    ) -> Result<T, SettingsError> {
        self.paths.ensure_created()?;
        let _guard = LockGuard::acquire(&self.paths.lock_file())?;

        let mut settings = self.load();
        let outcome = change(&mut settings)?;
        let bytes = serde_json::to_vec_pretty(&settings.sanitized())?;
        write_atomic(&self.paths.settings_file(), &bytes)?;
        Ok(outcome)
    }

    /// Replaces the whole file with `settings`.
    pub fn save(&self, settings: &AppSettings) -> Result<(), SettingsError> {
        self.update(|stored| {
            stored.clone_from(settings);
            Ok(())
        })
    }

    /// Applies a change made on the settings screen.
    pub fn update_preferences(&self, patch: PreferencesPatch) -> Result<(), SettingsError> {
        self.update(|settings| {
            patch.apply(settings);
            Ok(())
        })
    }

    /// Stores a key for `provider_id`, sealed. An empty value clears it.
    pub fn set_api_key(&self, provider_id: &str, key: &str) -> Result<(), SettingsError> {
        if !KEYED_PROVIDERS.contains(&provider_id) {
            return Err(SettingsError::Rejected(format!(
                "Source inconnue ou sans clé : « {provider_id} »."
            )));
        }
        let trimmed = key.trim();
        if trimmed.len() > MAX_KEY_LEN
            || trimmed.chars().any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(SettingsError::Rejected(
                "Cette clé n'a pas la forme d'une clé d'API : vérifiez le copier-coller."
                    .to_owned(),
            ));
        }

        let sealed = if trimmed.is_empty() {
            None
        } else {
            journal::keep_out(trimmed);
            Some(secret::seal(trimmed)?.as_str().to_owned())
        };

        self.update(|settings| {
            match sealed {
                Some(sealed) => {
                    settings
                        .protected_api_keys
                        .insert(provider_id.to_owned(), sealed);
                }
                None => {
                    settings.protected_api_keys.remove(provider_id);
                }
            }
            Ok(())
        })
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
    ///
    /// Checked and minted under the settings lock, so the window and a
    /// command-line server starting at the same moment agree on one token
    /// instead of each writing its own.
    pub fn ensure_mcp_token(&self) -> Result<String, SettingsError> {
        if let Some(token) = self.mcp_token(&self.load()) {
            return Ok(token);
        }
        self.update(|settings| match self.mcp_token(settings) {
            Some(token) => Ok(token),
            None => Self::mint_into(settings),
        })
    }

    /// Mints a new token, invalidating the old one.
    pub fn regenerate_mcp_token(&self) -> Result<String, SettingsError> {
        self.update(Self::mint_into)
    }

    fn mint_into(settings: &mut AppSettings) -> Result<String, SettingsError> {
        let token = format!(
            "{:032x}{:032x}",
            uuid::Uuid::new_v4().as_u128(),
            uuid::Uuid::new_v4().as_u128()
        );
        journal::keep_out(&token);
        settings.protected_mcp_token = Some(secret::seal(&token)?.as_str().to_owned());
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

    /// The lost update this guards: the window saving a preference while the
    /// MCP server mints its token, each from its own copy of the file.
    #[test]
    fn concurrent_writers_keep_each_others_changes() {
        let dir = tempfile::tempdir().unwrap();
        let service = SettingsService::new(Paths::at(dir.path()));

        std::thread::scope(|scope| {
            for round in 0..20_u64 {
                let service = &service;
                scope.spawn(move || {
                    service
                        .update_preferences(PreferencesPatch {
                            refresh_interval_seconds: Some(15 + round),
                            ..PreferencesPatch::default()
                        })
                        .unwrap();
                });
                scope.spawn(move || {
                    service
                        .set_api_key("finnhub", "cle-finnhub-123456")
                        .unwrap();
                });
            }
            scope.spawn(|| service.ensure_mcp_token().unwrap());
        });

        let settings = service.load();
        assert!(
            service.mcp_token(&settings).is_some(),
            "le jeton a été perdu"
        );
        assert!(
            service.api_key(&settings, "finnhub").is_some(),
            "la clé a été perdue"
        );
    }

    /// The page must not be able to write a secret, sealed or not, through
    /// the preferences path.
    #[test]
    fn a_preference_change_cannot_carry_a_secret() {
        let smuggled = serde_json::from_str::<PreferencesPatch>(
            r#"{"colourBlindPalette":true,"protectedMcpToken":"plain:6162"}"#,
        );
        assert!(smuggled.is_err());

        let fine: PreferencesPatch =
            serde_json::from_str(r#"{"colourBlindPalette":true}"#).unwrap();
        assert_eq!(fine.colour_blind_palette, Some(true));
        assert!(
            !fine.touches_market(),
            "une couleur ne touche pas au marché"
        );
    }

    #[test]
    fn the_settings_screen_is_never_shown_a_secret() {
        let dir = tempfile::tempdir().unwrap();
        let service = SettingsService::new(Paths::at(dir.path()));
        service
            .set_api_key("coingecko", "cle-a-ne-pas-montrer")
            .unwrap();
        service.ensure_mcp_token().unwrap();

        let shown = serde_json::to_string(&service.load().preferences()).unwrap();
        assert!(!shown.contains("protected"), "{shown}");
        assert!(!shown.contains("plain:"), "{shown}");
        assert!(!shown.contains("dpapi:"), "{shown}");
    }

    #[test]
    fn values_the_program_cannot_act_on_are_brought_back_in_bounds() {
        let settings = AppSettings {
            mcp_http_port: 80,
            refresh_interval_seconds: 0,
            quote_cache_seconds: 1_000_000,
            default_fee_percent: Decimal::from(40),
            default_currency: "euros".into(),
            theme: "néon".into(),
            crypto_provider_order: vec!["inconnu".into(), "CoinGecko".into(), "coingecko".into()],
            stock_provider_order: vec![],
            ..AppSettings::default()
        }
        .sanitized();

        assert_eq!(settings.mcp_http_port, DEFAULT_MCP_PORT);
        assert_eq!(settings.refresh_interval_seconds, 15);
        assert_eq!(settings.quote_cache_seconds, 3600);
        assert_eq!(settings.default_fee_percent, Decimal::from(5));
        assert_eq!(settings.default_currency, "EUR");
        assert_eq!(settings.theme, "system");
        assert_eq!(settings.crypto_provider_order, ["coingecko"]);
        assert_eq!(
            settings.stock_provider_order,
            AppSettings::default().stock_provider_order
        );
    }

    #[test]
    fn a_key_for_an_unknown_source_or_with_spaces_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let service = SettingsService::new(Paths::at(dir.path()));

        assert!(service.set_api_key("yahoo", "abcdefgh").is_err());
        assert!(service.set_api_key("../evil", "abcdefgh").is_err());
        assert!(service.set_api_key("finnhub", "abc def ghi").is_err());
        assert!(service.load().protected_api_keys.is_empty());
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
