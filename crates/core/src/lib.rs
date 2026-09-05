//! Safe Invest — domain model, trading engine and shared storage.
//!
//! This crate holds every rule of the game and nothing about how it is drawn.
//! The desktop window and the MCP server both depend on it, which is what
//! guarantees that a human and an AI are playing by the same rules: there is
//! one engine, and neither of them can go around it.

pub mod engine;
pub mod factory;
pub mod goal;
pub mod model;
pub mod money;
pub mod paths;
pub mod secret;
pub mod settings;
pub mod store;
pub mod valuation;
pub mod watcher;

pub use engine::{TradeAmount, TradeError};
pub use model::{
    Asset, AssetKind, GameSession, GameSummary, Goal, GoalProgress, GoalStatus, Holding,
    PlayerKind, PortfolioSnapshot, PositionView, Quote, Trade, TradeSide,
};
pub use paths::Paths;
pub use store::{GameStore, StoreError};

/// The version stamped into the binary, the MCP handshake and the save files.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
