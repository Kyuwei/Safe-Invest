//! Safe Invest — one executable.
//!
//! Run it with no arguments and it opens the window. Run `safe-invest mcp` and
//! the same file speaks the Model Context Protocol on stdin and stdout, so an
//! AI can play. That is deliberate: two programs would mean two versions to
//! keep in step, two things to install, and two places for a rule to drift.

// The window build must not flash a console. The console subcommands attach to
// the parent's console themselves (see `cli::attach_console`), and MCP mode
// works regardless because a client passes it pipes for stdin and stdout.
#![cfg_attr(
    all(windows, feature = "gui", not(debug_assertions)),
    windows_subsystem = "windows"
)]
// A binary crate has no downstream users, so `unreachable_pub` fires on every
// item — including the `pub fn`s that `#[tauri::command]` requires.
#![allow(
    unreachable_pub,
    reason = "binary crate; `pub` is required by tauri::command"
)]

mod cli;
#[cfg(feature = "gui")]
mod commands;
#[cfg(feature = "gui")]
mod gui;
#[cfg(feature = "gui")]
mod mcp_port;

use cli::{Command, Options, errln, outln};
use std::sync::atomic::{AtomicBool, Ordering};

/// Set once the command is known to be the window: the one mode in which a
/// failure has to be shown in a dialog, because nobody is reading a console.
static WINDOWED: AtomicBool = AtomicBool::new(false);

fn main() -> std::process::ExitCode {
    report_panics();

    let (command, options) = match cli::parse(std::env::args().skip(1)) {
        Ok(parsed) => parsed,
        Err(message) => {
            cli::attach_console();
            errln!("{message}\n");
            errln!("{}", cli::USAGE);
            return std::process::ExitCode::from(2);
        }
    };
    WINDOWED.store(command == Command::Window, Ordering::Relaxed);

    match run(command, &options) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            let message = format!("{error:#}");
            tracing::error!("arrêt sur erreur : {message}");
            // Double-clicked from Explorer, there is no console to print to,
            // and a window that never opens says nothing at all. Say it in a
            // box instead — this is where a missing WebView2 is announced.
            if cli::attach_console() || command != Command::Window {
                errln!("Erreur : {message}");
            } else {
                safe_invest_platform::dialog::error(
                    "Safe Invest ne peut pas démarrer",
                    &format!("{message}\n\n`safe-invest.exe doctor` affiche un diagnostic."),
                );
            }
            std::process::ExitCode::FAILURE
        }
    }
}

/// Writes a panic into the journal before the process goes.
///
/// The release build aborts on panic, which ends the process without a word:
/// no console in the windowed build, and nothing in the journal either. This
/// records what broke and where, then — in the window — says so on screen.
fn report_panics() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = info.payload();
        let message = payload
            .downcast_ref::<&str>()
            .map(ToString::to_string)
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_else(|| "erreur sans message".to_owned());
        let location = info
            .location()
            .map_or_else(String::new, |at| format!(" ({}:{})", at.file(), at.line()));

        tracing::error!(target: "safe_invest::panic", "erreur interne : {message}{location}");
        default(info);

        if WINDOWED.load(Ordering::Relaxed) {
            safe_invest_platform::dialog::error(
                "Safe Invest doit s'arrêter",
                &format!(
                    "Une erreur interne s'est produite : {message}{location}\n\n\
                     Le journal de diagnostic en garde la trace (Paramètres → Journal → \
                     Exporter le journal)."
                ),
            );
        }
    }));
}

fn run(command: Command, options: &Options) -> anyhow::Result<()> {
    match command {
        Command::Help => {
            cli::attach_console();
            outln!("{}", cli::USAGE);
            Ok(())
        }
        Command::Version => {
            cli::attach_console();
            outln!("Safe Invest {}", safe_invest_core::VERSION);
            Ok(())
        }
        Command::Doctor => {
            cli::attach_console();
            cli::init_logging(options, "doctor");
            cli::doctor(options)
        }
        Command::Mcp => {
            cli::init_logging(options, "mcp");
            // Two workers: the server answers one JSON-RPC call at a time and
            // spends that time waiting on the network, not on the CPU.
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .thread_name("safe-invest-mcp")
                .build()?;
            runtime.block_on(async {
                let context = cli::build_context(options)?;

                if !options.http {
                    return safe_invest_mcp::serve_stdio(context).await;
                }

                // A port needs a token, and the port needs saying out loud:
                // whoever ran this has to know where to point their client and
                // what to send with it. Both go to stderr, which is where
                // everything but the protocol goes in this mode.
                let settings = context.settings_service();
                let token = settings.ensure_mcp_token()?;
                let port = options.port.unwrap_or(settings.load().mcp_http_port);

                let listener = safe_invest_mcp::http::bind(port).await.map_err(|error| {
                    anyhow::anyhow!(
                        "impossible d'écouter sur 127.0.0.1:{port} : {error}. \
                         Un autre programme utilise peut-être ce port ; essayez --port."
                    )
                })?;

                errln!("Serveur MCP sur http://127.0.0.1:{port}/mcp");
                errln!("Authorization: Bearer {token}");
                safe_invest_mcp::http::serve(listener, context, token).await;
                Ok(())
            })
        }
        Command::Window => run_window(options),
    }
}

#[cfg(feature = "gui")]
fn run_window(options: &Options) -> anyhow::Result<()> {
    cli::init_logging(options, "fenêtre");
    gui::run(options)
}

#[cfg(not(feature = "gui"))]
fn run_window(_options: &Options) -> anyhow::Result<()> {
    cli::attach_console();
    anyhow::bail!(
        "cette version a été compilée sans interface graphique ; utilisez `safe-invest mcp` ou `safe-invest doctor`"
    )
}
