//! Argument parsing, logging, and the `doctor` subcommand.
//!
//! Parsing is hand-written rather than pulled from a crate: the whole surface
//! is four subcommands and three flags, and a dependency that ends up in the
//! shipped binary should earn its place.

use anyhow::Context as _;
use safe_invest_core::journal;
use safe_invest_core::settings::MIN_MCP_PORT as MIN_PORT;
use safe_invest_service::{Context, ContextConfig};
use std::path::PathBuf;

/// Writes one line, ignoring a failure to do so.
///
/// `println!` panics when the write fails, and a GUI-subsystem process started
/// without a console has no standard output to fail on. Under `panic = "abort"`
/// that panic becomes a bare non-zero exit code with nothing printed — the
/// worst possible way to report a version number, and how the release build
/// first failed its own smoke test.
pub fn write_line(mut sink: impl std::io::Write, text: &str) {
    let _ = sink.write_all(text.as_bytes());
    let _ = sink.write_all(b"\n");
    let _ = sink.flush();
}

macro_rules! outln {
    () => { $crate::cli::write_line(std::io::stdout(), "") };
    ($($arg:tt)*) => { $crate::cli::write_line(std::io::stdout(), &format!($($arg)*)) };
}

macro_rules! errln {
    ($($arg:tt)*) => { $crate::cli::write_line(std::io::stderr(), &format!($($arg)*)) };
}

pub(crate) use {errln, outln};

pub const USAGE: &str = "\
Safe Invest — simulateur d'investissement pédagogique.

UTILISATION
    safe-invest [OPTIONS]              Ouvre la fenêtre
    safe-invest mcp [OPTIONS]          Démarre le serveur MCP (stdin/stdout)
    safe-invest mcp --http [--port N]  Démarre le serveur MCP sur 127.0.0.1
    safe-invest doctor [OPTIONS]       Vérifie l'installation et affiche un diagnostic
    safe-invest --version
    safe-invest --help

OPTIONS
    --data-dir <CHEMIN>   Dossier des parties et des réglages
                          (par défaut : %LOCALAPPDATA%\\SafeInvest)
    --demo                Force le marché simulé : aucun appel réseau
    --http                Sert le MCP sur un port de bouclage au lieu de stdio
    --port <PORT>         Port d'écoute (implique --http ; 9800 par défaut).
                          L'écoute est toujours limitée à 127.0.0.1.
    -h, --help            Affiche cette aide
    -V, --version         Affiche la version

VARIABLES D'ENVIRONNEMENT
    SAFEINVEST_DATA_DIR         Équivalent de --data-dir
    SAFEINVEST_SIMULATED=1      Équivalent de --demo
    SAFEINVEST_LOG              Niveau de journalisation (error, warn, info, debug)
    SAFEINVEST_<SOURCE>_KEY     Clé API d'une source, par exemple
                                SAFEINVEST_COINMARKETCAP_KEY

Pour brancher une IA, ajoutez à votre client MCP :
    { \"command\": \"safe-invest\", \"args\": [\"mcp\"] }
ou, en mode port, pointez le client sur http://127.0.0.1:9800/mcp avec
l'en-tête « Authorization: Bearer … » que les Paramètres affichent.";

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
    /// `mcp --http` — serve on a loopback port instead of stdin/stdout.
    pub http: bool,
    /// The port for that, when `--port` was given. `None` uses the setting.
    pub port: Option<u16>,
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
            "--http" => options.http = true,
            "--port" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--port attend un numéro de port.".to_owned())?;
                options.port = Some(parse_port(&value)?);
                options.http = true;
            }
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

/// Reads a port, refusing the two mistakes that are easy to make.
///
/// A TCP port is sixteen bits. "98000" is not a large port, it is not a port,
/// and the error says so rather than wrapping it into something that happens to
/// fit.
fn parse_port(value: &str) -> Result<u16, String> {
    let trimmed = value.trim();
    let port: u32 = trimmed
        .parse()
        .map_err(|_| format!("Port invalide : « {trimmed} »."))?;

    let port = u16::try_from(port)
        .map_err(|_| format!("Port hors limites : {port}. Un port va de {MIN_PORT} à 65535."))?;

    if port < MIN_PORT {
        return Err(format!(
            "Port réservé : {port}. Choisissez un port entre {MIN_PORT} et 65535."
        ));
    }
    Ok(port)
}

pub fn build_context(options: &Options) -> anyhow::Result<Context> {
    Context::new(&ContextConfig {
        data_dir: options.data_dir.clone(),
        force_simulated: options.demo,
    })
    .context("impossible de préparer le dossier de données")
}

/// Where the program is writing its files, before a `Context` exists.
///
/// Logging is set up first — a failure while building the context is exactly
/// the kind of thing the journal is for — so it resolves the directory the
/// same way `Context` will, rather than waiting for it.
pub fn paths_for(options: &Options) -> safe_invest_core::Paths {
    options.data_dir.clone().map_or_else(
        safe_invest_core::Paths::discover,
        safe_invest_core::Paths::at,
    )
}

/// Sends every log line to the console and to the journal at once.
///
/// Two sinks, because they answer different questions. The console is for
/// whoever is watching right now; the journal is for the person who noticed an
/// hour later and has nothing but a window that misbehaved.
///
/// The console half is always standard error. In MCP mode standard output
/// carries the protocol and one stray line on it makes the client stop
/// answering; in `doctor` it carries a report people paste into bug reports.
/// Neither wants log lines mixed in.
struct Sink {
    journal: Option<journal::Handle>,
}

impl std::io::Write for Sink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if let Some(journal) = self.journal.as_mut() {
            let _ = journal.write(buf);
        }
        // Best-effort on purpose: a windowed build has no console attached, and
        // that must not cost the journal its line.
        let _ = std::io::stderr().write_all(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if let Some(journal) = self.journal.as_mut() {
            let _ = journal.flush();
        }
        Ok(())
    }
}

/// Sets up logging: standard error for whoever is watching, and the journal
/// under the data directory for everyone else.
pub fn init_logging(options: &Options) {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_env("SAFEINVEST_LOG")
        .unwrap_or_else(|_| EnvFilter::new("safe_invest=info,warn"));

    // A journal that cannot be opened — a read-only disk, a directory taken by
    // a file — is a diagnostic lost, never a launch refused.
    let journal = journal::Handle::open(&paths_for(options)).ok();
    if journal.is_none() {
        errln!("Journal indisponible : les messages n'iront qu'à la console.");
    }

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        // No ANSI: the journal is read as a text file, and escape codes turn it
        // into something nobody can quote in a bug report.
        .with_ansi(false)
        .with_writer(move || Sink {
            journal: journal.clone(),
        })
        .try_init();

    tracing::info!(version = safe_invest_core::VERSION, "Safe Invest démarre");
}

/// Reattaches the process to the terminal that launched it.
///
/// The Windows build is a GUI-subsystem executable so double-clicking it does
/// not flash a console window. The price is that `--version` typed at a prompt
/// would print into the void; this buys it back for the console subcommands.
/// Never called in MCP mode, where the client supplies its own pipes.
pub fn attach_console() {
    // Only the windowed release build starts without a console; every other
    // configuration already has one.
    #[cfg(all(windows, feature = "gui", not(debug_assertions)))]
    {
        safe_invest_platform::console::attach();
    }
}

/// Prints what the program can see of its own installation.
///
/// This exists because the previous release would not start on the user's
/// machine and said nothing about why. A diagnostic that names the data
/// directory, the webview and the configured sources turns "it does not work"
/// into a sentence someone can act on.
pub fn doctor(options: &Options) -> anyhow::Result<()> {
    outln!("Safe Invest {}", safe_invest_core::VERSION);
    outln!(
        "  système        : {} {}",
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    outln!("  exécutable     : {}", executable_path());

    outln!();
    outln!("Interface graphique");
    outln!(
        "  incluse        : {}",
        if cfg!(feature = "gui") { "oui" } else { "non" }
    );
    outln!("  moteur web     : {}", webview_report());

    let context = build_context(options)?;
    let paths = context.store().paths();
    outln!();
    outln!("Données");
    outln!("  dossier        : {}", paths.root().display());
    outln!("  accessible     : {}", writable_report(paths.root()));
    outln!("  parties        : {}", context.list_games().len());

    let settings = context.settings();
    outln!();
    outln!("Sources de cours");
    outln!(
        "  mode           : {}",
        if settings.force_simulated_mode {
            "simulé (aucun appel réseau)"
        } else {
            "réel, avec repli simulé"
        }
    );
    outln!(
        "  ordre crypto   : {}",
        settings.crypto_provider_order.join(" → ")
    );
    outln!(
        "  ordre actions  : {}",
        settings.stock_provider_order.join(" → ")
    );

    // Which sources have a key, never what the key is.
    let configured: Vec<&str> = ["coingecko", "coinmarketcap", "finnhub"]
        .into_iter()
        .filter(|id| context.settings_service().api_key(&settings, id).is_some())
        .collect();
    outln!(
        "  clés définies  : {}",
        if configured.is_empty() {
            "aucune (l'application fonctionne sans clé)".to_owned()
        } else {
            configured.join(", ")
        }
    );

    outln!();
    outln!("Serveur MCP");
    outln!("  stdio          : toujours disponible (`safe-invest mcp`)");
    outln!(
        "  port local     : {}",
        if settings.mcp_http_enabled {
            format!("activé sur 127.0.0.1:{}", settings.mcp_http_port)
        } else {
            "désactivé".to_owned()
        }
    );
    // Whether a token exists, never the token. A diagnostic gets pasted into
    // bug reports, and this one is a credential like any other.
    outln!(
        "  jeton          : {}",
        if context.settings_service().mcp_token(&settings).is_some() {
            "défini"
        } else {
            "aucun (créé au premier démarrage du port)"
        }
    );

    outln!();
    outln!("Journal");
    outln!("  fichier        : {}", journal::file(paths).display());
    outln!("  taille         : {}", human_size(journal::size(paths)));
    outln!("  lignes gardées : {}", journal::tail(paths, 100_000).len());

    Ok(())
}

/// Bytes, said the way a person reads them.
fn human_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} o")
    } else if bytes < 1024 * 1024 {
        format!("{} ko", bytes / 1024)
    } else {
        format!("{} Mo", bytes / (1024 * 1024))
    }
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

    /// The number this feature was asked for. It is not a port, and the error
    /// must say that rather than truncating it into one that happens to fit.
    #[test]
    fn a_number_too_large_for_a_port_is_refused_by_name() {
        let error = parse_port("98000").unwrap_err();
        assert!(error.contains("98000"), "{error}");
        assert!(error.contains("65535"), "{error}");
    }

    #[test]
    fn reserved_and_nonsense_ports_are_refused() {
        assert!(parse_port("0").is_err());
        assert!(parse_port("80").is_err());
        assert!(parse_port("1023").is_err());
        assert!(parse_port("").is_err());
        assert!(parse_port("neuf-mille").is_err());
        assert!(parse_port("-1").is_err());
    }

    #[test]
    fn a_usable_port_is_accepted_with_or_without_spaces() {
        assert_eq!(parse_port("9800"), Ok(9800));
        assert_eq!(parse_port("  1024 "), Ok(1024));
        assert_eq!(parse_port("65535"), Ok(65535));
    }

    #[test]
    fn asking_for_a_port_implies_serving_on_one() {
        let (command, options) =
            parse(["mcp".to_owned(), "--port".to_owned(), "1234".to_owned()]).unwrap();
        assert_eq!(command, Command::Mcp);
        assert!(options.http, "--port doit impliquer --http");
        assert_eq!(options.port, Some(1234));
    }

    #[test]
    fn stdio_stays_the_default_for_the_mcp_subcommand() {
        let (command, options) = parse(["mcp".to_owned()]).unwrap();
        assert_eq!(command, Command::Mcp);
        assert!(
            !options.http,
            "un port ne s'ouvre pas sans qu'on le demande"
        );
    }
}
