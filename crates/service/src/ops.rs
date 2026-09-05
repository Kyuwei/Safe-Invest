//! The operations themselves.

use crate::context::Context;
use crate::error::{ServiceError, ServiceResult};
use jiff::Timestamp;
use rust_decimal::Decimal;
use safe_invest_core::engine::{self, TradeAmount};
use safe_invest_core::factory::{self, NewGame};
use safe_invest_core::model::{
    Asset, AssetKind, GameSession, GameSummary, Goal, GoalProgress, PlayerKind, PortfolioSnapshot,
    Quote, Trade,
};
use safe_invest_core::settings::AppSettings;
use safe_invest_core::{goal, valuation};
use safe_invest_market::PricePoint;
use safe_invest_market::service::ProviderStatus;
use std::collections::HashMap;
use uuid::Uuid;

/// What "new game" needs. Mirrors the entry screen one for one.
#[derive(Debug, Clone)]
pub struct NewGameRequest {
    pub player_name: String,
    pub player_kind: PlayerKind,
    pub starting_cash: Decimal,
    pub currency: Option<String>,
    pub fee_percent: Option<Decimal>,
    pub target_amount: Option<Decimal>,
    pub deadline: Option<Timestamp>,
}

#[derive(Debug, Clone)]
pub struct SetGoalRequest {
    pub game_id: Option<Uuid>,
    pub target_amount: Decimal,
    pub deadline: Timestamp,
}

/// How much to trade — exactly one of the three.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeSizing {
    Quantity(Decimal),
    Amount(Decimal),
    All,
}

impl TradeSizing {
    /// Builds the sizing from the three optional fields a tool call carries,
    /// refusing the ambiguous combinations rather than picking one.
    pub fn from_options(
        quantity: Option<Decimal>,
        amount: Option<Decimal>,
        all: bool,
    ) -> ServiceResult<Self> {
        match (quantity, amount, all) {
            (Some(q), None, false) => Ok(Self::Quantity(q)),
            (None, Some(a), false) => Ok(Self::Amount(a)),
            (None, None, true) => Ok(Self::All),
            (None, None, false) => Err(ServiceError::rejected(
                "Précisez une quantité (quantity), un montant (amount) ou all=true.",
            )),
            _ => Err(ServiceError::rejected(
                "Précisez une seule façon de dimensionner l'opération : quantity, amount ou all.",
            )),
        }
    }
}

impl From<TradeSizing> for TradeAmount {
    fn from(sizing: TradeSizing) -> Self {
        match sizing {
            TradeSizing::Quantity(q) => Self::Units(q),
            TradeSizing::Amount(a) => Self::Cash(a),
            TradeSizing::All => Self::All,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BuyRequest {
    pub game_id: Option<Uuid>,
    pub symbol: String,
    pub kind: AssetKind,
    pub sizing: TradeSizing,
    pub rationale: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SellRequest {
    pub game_id: Option<Uuid>,
    pub symbol: String,
    pub kind: AssetKind,
    pub sizing: TradeSizing,
    pub rationale: Option<String>,
}

/// A portfolio, its goal, and the quotes it was valued with.
#[derive(Debug, Clone)]
pub struct PortfolioReport {
    pub session: GameSession,
    pub snapshot: PortfolioSnapshot,
    pub goal: Option<GoalProgress>,
}

impl Context {
    // ------------------------------------------------------------- games

    pub fn list_games(&self) -> Vec<GameSummary> {
        self.store().list()
    }

    pub fn current_game_id(&self) -> Option<Uuid> {
        self.store().current_game()
    }

    /// Resolves the game to act on: the one given, else the current one.
    pub fn resolve_game_id(&self, requested: Option<Uuid>) -> ServiceResult<Uuid> {
        requested
            .or_else(|| self.store().current_game())
            .ok_or(ServiceError::NoCurrentGame)
    }

    pub fn load_game(&self, id: Option<Uuid>) -> ServiceResult<GameSession> {
        let id = self.resolve_game_id(id)?;
        Ok(self.store().load(id)?)
    }

    pub fn create_game(
        &self,
        request: NewGameRequest,
        now: Timestamp,
    ) -> ServiceResult<GameSession> {
        let settings = self.settings();
        let goal = match (request.target_amount, request.deadline) {
            (Some(target_amount), Some(deadline)) => Some(Goal {
                target_amount,
                deadline,
            }),
            (None, None) => None,
            _ => {
                return Err(ServiceError::rejected(
                    "Un objectif demande à la fois un montant (target_amount) et une date (deadline).",
                ));
            }
        };

        let session = factory::create(
            NewGame {
                player_name: request.player_name,
                player_kind: request.player_kind,
                currency: request.currency.unwrap_or(settings.default_currency),
                starting_cash: request.starting_cash,
                fee_percent: request.fee_percent.unwrap_or(settings.default_fee_percent),
                goal,
            },
            now,
        )?;

        self.store().save(&session)?;
        self.store().set_current_game(Some(session.id))?;
        Ok(session)
    }

    /// Makes `id` the game every other call acts on by default.
    pub fn open_game(&self, id: Uuid) -> ServiceResult<GameSession> {
        let session = self.store().load(id)?;
        self.store().set_current_game(Some(id))?;
        Ok(session)
    }

    pub fn delete_game(&self, id: Uuid) -> ServiceResult<()> {
        Ok(self.store().delete(id)?)
    }

    pub fn set_goal(&self, request: &SetGoalRequest, now: Timestamp) -> ServiceResult<GameSession> {
        let id = self.resolve_game_id(request.game_id)?;

        self.store().mutate(id, |session| {
            if request.target_amount <= session.starting_cash {
                return Err(ServiceError::rejected(
                    "Le montant à atteindre doit dépasser le capital de départ.",
                ));
            }
            if request.deadline <= now {
                return Err(ServiceError::rejected(
                    "La date limite doit être dans le futur.",
                ));
            }
            session.goal = Some(Goal {
                target_amount: request.target_amount,
                deadline: request.deadline,
            });
            session.updated_at = now;
            Ok(session.clone())
        })
    }

    // --------------------------------------------------------- portfolio

    /// Values a game at the current market.
    pub async fn portfolio(
        &self,
        id: Option<Uuid>,
        now: Timestamp,
    ) -> ServiceResult<PortfolioReport> {
        let session = self.load_game(id)?;
        let assets: Vec<Asset> = session.holdings.iter().map(|h| h.asset.clone()).collect();

        let quotes = if assets.is_empty() {
            HashMap::new()
        } else {
            self.market()
                .await
                .quotes(&assets, &session.currency)
                .await
                .quotes
        };

        let snapshot = valuation::snapshot(&session, &quotes, now);
        let goal = goal::evaluate(&session, &snapshot, now);

        Ok(PortfolioReport {
            session,
            snapshot,
            goal,
        })
    }

    pub async fn goal_progress(
        &self,
        id: Option<Uuid>,
        now: Timestamp,
    ) -> ServiceResult<Option<GoalProgress>> {
        Ok(self.portfolio(id, now).await?.goal)
    }

    /// Trade history, newest first, capped at `limit`.
    pub fn trade_history(
        &self,
        id: Option<Uuid>,
        limit: Option<usize>,
    ) -> ServiceResult<Vec<Trade>> {
        let session = self.load_game(id)?;
        let mut trades = session.trades;
        trades.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        trades.truncate(limit.unwrap_or(usize::MAX));
        Ok(trades)
    }

    // ------------------------------------------------------------ market

    pub async fn search_assets(&self, query: &str, kind: Option<AssetKind>) -> Vec<Asset> {
        self.market().await.search(query, kind).await
    }

    pub fn popular_assets(&self, kind: Option<AssetKind>) -> Vec<Asset> {
        safe_invest_market::catalog::popular(kind)
    }

    pub async fn quotes(&self, assets: &[Asset], currency: &str) -> HashMap<String, Quote> {
        self.market().await.quotes(assets, currency).await.quotes
    }

    pub async fn price_history(&self, asset: &Asset, days: u16, currency: &str) -> Vec<PricePoint> {
        self.market().await.history(asset, days, currency).await
    }

    pub async fn market_sources(&self) -> Vec<ProviderStatus> {
        self.market().await.statuses()
    }

    /// Turns a symbol into a full [`Asset`], recovering the provider id from
    /// the catalogue when it knows the symbol.
    pub fn resolve_asset(&self, kind: AssetKind, symbol: &str) -> ServiceResult<Asset> {
        let symbol = symbol.trim();
        if symbol.is_empty() {
            return Err(ServiceError::UnknownAsset {
                query: symbol.to_owned(),
            });
        }

        Ok(safe_invest_market::catalog::lookup(kind, symbol)
            .unwrap_or_else(|| Asset::new(symbol, symbol, kind)))
    }

    // ----------------------------------------------------------- trading

    pub async fn buy(&self, request: BuyRequest, now: Timestamp) -> ServiceResult<Trade> {
        let id = self.resolve_game_id(request.game_id)?;
        let session = self.store().load(id)?;
        let asset = self.resolve_asset(request.kind, &request.symbol)?;
        let quote = self.quote_for(&asset, &session.currency).await?;

        self.store().mutate(id, |session| {
            engine::buy(
                session,
                &asset,
                &quote,
                request.sizing.into(),
                request.rationale.as_deref(),
                now,
            )
            .map_err(ServiceError::from)
        })
    }

    pub async fn sell(&self, request: SellRequest, now: Timestamp) -> ServiceResult<Trade> {
        let id = self.resolve_game_id(request.game_id)?;
        let session = self.store().load(id)?;
        let asset = self.resolve_asset(request.kind, &request.symbol)?;
        let quote = self.quote_for(&asset, &session.currency).await?;

        self.store().mutate(id, |session| {
            engine::sell(
                session,
                &asset,
                &quote,
                request.sizing.into(),
                request.rationale.as_deref(),
                now,
            )
            .map_err(ServiceError::from)
        })
    }

    /// One quote, or a refusal. Trading on a price nobody could produce is
    /// exactly the case where guessing would be worst.
    async fn quote_for(&self, asset: &Asset, currency: &str) -> ServiceResult<Quote> {
        let quotes = self.quotes(std::slice::from_ref(asset), currency).await;
        quotes
            .get(&asset.key())
            .cloned()
            .ok_or_else(|| ServiceError::NoQuote {
                symbol: asset.symbol.clone(),
            })
    }

    // ---------------------------------------------------------- settings

    pub fn save_settings(&self, settings: &AppSettings) -> ServiceResult<()> {
        self.settings_service()
            .save(settings)
            .map_err(|e| ServiceError::Storage(e.to_string()))
    }

    /// Stores an API key, sealed. Returns nothing: a stored secret is never
    /// read back out to a caller.
    pub fn set_api_key(&self, provider_id: &str, key: &str) -> ServiceResult<()> {
        self.settings_service()
            .set_api_key(provider_id, key)
            .map_err(|e| ServiceError::Storage(e.to_string()))
    }
}
