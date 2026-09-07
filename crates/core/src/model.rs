//! The vocabulary of the game. Everything the app, the MCP server and the
//! on-disk save file agree on lives here.

use jiff::Timestamp;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

/// What kind of instrument an asset is. Drives which quote providers are tried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AssetKind {
    Crypto,
    Stock,
    Etf,
}

impl AssetKind {
    pub const ALL: [Self; 3] = [Self::Crypto, Self::Stock, Self::Etf];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Crypto => "crypto",
            Self::Stock => "stock",
            Self::Etf => "etf",
        }
    }

    /// Stocks and ETFs quote through the same providers; crypto does not.
    pub fn is_equity(self) -> bool {
        matches!(self, Self::Stock | Self::Etf)
    }
}

impl fmt::Display for AssetKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for AssetKind {
    type Err = UnknownVariant;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "crypto" => Ok(Self::Crypto),
            "stock" | "action" => Ok(Self::Stock),
            "etf" => Ok(Self::Etf),
            _ => Err(UnknownVariant),
        }
    }
}

/// Who is at the controls. An AI game is observation-only in the UI and forces
/// a written justification on every trade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlayerKind {
    Human,
    Ai,
}

impl PlayerKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::Ai => "ai",
        }
    }
}

impl std::str::FromStr for PlayerKind {
    type Err = UnknownVariant;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "human" | "humain" | "personne" => Ok(Self::Human),
            "ai" | "ia" => Ok(Self::Ai),
            _ => Err(UnknownVariant),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TradeSide {
    Buy,
    Sell,
}

impl TradeSide {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Buy => "buy",
            Self::Sell => "sell",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GoalStatus {
    Achieved,
    OnTrack,
    Behind,
    Expired,
}

/// Why a game stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EndReason {
    /// The target was reached, at least for the moment it was measured.
    GoalReached,
    /// The deadline went by with the target still out of reach.
    DeadlinePassed,
    /// Somebody decided they were done.
    Stopped,
}

impl EndReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GoalReached => "goalReached",
            Self::DeadlinePassed => "deadlinePassed",
            Self::Stopped => "stopped",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::GoalReached => "Objectif atteint",
            Self::DeadlinePassed => "Date limite dépassée",
            Self::Stopped => "Partie terminée",
        }
    }
}

/// How a game ended, written once and never recomputed.
///
/// `final_value` is the portfolio's worth at the instant the game stopped, kept
/// verbatim. Recomputing it when the summary is next opened would quietly
/// rewrite the result — a game "won" at 25 000 € would show 23 800 € a week
/// later because the market moved on without it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Outcome {
    pub ended_at: Timestamp,
    pub reason: EndReason,
    pub final_value: Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("valeur inconnue")]
pub struct UnknownVariant;

/// A tradable instrument. `symbol` is the user-facing ticker; `provider_id` is
/// whatever the winning data source calls it (`bitcoin`, for CoinGecko).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub symbol: String,
    pub name: String,
    pub kind: AssetKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo_url: Option<String>,
}

impl Asset {
    pub fn new(symbol: impl Into<String>, name: impl Into<String>, kind: AssetKind) -> Self {
        Self {
            symbol: Self::normalize(&symbol.into()),
            name: name.into(),
            kind,
            provider_id: None,
            logo_url: None,
        }
    }

    /// The identity used as a map key everywhere: `crypto:BTC`, `stock:MSFT`.
    pub fn key(&self) -> String {
        Self::make_key(self.kind, &self.symbol)
    }

    pub fn make_key(kind: AssetKind, symbol: &str) -> String {
        format!("{}:{}", kind.as_str(), Self::normalize(symbol))
    }

    pub fn normalize(symbol: &str) -> String {
        symbol.trim().to_uppercase()
    }
}

/// A price at a point in time, and — just as important — where it came from.
///
/// `source_id` and `is_simulated` travel with every quote so the UI can always
/// tell the player that a number is a fallback or an outright simulation. On a
/// teaching tool, letting an invented price pass for a real one is the one
/// unforgivable bug.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Quote {
    pub symbol: String,
    pub kind: AssetKind,
    pub price: Decimal,
    pub currency: String,
    pub as_of: Timestamp,
    pub source_id: String,
    #[serde(default)]
    pub is_simulated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub change_percent_24h: Option<Decimal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub market_cap: Option<Decimal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_24h: Option<Decimal>,
}

impl Quote {
    pub fn key(&self) -> String {
        Asset::make_key(self.kind, &self.symbol)
    }

    /// +1 up, -1 down, 0 flat or unknown — the green/red decision, made once.
    pub fn direction(&self) -> i8 {
        direction_of(self.change_percent_24h)
    }
}

/// An open position: how many units, and what they cost on average.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Holding {
    pub asset: Asset,
    pub quantity: Decimal,
    pub average_cost: Decimal,
}

impl Holding {
    /// What the position cost, ignoring today's price.
    pub fn cost_basis(&self) -> Decimal {
        crate::money::mul(self.quantity, self.average_cost).unwrap_or(Decimal::ZERO)
    }
}

/// One buy or sell, kept forever. `rationale` is what makes the AI history
/// readable as a chain of decisions rather than a list of numbers.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trade {
    pub id: Uuid,
    pub timestamp: Timestamp,
    pub side: TradeSide,
    pub asset: Asset,
    pub quantity: Decimal,
    pub unit_price: Decimal,
    pub fees: Decimal,
    /// Cash actually leaving (buy) or entering (sell) the account.
    pub total: Decimal,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub realized_pnl: Option<Decimal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    pub actor_kind: PlayerKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quote_source_id: Option<String>,
    #[serde(default)]
    pub quote_was_simulated: bool,
}

/// One reading of what the whole portfolio was worth.
///
/// The dashboard draws a curve from these. They are recorded by the app as it
/// values the portfolio rather than reconstructed afterwards, because a
/// reconstruction would have to guess at prices nobody wrote down — and a made-up
/// curve is exactly the kind of thing this program must not draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValuePoint {
    pub at: Timestamp,
    pub total_value: Decimal,
}

/// A target amount and the date it has to be reached by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Goal {
    pub target_amount: Decimal,
    pub deadline: Timestamp,
}

/// One game. This is the unit that is saved, watched and shared between the
/// window and the MCP server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameSession {
    pub id: Uuid,
    pub player_name: String,
    pub player_kind: PlayerKind,
    pub currency: String,
    pub starting_cash: Decimal,
    pub cash: Decimal,
    #[serde(default)]
    pub holdings: Vec<Holding>,
    #[serde(default)]
    pub trades: Vec<Trade>,
    /// What the portfolio was worth, over time. Appended by the app; see
    /// [`GameSession::record_value`].
    #[serde(default)]
    pub value_history: Vec<ValuePoint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal: Option<Goal>,
    /// Set once the game is over. Present means read-only, everywhere.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<Outcome>,
    #[serde(default)]
    pub fee_percent: Decimal,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    #[serde(default = "GameSession::current_schema_version")]
    pub schema_version: u32,
}

impl GameSession {
    pub const CURRENT_SCHEMA_VERSION: u32 = 1;

    /// Fifteen minutes between readings.
    pub const VALUE_INTERVAL_SECONDS: i64 = 15 * 60;

    /// Roughly a month of readings at that interval.
    pub const MAX_VALUE_POINTS: usize = 2880;

    fn current_schema_version() -> u32 {
        Self::CURRENT_SCHEMA_VERSION
    }

    /// Whether this game has stopped and can no longer be traded.
    pub fn is_over(&self) -> bool {
        self.outcome.is_some()
    }

    /// Ends the game, keeping the value it had at that moment.
    ///
    /// Ending twice is not an error and does not overwrite: the first ending is
    /// the one that happened, and a second call — a late refresh racing a
    /// manual stop — must not move the finish line afterwards.
    pub fn finish(&mut self, reason: EndReason, final_value: Decimal, at: Timestamp) -> bool {
        if self.outcome.is_some() {
            return false;
        }
        self.outcome = Some(Outcome {
            ended_at: at,
            reason,
            final_value,
        });
        self.updated_at = at;
        true
    }

    /// Sum of every realised gain and loss booked so far.
    pub fn realized_pnl(&self) -> Decimal {
        self.trades
            .iter()
            .filter_map(|t| t.realized_pnl)
            .fold(Decimal::ZERO, |acc, v| acc.checked_add(v).unwrap_or(acc))
    }

    /// Records what the portfolio is worth, if enough time has passed.
    ///
    /// Returns whether anything was added, so a caller can avoid writing the
    /// save file for nothing. Two rules keep the file from growing without
    /// bound: at most one reading per [`Self::VALUE_INTERVAL_SECONDS`], and the
    /// oldest are dropped past [`Self::MAX_VALUE_POINTS`] — a month of readings
    /// at a quarter-hour each.
    pub fn record_value(&mut self, at: Timestamp, total_value: Decimal) -> bool {
        // A finished game's curve is finished too. The holdings are still
        // quoted so the portfolio screen can show what they are worth today,
        // but appending those readings would extend the "trajectory of the
        // game" past the end of the game — inventing history after the fact,
        // which is the one thing this curve exists to avoid.
        if self.is_over() {
            return false;
        }

        if let Some(last) = self.value_history.last() {
            let elapsed = at.as_second().saturating_sub(last.at.as_second());
            if elapsed < Self::VALUE_INTERVAL_SECONDS {
                return false;
            }
        }

        self.value_history.push(ValuePoint { at, total_value });
        if self.value_history.len() > Self::MAX_VALUE_POINTS {
            let excess = self.value_history.len() - Self::MAX_VALUE_POINTS;
            self.value_history.drain(..excess);
        }
        true
    }

    pub fn find_holding(&self, kind: AssetKind, symbol: &str) -> Option<&Holding> {
        let key = Asset::make_key(kind, symbol);
        self.holdings.iter().find(|h| h.asset.key() == key)
    }

    pub fn find_holding_mut(&mut self, kind: AssetKind, symbol: &str) -> Option<&mut Holding> {
        let key = Asset::make_key(kind, symbol);
        self.holdings.iter_mut().find(|h| h.asset.key() == key)
    }

    pub fn summary(&self) -> GameSummary {
        GameSummary {
            id: self.id,
            player_name: self.player_name.clone(),
            player_kind: self.player_kind,
            currency: self.currency.clone(),
            starting_cash: self.starting_cash,
            cash: self.cash,
            holding_count: self.holdings.len(),
            trade_count: self.trades.len(),
            created_at: self.created_at,
            updated_at: self.updated_at,
            goal: self.goal,
            outcome: self.outcome,
        }
    }
}

/// The cheap view of a game, for the "resume a game" list — no need to hand the
/// whole trade history to a menu screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameSummary {
    pub id: Uuid,
    pub player_name: String,
    pub player_kind: PlayerKind,
    pub currency: String,
    pub starting_cash: Decimal,
    pub cash: Decimal,
    pub holding_count: usize,
    pub trade_count: usize,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal: Option<Goal>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<Outcome>,
}

/// A holding priced at the current market. `None` prices mean no source could
/// quote it — shown as "cours indisponible", never as zero.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionView {
    pub asset: Asset,
    pub quantity: Decimal,
    pub average_cost: Decimal,
    pub cost_basis: Decimal,
    pub price: Option<Decimal>,
    pub market_value: Option<Decimal>,
    pub unrealized_pnl: Option<Decimal>,
    pub unrealized_pnl_percent: Option<Decimal>,
    pub change_percent_24h: Option<Decimal>,
    pub source_id: Option<String>,
    #[serde(default)]
    pub is_simulated: bool,
    pub quoted_at: Option<Timestamp>,
    pub weight_percent: Decimal,
}

impl PositionView {
    pub fn direction(&self) -> i8 {
        direction_of(self.unrealized_pnl)
    }
}

/// The whole portfolio, valued at one instant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortfolioSnapshot {
    pub as_of: Timestamp,
    pub currency: String,
    pub cash: Decimal,
    pub starting_cash: Decimal,
    pub market_value: Decimal,
    pub total_value: Decimal,
    pub total_pnl: Decimal,
    pub total_pnl_percent: Decimal,
    pub realized_pnl: Decimal,
    pub unrealized_pnl: Decimal,
    pub positions: Vec<PositionView>,
    #[serde(default)]
    pub contains_simulated_prices: bool,
    #[serde(default)]
    pub unpriced_symbols: Vec<String>,
}

impl PortfolioSnapshot {
    pub fn direction(&self) -> i8 {
        direction_of(Some(self.total_pnl))
    }
}

/// How the game is tracking against its target.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalProgress {
    pub target_amount: Decimal,
    pub deadline: Timestamp,
    pub current_value: Decimal,
    pub starting_cash: Decimal,
    pub progress_percent: Decimal,
    pub amount_remaining: Decimal,
    pub days_remaining: i64,
    pub status: GoalStatus,
    pub required_annualised_return_percent: Option<Decimal>,
    pub achieved_annualised_return_percent: Option<Decimal>,
}

fn direction_of(value: Option<Decimal>) -> i8 {
    match value {
        Some(v) if v.is_sign_positive() && !v.is_zero() => 1,
        Some(v) if v.is_sign_negative() && !v.is_zero() => -1,
        _ => 0,
    }
}
