//! What the window is allowed to ask for.
//!
//! Every command is a thin call into `safe-invest-service` — the same functions
//! the MCP tools use. The page has no other way to reach the engine: the
//! capability file grants it these commands and nothing else, no filesystem, no
//! shell, no arbitrary HTTP.
//!
//! Tauri runs a command declared without `async` on the thread that draws the
//! window. Anything here that reads or writes the disk is therefore declared
//! `#[tauri::command(async)]`: a slow disk, an antivirus scan or a long list of
//! saves must never freeze the interface while it waits.

#![allow(
    clippy::needless_pass_by_value,
    reason = "tauri::State is taken by value; that is the framework's calling convention"
)]

use safe_invest_core::journal;
use safe_invest_core::model::{AssetKind, PlayerKind};
use safe_invest_core::settings::{KEYED_PROVIDERS, Preferences, PreferencesPatch};
use safe_invest_service::view::{AssetView, DashboardView, MarketRow, TradeRow};
use safe_invest_service::{
    BuyRequest, Context, NewGameRequest, SellRequest, ServiceError, TradeSizing, view,
};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use uuid::Uuid;

/// A failure, shaped for the interface: a sentence to show, and sometimes a
/// suggestion of what to do instead.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub message: String,
    pub hint: Option<String>,
}

impl From<safe_invest_core::settings::SettingsError> for CommandError {
    fn from(error: safe_invest_core::settings::SettingsError) -> Self {
        Self {
            message: error.to_string(),
            hint: Some(
                "Vérifiez que le dossier des réglages est accessible en écriture.".to_owned(),
            ),
        }
    }
}

impl From<ServiceError> for CommandError {
    fn from(error: ServiceError) -> Self {
        Self {
            hint: error.hint().map(ToOwned::to_owned),
            message: error.to_string(),
        }
    }
}

type Answer<T> = Result<T, CommandError>;

fn parse_id(value: &str) -> Answer<Uuid> {
    Uuid::parse_str(value.trim()).map_err(|_| CommandError {
        message: "Identifiant de partie invalide.".to_owned(),
        hint: None,
    })
}

fn parse_amount(value: &str) -> Answer<rust_decimal::Decimal> {
    // The page sends money as a string so a float never rounds a cent away
    // between the input box and the engine.
    rust_decimal::Decimal::from_str(value.trim().replace(',', ".").as_str()).map_err(|_| {
        CommandError {
            message: format!("Montant illisible : « {value} »."),
            hint: Some("Utilisez des chiffres, par exemple 1000 ou 1000,50.".to_owned()),
        }
    })
}

fn parse_kind(value: &str) -> Answer<AssetKind> {
    AssetKind::from_str(value).map_err(|_| CommandError {
        message: format!("Type d'actif inconnu : « {value} »."),
        hint: Some("Attendu : crypto, stock ou etf.".to_owned()),
    })
}

// -------------------------------------------------------------- app state

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub data_dir: String,
    /// This executable's own path, so the settings screen can hand over an MCP
    /// configuration block that is already correct rather than a placeholder.
    pub exe_path: Option<String>,
    pub demo_mode: bool,
    /// The tools an AI would be given. Read from the MCP crate, never retyped.
    pub mcp_tools: Vec<String>,
}

#[tauri::command(async)]
pub fn app_info(context: tauri::State<'_, Context>) -> AppInfo {
    AppInfo {
        version: safe_invest_core::VERSION.to_owned(),
        data_dir: context.store().paths().root().display().to_string(),
        exe_path: std::env::current_exe()
            .ok()
            .map(|path| path.display().to_string()),
        demo_mode: context.settings().force_simulated_mode,
        mcp_tools: safe_invest_mcp::server::TOOL_NAMES
            .iter()
            .map(|name| (*name).to_owned())
            .collect(),
    }
}

// ------------------------------------------------------------------ games

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameCard {
    pub id: String,
    pub player_name: String,
    pub player_kind: PlayerKind,
    pub by_ai: bool,
    pub currency: String,
    pub cash: String,
    pub starting_cash: String,
    pub holding_count: usize,
    pub trade_count: usize,
    pub updated_at: String,
    pub has_goal: bool,
    /// Finished games open on their summary rather than on the portfolio.
    pub finished: bool,
    pub end_reason_label: Option<String>,
}

#[tauri::command(async)]
pub fn list_games(context: tauri::State<'_, Context>) -> Vec<GameCard> {
    context
        .list_games()
        .into_iter()
        .map(|game| GameCard {
            id: game.id.to_string(),
            by_ai: game.player_kind == PlayerKind::Ai,
            player_name: game.player_name,
            player_kind: game.player_kind,
            cash: view::money(game.cash, &game.currency),
            starting_cash: view::money(game.starting_cash, &game.currency),
            currency: game.currency,
            holding_count: game.holding_count,
            trade_count: game.trade_count,
            updated_at: view::datetime(game.updated_at),
            has_goal: game.goal.is_some(),
            finished: game.outcome.is_some(),
            end_reason_label: game.outcome.map(|o| o.reason.label().to_owned()),
        })
        .collect()
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewGameArgs {
    pub player_name: String,
    pub player_kind: String,
    pub starting_cash: String,
    pub currency: Option<String>,
    pub fee_percent: Option<String>,
    pub target_amount: Option<String>,
    pub deadline: Option<String>,
}

#[tauri::command(async)]
pub fn create_game(context: tauri::State<'_, Context>, args: NewGameArgs) -> Answer<String> {
    let player_kind = PlayerKind::from_str(&args.player_kind).map_err(|_| CommandError {
        message: "Indiquez qui joue : une personne ou une IA.".to_owned(),
        hint: None,
    })?;

    let deadline = args
        .deadline
        .as_deref()
        .filter(|d| !d.trim().is_empty())
        .map(parse_deadline)
        .transpose()?;

    let target_amount = args
        .target_amount
        .as_deref()
        .filter(|a| !a.trim().is_empty())
        .map(parse_amount)
        .transpose()?;

    let session = context.create_game(
        NewGameRequest {
            player_name: args.player_name,
            player_kind,
            starting_cash: parse_amount(&args.starting_cash)?,
            currency: args.currency,
            // An emptied fee box means "no fees", not a malformed number.
            fee_percent: args
                .fee_percent
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(parse_amount)
                .transpose()?,
            target_amount,
            deadline,
        },
        jiff::Timestamp::now(),
    )?;

    Ok(session.id.to_string())
}

fn parse_deadline(value: &str) -> Answer<jiff::Timestamp> {
    // The date input yields `YYYY-MM-DD`; the deadline is the end of that day.
    value
        .trim()
        .parse::<jiff::civil::Date>()
        .ok()
        .and_then(|date| {
            date.to_datetime(jiff::civil::time(23, 59, 59, 0))
                .in_tz("UTC")
                .ok()
        })
        .map(|zoned| zoned.timestamp())
        .or_else(|| value.trim().parse::<jiff::Timestamp>().ok())
        .ok_or_else(|| CommandError {
            message: format!("Date illisible : « {value} »."),
            hint: Some("Attendu : 2027-12-31.".to_owned()),
        })
}

/// Checks that a game can be opened before the page switches to it.
///
/// Nothing is recorded: the page keeps the id and names it in every call. The
/// remembered "current game" belongs to the MCP side, and opening a game here
/// must not move an AI that is playing another one.
#[tauri::command(async)]
pub fn open_game(context: tauri::State<'_, Context>, game_id: String) -> Answer<()> {
    context.load_game(parse_id(&game_id)?)?;
    Ok(())
}

#[tauri::command(async)]
pub fn delete_game(context: tauri::State<'_, Context>, game_id: String) -> Answer<()> {
    context.delete_game(parse_id(&game_id)?)?;
    Ok(())
}

// -------------------------------------------------------------- dashboard

#[tauri::command]
pub async fn dashboard(
    context: tauri::State<'_, Context>,
    game_id: String,
) -> Answer<DashboardView> {
    let report = context
        .portfolio(parse_id(&game_id)?, jiff::Timestamp::now())
        .await?;
    Ok(view::dashboard(&report))
}

/// The trades, plus the two figures that head the history screen.
///
/// The volume is summed here rather than in the window for the same reason
/// every other figure is: money arithmetic belongs where it can be checked,
/// and a front end adding up floating-point euros will eventually be a cent
/// out in a way nobody can explain.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryView {
    pub trades: Vec<TradeRow>,
    pub count: usize,
    pub volume: String,
    /// The date of the oldest operation shown, absent when there are none.
    pub since: Option<String>,
}

#[tauri::command(async)]
pub fn history(
    context: tauri::State<'_, Context>,
    game_id: String,
    limit: Option<usize>,
) -> Answer<HistoryView> {
    let id = parse_id(&game_id)?;
    let session = context.load_game(id)?;
    let trades = context.trade_history(id, limit)?;

    let volume = trades
        .iter()
        .fold(rust_decimal::Decimal::ZERO, |sum, trade| {
            safe_invest_core::money::add(sum, trade.total).unwrap_or(sum)
        });

    Ok(HistoryView {
        count: trades.len(),
        volume: view::money(volume, &session.currency),
        // `trade_history` hands back the most recent first, so the oldest is
        // the last row — and with a limit it is the oldest *shown*, which is
        // what the sentence beside it claims.
        since: trades.last().map(|trade| view::date(trade.timestamp)),
        trades: trades
            .iter()
            .map(|trade| view::trade(trade, &session.currency))
            .collect(),
    })
}

/// Ends the game at the value it has right now.
///
/// Allowed on an AI game too: the person watching is the one supervising it,
/// and stopping an AI that has gone astray is theirs to decide.
#[tauri::command]
pub async fn end_game(
    context: tauri::State<'_, Context>,
    game_id: String,
) -> Answer<DashboardView> {
    let id = parse_id(&game_id)?;
    let now = jiff::Timestamp::now();
    context.end_game(id, PlayerKind::Human, now).await?;
    Ok(view::dashboard(&context.portfolio(id, now).await?))
}

/// What a finished game amounted to. Refuses a game still in play.
#[tauri::command(async)]
pub fn summary(context: tauri::State<'_, Context>, game_id: String) -> Answer<view::SummaryView> {
    let id = parse_id(&game_id)?;
    let session = context.load_game(id)?;
    let summary = context.summary(id)?;
    Ok(view::summary(&session, &summary))
}

// ----------------------------------------------------------------- market

/// The currency a screen quotes in: the open game's, else the default.
fn currency_of(context: &Context, game_id: Option<&str>) -> Answer<String> {
    match game_id.filter(|id| !id.trim().is_empty()) {
        Some(id) => Ok(context.load_game(parse_id(id)?)?.currency),
        None => Ok(context.settings().default_currency),
    }
}

#[tauri::command]
pub async fn market(
    context: tauri::State<'_, Context>,
    query: String,
    kind: Option<String>,
    game_id: Option<String>,
) -> Answer<Vec<MarketRow>> {
    let kind = kind
        .as_deref()
        .filter(|k| !k.is_empty() && *k != "all")
        .map(parse_kind)
        .transpose()?;

    let currency = currency_of(&context, game_id.as_deref())?;

    let assets = if query.trim().is_empty() {
        context.popular_assets(kind)
    } else {
        context.search_assets(&query, kind).await
    };

    // Quoting forty search hits would burn a free tier in one keystroke.
    let shown: Vec<_> = assets.into_iter().take(24).collect();
    let quotes = context.quotes(&shown, &currency).await;

    Ok(shown
        .iter()
        .map(|asset| view::market_row(asset, quotes.get(&asset.key()), &currency))
        .collect())
}

/// One asset's page: price, recent shape, what is already held, and a sentence
/// on what this kind of asset even is.
#[tauri::command]
pub async fn asset(
    context: tauri::State<'_, Context>,
    symbol: String,
    kind: String,
    days: Option<u16>,
    game_id: Option<String>,
) -> Answer<AssetView> {
    let game = game_id
        .as_deref()
        .filter(|id| !id.trim().is_empty())
        .map(parse_id)
        .transpose()?;
    let report = context
        .asset_report(parse_kind(&kind)?, &symbol, days.unwrap_or(30), game)
        .await?;
    Ok(view::asset_view(&report))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sparkline {
    pub symbol: String,
    /// Closes, oldest first, as plain numbers for the SVG to scale.
    pub points: Vec<f64>,
    pub currency: String,
}

#[tauri::command]
pub async fn price_history(
    context: tauri::State<'_, Context>,
    symbol: String,
    kind: String,
    days: Option<u16>,
    game_id: Option<String>,
) -> Answer<Sparkline> {
    use rust_decimal::prelude::ToPrimitive;

    let kind = parse_kind(&kind)?;
    let asset = context.resolve_asset(kind, &symbol)?;
    let currency = currency_of(&context, game_id.as_deref())?;

    let points = context
        .price_history(&asset, days.unwrap_or(30).clamp(1, 365), &currency)
        .await;

    Ok(Sparkline {
        symbol: asset.symbol,
        points: points.iter().filter_map(|p| p.price.to_f64()).collect(),
        currency,
    })
}

// ---------------------------------------------------------------- trading

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderArgs {
    pub game_id: String,
    pub symbol: String,
    pub kind: String,
    pub quantity: Option<String>,
    pub amount: Option<String>,
    #[serde(default)]
    pub all: bool,
    pub rationale: Option<String>,
}

impl OrderArgs {
    fn sizing(&self) -> Answer<TradeSizing> {
        let quantity = self
            .quantity
            .as_deref()
            .filter(|v| !v.trim().is_empty())
            .map(parse_amount)
            .transpose()?;
        let amount = self
            .amount
            .as_deref()
            .filter(|v| !v.trim().is_empty())
            .map(parse_amount)
            .transpose()?;

        Ok(TradeSizing::from_options(quantity, amount, self.all)?)
    }
}

#[tauri::command]
pub async fn buy(context: tauri::State<'_, Context>, args: OrderArgs) -> Answer<TradeRow> {
    let sizing = args.sizing()?;
    let id = parse_id(&args.game_id)?;
    let session = context.load_game(id)?;

    let trade = context
        .buy(
            BuyRequest {
                game_id: id,
                actor: PlayerKind::Human,
                symbol: args.symbol,
                kind: parse_kind(&args.kind)?,
                sizing,
                rationale: args.rationale,
            },
            jiff::Timestamp::now(),
        )
        .await?;

    Ok(view::trade(&trade, &session.currency))
}

#[tauri::command]
pub async fn sell(context: tauri::State<'_, Context>, args: OrderArgs) -> Answer<TradeRow> {
    let sizing = args.sizing()?;
    let id = parse_id(&args.game_id)?;
    let session = context.load_game(id)?;

    let trade = context
        .sell(
            SellRequest {
                game_id: id,
                actor: PlayerKind::Human,
                symbol: args.symbol,
                kind: parse_kind(&args.kind)?,
                sizing,
                rationale: args.rationale,
            },
            jiff::Timestamp::now(),
        )
        .await?;

    Ok(view::trade(&trade, &session.currency))
}

// --------------------------------------------------------------- settings

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    /// What is on disk — the screen edits this, never the overridden copy.
    /// No secret is in it, sealed or otherwise.
    pub settings: Preferences,
    /// Which providers have a key stored — never the key itself.
    pub configured_keys: Vec<String>,
    /// True when `--demo` forces the simulator whatever the file says.
    pub demo_forced: bool,
}

#[tauri::command(async)]
pub fn get_settings(context: tauri::State<'_, Context>) -> SettingsView {
    let settings = context.stored_settings();
    let configured = KEYED_PROVIDERS
        .iter()
        .filter(|id| context.settings_service().api_key(&settings, id).is_some())
        .map(|id| (*id).to_owned())
        .collect();

    SettingsView {
        settings: settings.preferences(),
        configured_keys: configured,
        demo_forced: context.is_demo_forced(),
    }
}

/// Applies what changed on the settings screen — only that.
#[tauri::command]
pub async fn save_settings(
    context: tauri::State<'_, Context>,
    port: tauri::State<'_, crate::mcp_port::McpPort>,
    change: PreferencesPatch,
) -> Answer<crate::mcp_port::PortStatus> {
    context.update_preferences(change).await?;
    // The port follows the setting immediately: a toggle that only takes
    // effect at the next launch is a toggle nobody trusts.
    Ok(port.reconcile(&context).await)
}

/// What the MCP port is doing, and everything a client needs to reach it.
///
/// The token is returned here — unlike an API key, which is somebody else's
/// secret and is never read back. This one is ours, it is useless anywhere but
/// this machine, and a person cannot configure a client without seeing it.
#[tauri::command(async)]
pub fn mcp_access(
    context: tauri::State<'_, Context>,
    port: tauri::State<'_, crate::mcp_port::McpPort>,
) -> McpAccess {
    let settings = context.stored_settings();
    McpAccess {
        status: port.status(),
        enabled: settings.mcp_http_enabled,
        configured_port: settings.mcp_http_port,
        token: context.settings_service().mcp_token(&settings),
        exe_path: std::env::current_exe()
            .ok()
            .map(|path| path.display().to_string()),
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAccess {
    pub status: crate::mcp_port::PortStatus,
    pub enabled: bool,
    pub configured_port: u16,
    /// Absent until the server has been switched on once.
    pub token: Option<String>,
    pub exe_path: Option<String>,
}

/// Mints a new token, which immediately locks out anything holding the old one.
#[tauri::command]
pub async fn regenerate_mcp_token(
    context: tauri::State<'_, Context>,
    port: tauri::State<'_, crate::mcp_port::McpPort>,
) -> Answer<String> {
    let token = context.settings_service().regenerate_mcp_token()?;
    // The running server still holds the old token in memory, so it has to be
    // restarted or the new one would not work until the next launch.
    port.restart(&context).await;
    Ok(token)
}

/// Stores an API key. There is deliberately no command to read one back:
/// showing a stored secret has no use and only creates a way to leak it.
#[tauri::command]
pub async fn set_api_key(
    context: tauri::State<'_, Context>,
    provider_id: String,
    key: String,
) -> Answer<()> {
    context.set_api_key(&provider_id, &key).await?;
    Ok(())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceRow {
    pub id: String,
    pub label: String,
    pub kinds: Vec<String>,
    pub configured: bool,
    pub is_simulated: bool,
    pub healthy: Option<bool>,
    pub detail: Option<String>,
    pub last_used: Option<String>,
}

#[tauri::command]
pub async fn market_sources(context: tauri::State<'_, Context>) -> Answer<Vec<SourceRow>> {
    let rows = context
        .market_sources()
        .await
        .into_iter()
        .map(|status| SourceRow {
            id: status.id,
            label: status.label,
            kinds: status.kinds.iter().map(|k| k.as_str().to_owned()).collect(),
            configured: status.configured,
            is_simulated: status.is_simulated,
            healthy: status.healthy,
            detail: status.detail,
            last_used: status.last_used.map(view::datetime),
        });
    Ok(rows.collect())
}

/// Opens the data directory in the system file manager.
#[tauri::command]
pub fn open_data_dir(app: tauri::AppHandle, context: tauri::State<'_, Context>) -> Answer<()> {
    use tauri_plugin_opener::OpenerExt as _;

    let path = context.store().paths().root().display().to_string();
    app.opener()
        .open_path(path, None::<&str>)
        .map_err(|error| CommandError {
            message: format!("Impossible d'ouvrir le dossier : {error}"),
            hint: None,
        })
}

/* -------------------------------------------------------------- journal */

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JournalView {
    pub path: String,
    pub lines: Vec<String>,
    pub bytes: u64,
}

/// The tail of the diagnostic journal, for the settings screen to show.
///
/// Capped here rather than trusting the caller: a journal is a megabyte, and
/// pushing all of it through the bridge to draw a panel nobody scrolls would
/// be a waste on every visit.
#[tauri::command(async)]
pub fn read_journal(context: tauri::State<'_, Context>, lines: Option<usize>) -> JournalView {
    let paths = context.store().paths();
    let limit = lines.unwrap_or(300).min(2_000);

    JournalView {
        path: journal::file(paths).display().to_string(),
        lines: journal::tail(paths, limit),
        bytes: journal::size(paths),
    }
}

/// Writes the whole journal to one file and says where it landed.
///
/// It goes to the desktop when there is one. A bug report is written by
/// somebody who then has to find the file to attach it, and a path inside
/// `%LOCALAPPDATA%` is not somewhere people find things.
#[tauri::command(async)]
pub fn export_journal(
    app: tauri::AppHandle,
    context: tauri::State<'_, Context>,
) -> Answer<ExportedJournal> {
    use tauri_plugin_opener::OpenerExt as _;

    let paths = context.store().paths();
    let destination =
        safe_invest_core::paths::desktop_dir().unwrap_or_else(|| paths.root().to_path_buf());

    let exported =
        journal::export(paths, &destination, &export_header(context.inner())).map_err(|error| {
            CommandError {
                message: format!("Impossible d'écrire le journal : {error}"),
                hint: Some("Vérifiez l'espace disque disponible.".to_owned()),
            }
        })?;

    // Reveal it rather than open it: the file is meant to be attached to a
    // message, not read on screen.
    let _ = app.opener().reveal_item_in_dir(&exported).or_else(|_| {
        app.opener()
            .open_path(destination.display().to_string(), None::<&str>)
    });

    Ok(ExportedJournal {
        path: exported.display().to_string(),
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportedJournal {
    pub path: String,
}

/// What the journal is worth nothing without: which build, on which machine.
fn export_header(context: &Context) -> String {
    let settings = context.settings();
    format!(
        "Safe Invest {version} — journal exporté le {when}\n\
         système : {os} {arch}\n\
         dossier : {dir}\n\
         mode    : {mode}\n\
         port MCP: {port}",
        version = safe_invest_core::VERSION,
        when = jiff::Zoned::now().strftime("%d/%m/%Y à %H:%M"),
        os = std::env::consts::OS,
        arch = std::env::consts::ARCH,
        dir = context.store().paths().root().display(),
        mode = if settings.force_simulated_mode {
            "démonstration (cours simulés)"
        } else {
            "cours réels"
        },
        port = if settings.mcp_http_enabled {
            format!("activé sur 127.0.0.1:{}", settings.mcp_http_port)
        } else {
            "désactivé".to_owned()
        },
    )
}

/// Records something the interface could not do, so the journal holds both
/// halves of the program rather than only the Rust one.
#[tauri::command]
pub fn log_ui_error(message: String) {
    // Bounded: a loop of failing renders must not be able to fill the journal
    // with one enormous line.
    let message: String = message.chars().take(500).collect();
    tracing::error!(target: "safe_invest::ui", "{message}");
}
