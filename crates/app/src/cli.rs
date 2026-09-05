//! Argument parsing, logging, and the `doctor` subcommand.
//!
//! Parsing is hand-written rather than pulled from a crate: the whole surface
//! is four subcommands and three flags, and a dependency that ends up in the
//! shipped binary should earn its place.

use anyhow::Context as _;
use safe_invest_service::{Context, ContextConfig};
use std::path::PathBuf;

pub const USAGE: &str = "\
Safe Invest — simulateur d'investissement pédagogique.

UTILISATION
    safe-invest [OPTIONS]              Ouvre la fenêtre
    safe-invest mcp [OPTIONS]          Démarre le serveur MCP (stdin/stdout)
    safe-invest doctor [OPTIONS]       Vérifie l'installation et affiche un diagnostic
    safe-invest --version
    safe-invest --help

OPTIONS
    --data-dir <CHEMIN>   Dossier des parties et des réglages
                          (par défaut : %LOCALAPPDATA%\\SafeInvest)
    --demo                Force le marché simulé : aucun appel réseau
    -h, --help            Affiche cette aide
    -V, --version         Affiche la version

VARIABLES D'ENVIRONNEMENT
    SAFEINVEST_DATA_DIR         Équivalent de --data-dir
    SAFEINVEST_SIMULATED=1      Équivalent de --demo
    SAFEINVEST_LOG              Niveau de journalisation (error, warn, info, debug)
    SAFEINVEST_<SOURCE>_KEY     Clé API d'une source, par exemple
                                SAFEINVEST_COINMARKETCAP_KEY

Pour brancher une IA, ajoutez à votre client MCP :
    { \"command\": \"safe-invest\", \"args\": [\"mcp\"] }";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    Window,
    Mcp,
    Doctor,
    Help,
    Version,
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    pub data_dir: Option<PathBuf>,
    pub demo: bool,
}

/// Reads the command line. Returns the message to print on a bad invocation
/// rather than exiting, so `main` decides how to report it.
pub fn parse(args: impl IntoIterator<Item = String>) -> Result<(Command, Options), String> {
    let mut command = None;
    let mut options = Options {
        demo: matches!(
            std::env::var("SAFEINVEST_SIMULATED").as_deref(),
            Ok("1" | "true")
        ),
        ..Options::default()
    };

    let mut args = args.into_iter().peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok((Command::Help, options)),
            "-V" | "--version" => return Ok((Command::Version, options)),
            "--demo" => options.demo = true,
            "--data-dir" => {
                let path = args
                    .next()
                    .ok_or_else(|| "--data-dir attend un chemin.".to_owned())?;
                options.data_dir = Some(PathBuf::from(path));
            }
            other if other.starts_with('-') => {
                return Err(format!("Option inconnue : {other}"));
            }
            "mcp" if command.is_none() => command = Some(Command::Mcp),
            "doctor" if command.is_none() => command = Some(Command::Doctor),
            other => return Err(format!("Argument inattendu : {other}")),
        }
    }

    Ok((command.unwrap_or(Command::Window), options))
}

pub fn build_context(options: &Options) -> anyhow::Result<Context> {
    Context::new(&ContextConfig {
        data_dir: options.data_dir.clone(),
        force_simulated: options.demo,
    })
    .context("impossible de préparer le dossier de données")
}

/// Sets up logging. `stderr_only` is what MCP mode needs — stdout is the
/// transport there, and a log line on it corrupts the protocol stream.
pub fn init_logging(stderr_only: bool) {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_env("SAFEINVEST_LOG")
        .unwrap_or_else(|_| EnvFilter::new("safe_invest=info,warn"));

    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    let installed = if stderr_only {
        builder.with_writer(std::io::stderr).try_init()
    } else {
        builder.try_init()
    };
    let _ = installed;
}

/// Reattaches the process to the terminal that launched it.
///
/// The Windows build is a GUI-subsystem executable so double-clicking it does
/// not flash a console window. The price is that `--version` typed at a prompt
/// would print into the void; this buys it back for the console subcommands.
/// Never called in MCP mode, where the client supplies its own pipes.
pub fn attach_console() {
    #[cfg(all(windows, feature = "gui", not(debug_assertions)))]
    {
        #![allow(
            unsafe_code,
            reason = "AttachConsole is a C API with no safe wrapper in-tree"
        )]
        use windows_sys::Win32::System::Console::{ATTACH_PARENT_PROCESS, AttachConsole};

        // SAFETY: no arguments, no pointers; the call either attaches to the
        // parent's console or reports that there is none.
        unsafe {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}

/// Prints what the program can see of its own installation.
///
/// This exists because the previous release would not start on the user's
/// machine and said nothing about why. A diagnostic that names the data
/// directory, the webview and the configured sources turns "it does not work"
/// into a sentence someone can act on.
pub fn doctor(options: &Options) -> anyhow::Result<()> {
    println!("Safe Invest {}", safe_invest_core::VERSION);
    println!(
        "  système        : {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    println!("  exécutable     : {}", executable_path());

    println!();
    println!("Interface graphique");
    println!(
        "  incluse        : {}",
        if cfg!(feature = "gui") { "oui" } else { "non" }
    );
    println!("  moteur web     : {}", webview_report());

    let context = build_context(options)?;
    let paths = context.store().paths();
    println!();
    println!("Données");
    println!("  dossier        : {}", paths.root().display());
    println!("  accessible     : {}", writable_report(paths.root()));
    println!("  parties        : {}", context.list_games().len());

    let settings = context.settings();
    println!();
    println!("Sources de cours");
    println!(
        "  mode           : {}",
        if settings.force_simulated_mode {
            "simulé (aucun appel réseau)"
        } else {
            "réel, avec repli simulé"
        }
    );
    println!(
        "  ordre crypto   : {}",
        settings.crypto_provider_order.join(" → ")
    );
    println!(
        "  ordre actions  : {}",
        settings.stock_provider_order.join(" → ")
    );

    // Which sources have a key, never what the key is.
    let configured: Vec<&str> = ["coingecko", "coinmarketcap", "finnhub"]
        .into_iter()
        .filter(|id| context.settings_service().api_key(&settings, id).is_some())
        .collect();
    println!(
        "  clés définies  : {}",
        if configured.is_empty() {
            "aucune (l'application fonctionne sans clé)".to_owned()
        } else {
            configured.join(", ")
        }
    );

    Ok(())
}

fn executable_path() -> String {
    std::env::current_exe().map_or_else(
        |_| "(inconnu)".to_owned(),
        |path| path.display().to_string(),
    )
}

fn writable_report(path: &std::path::Path) -> String {
    let probe = path.join(".write-probe");
    match std::fs::write(&probe, b"ok") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            "oui".to_owned()
        }
        Err(error) => format!("NON — {error}"),
    }
}

#[cfg(feature = "gui")]
fn webview_report() -> String {
    match tauri::webview_version() {
        Ok(version) => format!("disponible (version {version})"),
        Err(error) => format!(
            "INTROUVABLE — {error}\n                   \
             Sur Windows, installez « Microsoft Edge WebView2 Runtime » :\n                   \
             https://developer.microsoft.com/microsoft-edge/webview2/"
        ),
    }
}

#[cfg(not(feature = "gui"))]
fn webview_report() -> String {
    "sans objet (build console)".to_owned()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "a test that trips is a test that failed"
)]
mod tests {
    use super::*;

    fn parse_args(args: &[&str]) -> Result<(Command, Options), String> {
        parse(args.iter().map(|s| (*s).to_owned()))
    }

    #[test]
    fn no_arguments_opens_the_window() {
        let (command, _) = parse_args(&[]).unwrap();
        assert_eq!(command, Command::Window);
    }

    #[test]
    fn the_mcp_subcommand_is_recognised() {
        let (command, _) = parse_args(&["mcp"]).unwrap();
        assert_eq!(command, Command::Mcp);
    }

    #[test]
    fn options_may_come_before_or_after_the_subcommand() {
        let (command, options) = parse_args(&["--demo", "mcp"]).unwrap();
        assert_eq!(command, Command::Mcp);
        assert!(options.demo);

        let (command, options) = parse_args(&["mcp", "--demo"]).unwrap();
        assert_eq!(command, Command::Mcp);
        assert!(options.demo);
    }

    #[test]
    fn a_data_directory_is_read_as_a_path() {
        let (_, options) = parse_args(&["--data-dir", "/tmp/parties"]).unwrap();
        assert_eq!(options.data_dir, Some(PathBuf::from("/tmp/parties")));
    }

    #[test]
    fn a_data_directory_without_a_value_is_refused() {
        assert!(parse_args(&["--data-dir"]).is_err());
    }

    #[test]
    fn an_unknown_option_is_refused_rather_than_ignored() {
        let error = parse_args(&["--turbo"]).unwrap_err();
        assert!(error.contains("--turbo"));
    }

    #[test]
    fn help_and_version_win_over_anything_else() {
        assert_eq!(parse_args(&["mcp", "--help"]).unwrap().0, Command::Help);
        assert_eq!(parse_args(&["-V"]).unwrap().0, Command::Version);
    }

    #[test]
    fn the_usage_text_shows_how_to_wire_an_ai_client() {
        assert!(USAGE.contains("\"command\": \"safe-invest\""));
        assert!(USAGE.contains("\"args\": [\"mcp\"]"));
    }
}
