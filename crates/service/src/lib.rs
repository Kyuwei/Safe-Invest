//! Everything the application can *do*, in one place.
//!
//! The window and the MCP server are two faces of this crate, and neither has
//! a private path to the engine. That is what makes the brief's promise true:
//! a human and an AI play the same game, under the same rules, against the same
//! files — because they call the same functions.
//!
//! Nothing here knows about windows, JSON-RPC or HTML. Errors come back as
//! [`ServiceError`], with a sentence meant for the player and a hint meant for
//! whoever is driving.

mod context;
mod error;
mod ops;
pub mod view;

pub use context::{Context, ContextConfig};
pub use error::{ServiceError, ServiceResult};
pub use ops::{BuyRequest, NewGameRequest, SellRequest, SetGoalRequest, TradeSizing};
