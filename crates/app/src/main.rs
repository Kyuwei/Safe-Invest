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

use cli::{Command, Options};

fn main() -> std::process::ExitCode {
    let (command, options) = match cli::parse(std::env::args().skip(1)) {
        Ok(parsed) => parsed,
        Err(message) => {
            cli::attach_console();
            eprintln!("{message}\n");
            eprintln!("{}", cli::USAGE);
            return std::process::ExitCode::from(2);
        }
    };

    match run(command, &options) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            cli::attach_console();
            eprintln!("Erreur : {error:#}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run(command: Command, options: &Options) -> anyhow::Result<()> {
    match command {
        Command::Help => {
            cli::attach_console();
            println!("{}", cli::USAGE);
            Ok(())
        }
        Command::Version => {
            cli::attach_console();
            println!("Safe Invest {}", safe_invest_core::VERSION);
            Ok(())
        }
        Command::Doctor => {
            cli::attach_console();
            cli::init_logging(false);
            cli::doctor(options)
        }
        Command::Mcp => {
            // Logs go to stderr and only to stderr: stdout carries the protocol,
            // and one stray line on it makes the client stop answering.
            cli::init_logging(true);
            // Two workers: the server answers one JSON-RPC call at a time and
            // spends that time waiting on the network, not on the CPU.
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .thread_name("safe-invest-mcp")
                .build()?;
            runtime.block_on(async {
                let context = cli::build_context(options)?;
                safe_invest_mcp::serve_stdio(context).await
            })
        }
        Command::Window => run_window(options),
    }
}

#[cfg(feature = "gui")]
fn run_window(options: &Options) -> anyhow::Result<()> {
    cli::init_logging(false);
    gui::run(options)
}

#[cfg(not(feature = "gui"))]
fn run_window(_options: &Options) -> anyhow::Result<()> {
    cli::attach_console();
    anyhow::bail!(
        "cette version a été compilée sans interface graphique ; utilisez `safe-invest mcp` ou `safe-invest doctor`"
    )
}
