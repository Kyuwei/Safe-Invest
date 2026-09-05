//! What the window is allowed to ask for.
//!
//! Every command is a thin call into `safe-invest-service` — the same functions
//! the MCP tools use. The page has no other way to reach the engine: the
//! capability file grants it these commands and nothing else, no filesystem, no
//! shell, no arbitrary HTTP.

#![allow(
    clippy::needless_pass_by_value,
    reason = "tauri::State is taken by value; that is the framework's calling convention"
)]

use safe_invest_core::model::{AssetKind, PlayerKind};
use safe_invest_core::settings::AppSettings;
use safe_invest_service::view::{DashboardView, MarketRow, TradeRow};
use safe_invest_service::{
    BuyRequest, Context, NewGameRequest, SellRequest, ServiceError, SetGoalRequest, TradeSizing,
    view,
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
    pub demo_mode: bool,
    pub current_game_id: Option<String>,
}

#[tauri::command]
pub fn app_info(context: tauri::State<'_, Context>) -> AppInfo {
    AppInfo {
        version: safe_invest_core::VERSION.to_owned(),
        data_dir: context.store().paths().root().display().to_string(),
        demo_mode: context.settings().force_simulated_mode,
        current_game_id: context.current_game_id().map(|id| id.to_string()),
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
}

#[tauri::command]
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

#[tauri::command]
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
            fee_percent: args.fee_percent.as_deref().map(parse_amount).transpose()?,
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

#[tauri::command]
pub fn open_game(context: tauri::State<'_, Context>, game_id: String) -> Answer<()> {
    context.open_game(parse_id(&game_id)?)?;
    Ok(())
}

#[tauri::command]
pub fn delete_game(context: tauri::State<'_, Context>, game_id: String) -> Answer<()> {
    context.delete_game(parse_id(&game_id)?)?;
    Ok(())
}

#[tauri::command]
pub fn set_goal(
    context: tauri::State<'_, Context>,
    target_amount: String,
    deadline: String,
) -> Answer<()> {
    context.set_goal(
        &SetGoalRequest {
            game_id: None,
            target_amount: parse_amount(&target_amount)?,
            deadline: parse_deadline(&deadline)?,
        },
        jiff::Timestamp::now(),
    )?;
    Ok(())
}

// -------------------------------------------------------------- dashboard

#[tauri::command]
pub async fn dashboard(context: tauri::State<'_, Context>) -> Answer<DashboardView> {
    let report = context.portfolio(None, jiff::Timestamp::now()).await?;
    Ok(view::dashboard(&report))
}

#[tauri::command]
pub fn history(context: tauri::State<'_, Context>, limit: Option<usize>) -> Answer<Vec<TradeRow>> {
    let session = context.load_game(None)?;
    let trades = context.trade_history(None, limit)?;
    Ok(trades
        .iter()
        .map(|trade| view::trade(trade, &session.currency))
        .collect())
}

// ----------------------------------------------------------------- market

#[tauri::command]
pub async fn market(
    context: tauri::State<'_, Context>,
    query: String,
    kind: Option<String>,
) -> Answer<Vec<MarketRow>> {
    let kind = kind
        .as_deref()
        .filter(|k| !k.is_empty() && *k != "all")
        .map(parse_kind)
        .transpose()?;

    let currency = context
        .load_game(None)
        .map_or_else(|_| context.settings().default_currency, |g| g.currency);

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
) -> Answer<Sparkline> {
    use rust_decimal::prelude::ToPrimitive;

    let kind = parse_kind(&kind)?;
    let asset = context.resolve_asset(kind, &symbol)?;
    let currency = context
        .load_game(None)
        .map_or_else(|_| context.settings().default_currency, |g| g.currency);

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
    let session = context.load_game(None)?;

    let trade = context
        .buy(
            BuyRequest {
                game_id: None,
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
    let session = context.load_game(None)?;

    let trade = context
        .sell(
            SellRequest {
                game_id: None,
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
    pub settings: AppSettings,
    /// Which providers have a key stored — never the key itself.
    pub configured_keys: Vec<String>,
}

#[tauri::command]
pub fn get_settings(context: tauri::State<'_, Context>) -> SettingsView {
    let settings = context.settings();
    let configured = ["coingecko", "coinmarketcap", "finnhub"]
        .into_iter()
        .filter(|id| context.settings_service().api_key(&settings, id).is_some())
        .map(ToOwned::to_owned)
        .collect();

    SettingsView {
        settings,
        configured_keys: configured,
    }
}

#[tauri::command]
pub async fn save_settings(
    context: tauri::State<'_, Context>,
    settings: AppSettings,
) -> Answer<()> {
    context.save_settings(&settings)?;
    context.reload_market().await?;
    Ok(())
}

/// Stores an API key. There is deliberately no command to read one back:
/// showing a stored secret has no use and only creates a way to leak it.
#[tauri::command]
pub async fn set_api_key(
    context: tauri::State<'_, Context>,
    provider_id: String,
    key: String,
) -> Answer<()> {
    context.set_api_key(&provider_id, &key)?;
    context.reload_market().await?;
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
