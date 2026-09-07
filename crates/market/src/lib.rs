//! Real market data for Safe Invest.
//!
//! The design rule of this crate is that a number always says where it came
//! from. Every [`Quote`](safe_invest_core::model::Quote) carries a `source_id`
//! and an `is_simulated` flag, and both travel unchanged to the badge the
//! player sees. An educational tool that let an invented price pass for a real
//! one would be teaching the wrong thing.

pub mod cache;
pub mod catalog;
pub mod error;
pub mod fx;
pub mod http;
pub mod providers;
pub mod ratelimit;
pub mod service;

pub use error::{ProviderError, ProviderResult};
pub use providers::{PricePoint, QuoteProvider};
pub use service::{ChainOptions, MarketDataService, ProviderStatus, QuoteBatch};
