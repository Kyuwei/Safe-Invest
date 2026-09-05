//! The window.
//!
//! Small on purpose: it wires the service into Tauri, starts the file watcher
//! that makes AI mode live, and gets out of the way. Everything the interface
//! can do is a command in [`crate::commands`].

use crate::cli::Options;
use anyhow::Context as _;
use safe_invest_core::watcher::StoreWatcher;
use tauri::{Emitter, Manager};

/// Sent to the page whenever a game file changes on disk — which is how the
/// window notices that the MCP server, in another process, just traded.
const GAME_CHANGED: &str = "safe-invest://game-changed";

pub fn run(options: &Options) -> anyhow::Result<()> {
    // Fail with a sentence someone can act on rather than a blank window. This
    // is the failure the previous release had and could not explain.
    if let Err(error) = tauri::webview_version() {
        anyhow::bail!(
            "le moteur web du système est introuvable ({error}).\n\
             Sur Windows, installez « Microsoft Edge WebView2 Runtime » :\n\
             https://developer.microsoft.com/microsoft-edge/webview2/\n\
             Puis relancez Safe Invest. `safe-invest doctor` vérifie l'installation."
        );
    }

    let context = crate::cli::build_context(options)?;

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(context.clone())
        .invoke_handler(tauri::generate_handler![
            crate::commands::app_info,
            crate::commands::list_games,
            crate::commands::create_game,
            crate::commands::open_game,
            crate::commands::delete_game,
            crate::commands::set_goal,
            crate::commands::dashboard,
            crate::commands::history,
            crate::commands::market,
            crate::commands::price_history,
            crate::commands::buy,
            crate::commands::sell,
            crate::commands::get_settings,
            crate::commands::save_settings,
            crate::commands::set_api_key,
            crate::commands::market_sources,
            crate::commands::open_data_dir,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            let watcher = StoreWatcher::start(context.store().paths(), move || {
                // A file changed; tell the page to reload. Sending the reason
                // rather than the data keeps this thread off the market API.
                let _ = handle.emit(GAME_CHANGED, ());
            })
            .context("impossible de surveiller le dossier des parties")?;

            // Held for the life of the app; dropping it stops the watching.
            app.manage(watcher);
            Ok(())
        })
        .run(tauri::generate_context!())
        .context("l'interface n'a pas pu démarrer")
}
