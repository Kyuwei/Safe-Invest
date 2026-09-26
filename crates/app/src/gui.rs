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

    // Tauri would otherwise spin up one worker per core. This app's async work
    // is a handful of small HTTP requests a minute; two workers cover it, and
    // the thread stacks and per-worker allocations of a 16-core machine do not
    // sit in memory for nothing.
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .thread_name("safe-invest-async")
        .build()
        .context("impossible de démarrer l'exécuteur asynchrone")?;
    tauri::async_runtime::set(runtime.handle().clone());

    let context = crate::cli::build_context(options)?;

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(context.clone())
        .manage(crate::mcp_port::McpPort::default())
        .invoke_handler(tauri::generate_handler![
            crate::commands::app_info,
            crate::commands::list_games,
            crate::commands::create_game,
            crate::commands::open_game,
            crate::commands::delete_game,
            crate::commands::dashboard,
            crate::commands::end_game,
            crate::commands::summary,
            crate::commands::history,
            crate::commands::market,
            crate::commands::asset,
            crate::commands::price_history,
            crate::commands::buy,
            crate::commands::sell,
            crate::commands::get_settings,
            crate::commands::save_settings,
            crate::commands::set_api_key,
            crate::commands::market_sources,
            crate::commands::mcp_access,
            crate::commands::regenerate_mcp_token,
            crate::commands::open_data_dir,
            crate::commands::read_journal,
            crate::commands::export_journal,
            crate::commands::log_ui_error,
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

            // If the port was left switched on, open it now rather than at the
            // first visit to the settings screen — a client pointed at the URL
            // should find it as soon as the window is up.
            let started = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let context = started
                    .state::<safe_invest_service::Context>()
                    .inner()
                    .clone();
                let status = started
                    .state::<crate::mcp_port::McpPort>()
                    .reconcile(&context)
                    .await;
                if let Some(problem) = status.problem {
                    tracing::warn!(%problem, "le port MCP demandé n'a pas pu être ouvert");
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .context("l'interface n'a pas pu démarrer")
}
