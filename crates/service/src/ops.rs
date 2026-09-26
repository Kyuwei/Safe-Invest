//! The operations themselves.

use crate::context::Context;
use crate::error::{ServiceError, ServiceResult};
use jiff::Timestamp;
use rust_decimal::Decimal;
use safe_invest_core::engine::{self, TradeAmount};
use safe_invest_core::factory::{self, NewGame};
use safe_invest_core::model::{
    Asset, AssetKind, EndReason, GameSession, GameSummary, Goal, GoalProgress, GoalStatus,
    PlayerKind, PortfolioSnapshot, Quote, Trade,
};
use safe_invest_core::settings::{PreferencesPatch, SettingsError};
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
    pub game_id: Uuid,
    /// Who asks. Only the player a game belongs to may change its goal.
    pub actor: PlayerKind,
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
    pub game_id: Uuid,
    /// Who places the order: the window trades as a person, MCP as an AI.
    pub actor: PlayerKind,
    pub symbol: String,
    pub kind: AssetKind,
    pub sizing: TradeSizing,
    pub rationale: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SellRequest {
    pub game_id: Uuid,
    /// Who places the order: the window trades as a person, MCP as an AI.
    pub actor: PlayerKind,
    pub symbol: String,
    pub kind: AssetKind,
    pub sizing: TradeSizing,
    pub rationale: Option<String>,
}

/// Everything the asset screen shows: the price, its recent shape, and what
/// the player already holds of it.
#[derive(Debug, Clone)]
pub struct AssetReport {
    pub asset: Asset,
    pub quote: Option<Quote>,
    pub history: Vec<PricePoint>,
    pub history_days: u16,
    pub position: Option<safe_invest_core::model::Holding>,
    pub currency: String,
    pub cash: Decimal,
    pub fee_percent: Decimal,
    pub observer_mode: bool,
    /// The game is over: nothing here can be bought or sold any more.
    pub finished: bool,
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

    /// The game an AI last opened, as remembered on disk between two runs of
    /// the MCP server.
    ///
    /// Only the MCP side reads or writes it. The window names the game it shows
    /// in every call instead: were the two to share one "current game", a
    /// person opening their own portfolio would silently move the AI onto it,
    /// and the AI opening its game would send the person's clicks there.
    pub fn remembered_ai_game(&self) -> Option<Uuid> {
        self.store().current_game()
    }

    /// Records the game an AI is now playing, for the next run to resume.
    pub fn remember_ai_game(&self, id: Uuid) -> ServiceResult<()> {
        Ok(self.store().set_current_game(Some(id))?)
    }

    pub fn load_game(&self, id: Uuid) -> ServiceResult<GameSession> {
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
        Ok(session)
    }

    pub fn delete_game(&self, id: Uuid) -> ServiceResult<()> {
        Ok(self.store().delete(id)?)
    }

    pub fn set_goal(&self, request: &SetGoalRequest, now: Timestamp) -> ServiceResult<GameSession> {
        self.store().mutate(request.game_id, |session| {
            engine::validate_actor(session, request.actor)?;
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
    pub async fn portfolio(&self, id: Uuid, now: Timestamp) -> ServiceResult<PortfolioReport> {
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
        let trustworthy = self.is_trustworthy(&snapshot).await;

        // A goal that has been met, or a deadline that has gone by, ends the
        // game here — at the valuation that decided it. Waiting for someone to
        // press a button would let the recorded result drift away from the one
        // that was actually reached.
        //
        // But only on a valuation worth keeping. A line no source could price
        // drops out of the total, and a fallback to the simulator invents its
        // price; freezing either as the result of the game — or drawing it on
        // the curve — would write down a number that never existed. The next
        // complete valuation decides instead.
        let ending =
            goal.as_ref()
                .filter(|_| trustworthy)
                .and_then(|progress| match progress.status {
                    GoalStatus::Achieved => Some(EndReason::GoalReached),
                    GoalStatus::Expired => Some(EndReason::DeadlinePassed),
                    GoalStatus::OnTrack | GoalStatus::Behind => None,
                });

        // The curve is recorded as the portfolio is valued, never reconstructed
        // afterwards: a reconstruction would have to invent prices nobody wrote
        // down. `record_value` keeps at most one reading a quarter-hour, and the
        // save file is only rewritten when it actually kept one — or when the
        // game just ended, which is always worth a write.
        let mut session = session;
        let updated = self
            .store()
            .mutate_if(session.id, |stored| {
                let kept = trustworthy && stored.record_value(now, snapshot.total_value);
                let ended =
                    ending.is_some_and(|reason| stored.finish(reason, snapshot.total_value, now));
                let changed = kept || ended;
                Ok::<_, ServiceError>((
                    changed.then(|| (stored.value_history.clone(), stored.outcome)),
                    changed,
                ))
            })
            // The valuation itself is still worth showing; losing one reading
            // of the curve is not worth failing the call for, but it is worth
            // knowing about.
            .unwrap_or_else(|error| {
                tracing::warn!(%error, game = %session.id, "relevé du portefeuille non enregistré");
                None
            });
        if let Some((history, outcome)) = updated {
            session.value_history = history;
            session.outcome = outcome;
        }

        Ok(PortfolioReport {
            session,
            snapshot,
            goal,
        })
    }

    /// Ends a game on request, at the value it has right now.
    ///
    /// It values the portfolio first, so the number kept is a real one rather
    /// than whatever the last refresh happened to leave behind. Ending a game
    /// that is already over changes nothing and is not an error — two clicks
    /// should not produce two different results.
    ///
    /// A person may stop an AI game — they are the one supervising it — but an
    /// AI may not stop a person's.
    pub async fn end_game(
        &self,
        id: Uuid,
        actor: PlayerKind,
        now: Timestamp,
    ) -> ServiceResult<GameSession> {
        let owner = self.load_game(id)?;
        if actor == PlayerKind::Ai {
            engine::validate_actor(&owner, actor)?;
        }

        let report = self.portfolio(id, now).await?;
        if report.session.is_over() {
            return Ok(report.session);
        }
        if !self.is_trustworthy(&report.snapshot).await {
            return Err(ServiceError::Unvalued {
                symbols: report.snapshot.unpriced_symbols.join(", "),
            });
        }
        let value = report.snapshot.total_value;

        self.store().mutate(id, |session| {
            session.finish(EndReason::Stopped, value, now);
            Ok::<_, ServiceError>(session.clone())
        })
    }

    /// What a finished game amounted to. Refuses a game still in play.
    pub fn summary(&self, id: Uuid) -> ServiceResult<safe_invest_core::summary::Summary> {
        let session = self.load_game(id)?;
        safe_invest_core::summary::of(&session).ok_or_else(|| {
            ServiceError::rejected(
                "Cette partie est encore en cours : il n'y a pas de bilan à en tirer.",
            )
        })
    }

    /// Whether a valuation is complete and real enough to be written down.
    ///
    /// Every line priced, and no price invented — unless inventing prices is
    /// the whole point, in demonstration mode.
    async fn is_trustworthy(&self, snapshot: &PortfolioSnapshot) -> bool {
        snapshot.unpriced_symbols.is_empty()
            && (!snapshot.contains_simulated_prices || self.market().await.is_simulation_forced())
    }

    /// Trade history, newest first, capped at `limit`.
    pub fn trade_history(&self, id: Uuid, limit: Option<usize>) -> ServiceResult<Vec<Trade>> {
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

    /// Everything needed to draw one asset's page, seen from `game` when one
    /// is open: its currency, what it holds, what it can still spend.
    pub async fn asset_report(
        &self,
        kind: AssetKind,
        symbol: &str,
        days: u16,
        game: Option<Uuid>,
    ) -> ServiceResult<AssetReport> {
        let asset = self.resolve_asset(kind, symbol)?;
        let session = game.map(|id| self.load_game(id)).transpose()?;

        let currency = session
            .as_ref()
            .map_or_else(|| self.settings().default_currency, |g| g.currency.clone());

        let market = self.market().await;
        let quotes = market
            .quotes(std::slice::from_ref(&asset), &currency)
            .await
            .quotes;
        let days = days.clamp(1, 365);
        let history = market.history(&asset, days, &currency).await;

        Ok(AssetReport {
            history_days: days,
            position: session
                .as_ref()
                .and_then(|g| g.find_holding(kind, &asset.symbol).cloned()),
            cash: session.as_ref().map_or(Decimal::ZERO, |g| g.cash),
            fee_percent: session.as_ref().map_or(Decimal::ZERO, |g| g.fee_percent),
            observer_mode: session
                .as_ref()
                .is_some_and(|g| g.player_kind == PlayerKind::Ai),
            finished: session.as_ref().is_some_and(GameSession::is_over),
            quote: quotes.get(&asset.key()).cloned(),
            asset,
            history,
            currency,
        })
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
        let id = request.game_id;
        let session = self.store().load(id)?;
        // Checked before the quote as well as inside the engine: an order that
        // will be refused should not spend a request of somebody's free tier.
        engine::validate_actor(&session, request.actor)?;
        let asset = self.resolve_asset(request.kind, &request.symbol)?;
        let quote = self.quote_for(&asset, &session.currency).await?;

        self.store().mutate(id, |session| {
            engine::buy(
                session,
                request.actor,
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
        let id = request.game_id;
        let session = self.store().load(id)?;
        engine::validate_actor(&session, request.actor)?;
        let asset = self.resolve_asset(request.kind, &request.symbol)?;
        let quote = self.quote_for(&asset, &session.currency).await?;

        self.store().mutate(id, |session| {
            engine::sell(
                session,
                request.actor,
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

    /// Applies a change from the settings screen, and rebuilds the market
    /// only if the change concerns it.
    pub async fn update_preferences(&self, patch: PreferencesPatch) -> ServiceResult<()> {
        let rebuild = patch.touches_market();
        self.settings_service()
            .update_preferences(patch)
            .map_err(settings_error)?;
        if rebuild {
            self.reload_market().await?;
        }
        Ok(())
    }

    /// Stores an API key, sealed, and puts it to use. Returns nothing: a
    /// stored secret is never read back out to a caller.
    pub async fn set_api_key(&self, provider_id: &str, key: &str) -> ServiceResult<()> {
        self.settings_service()
            .set_api_key(provider_id, key)
            .map_err(settings_error)?;
        self.reload_market().await
    }
}

/// A refusal is the caller's to fix; anything else is the disk's.
fn settings_error(error: SettingsError) -> ServiceError {
    match error {
        SettingsError::Rejected(message) => ServiceError::Rejected(message),
        other => ServiceError::Storage(other.to_string()),
    }
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
    use crate::ContextConfig;
    use safe_invest_market::providers::simulated::SimulatedProvider;
    use safe_invest_market::{ChainOptions, MarketDataService, fx::FxRates, http::HttpClient};
    use std::sync::Arc;
    use std::time::Duration;

    fn at(text: &str) -> Timestamp {
        text.parse().unwrap()
    }

    /// A market whose only source is the simulator. With `forced` it is demo
    /// mode; without, every price it gives is a fallback.
    fn market(
        sources: Vec<Arc<dyn safe_invest_market::QuoteProvider>>,
        forced: bool,
    ) -> MarketDataService {
        let order = AssetKind::ALL
            .into_iter()
            .map(|kind| (kind, vec!["simulated".to_owned()]))
            .collect();
        MarketDataService::with_providers(
            sources,
            ChainOptions {
                order,
                force_simulated: forced,
                cache_ttl: Duration::from_secs(60),
            },
            FxRates::new(HttpClient::new().unwrap()),
        )
    }

    /// An AI game holding some BTC, with a goal whose deadline is a day away.
    async fn game_with_a_position(context: &Context) -> Uuid {
        let start = at("2026-03-01T10:00:00Z");
        let session = context
            .create_game(
                NewGameRequest {
                    player_name: "Claude".into(),
                    player_kind: PlayerKind::Ai,
                    starting_cash: Decimal::from(1000),
                    currency: Some("EUR".into()),
                    fee_percent: None,
                    target_amount: Some(Decimal::from(1_000_000)),
                    deadline: Some(at("2026-03-02T10:00:00Z")),
                },
                start,
            )
            .unwrap();
        context
            .buy(
                BuyRequest {
                    game_id: session.id,
                    actor: PlayerKind::Ai,
                    symbol: "BTC".into(),
                    kind: AssetKind::Crypto,
                    sizing: TradeSizing::Amount(Decimal::from(500)),
                    rationale: Some("Une position pour le test.".into()),
                },
                start,
            )
            .await
            .unwrap();
        session.id
    }

    fn demo_context() -> (tempfile::TempDir, Context) {
        let dir = tempfile::tempdir().unwrap();
        let context = Context::new(&ContextConfig {
            data_dir: Some(dir.path().to_path_buf()),
            force_simulated: true,
        })
        .unwrap();
        (dir, context)
    }

    /// The deadline has passed, but a line cannot be priced: freezing the
    /// total now would record a result that leaves that line out.
    #[tokio::test]
    async fn a_game_is_not_frozen_on_a_valuation_that_misses_a_line() {
        let (_dir, context) = demo_context();
        let id = game_with_a_position(&context).await;
        let before = context.load_game(id).unwrap().value_history.len();

        context.replace_market(market(Vec::new(), false)).await;
        let late = at("2026-03-05T10:00:00Z");
        let report = context.portfolio(id, late).await.unwrap();

        assert!(!report.snapshot.unpriced_symbols.is_empty());
        let stored = context.load_game(id).unwrap();
        assert!(
            stored.outcome.is_none(),
            "la partie a été figée sur un total incomplet"
        );
        assert_eq!(
            stored.value_history.len(),
            before,
            "un relevé incomplet a été tracé"
        );

        let refused = context
            .end_game(id, PlayerKind::Human, late)
            .await
            .unwrap_err();
        assert!(
            matches!(refused, ServiceError::Unvalued { .. }),
            "{refused}"
        );

        // Once every line is priced again, the deadline does its work.
        context
            .replace_market(market(vec![Arc::new(SimulatedProvider::new())], true))
            .await;
        context.portfolio(id, late).await.unwrap();
        let ended = context.load_game(id).unwrap().outcome.unwrap();
        assert_eq!(ended.reason, EndReason::DeadlinePassed);
    }

    /// Outside demo mode, a simulated price is a stand-in for a source that
    /// failed. It is shown, flagged — but never written into the record.
    #[tokio::test]
    async fn a_fallback_price_is_never_written_into_the_record() {
        let (_dir, context) = demo_context();
        let id = game_with_a_position(&context).await;
        let before = context.load_game(id).unwrap().value_history.len();

        context
            .replace_market(market(vec![Arc::new(SimulatedProvider::new())], false))
            .await;
        let late = at("2026-03-05T10:00:00Z");
        let report = context.portfolio(id, late).await.unwrap();

        assert!(report.snapshot.contains_simulated_prices);
        let stored = context.load_game(id).unwrap();
        assert!(stored.outcome.is_none());
        assert_eq!(stored.value_history.len(), before);
    }

    /// A person supervises an AI game and may stop it; an AI may not stop a
    /// person's.
    #[tokio::test]
    async fn only_the_supervisor_may_stop_someone_elses_game() {
        let (_dir, context) = demo_context();
        let now = at("2026-03-01T10:00:00Z");

        let ai = game_with_a_position(&context).await;
        let stopped = context.end_game(ai, PlayerKind::Human, now).await.unwrap();
        assert!(stopped.is_over());

        let person = context
            .create_game(
                NewGameRequest {
                    player_name: "Léa".into(),
                    player_kind: PlayerKind::Human,
                    starting_cash: Decimal::from(1000),
                    currency: None,
                    fee_percent: None,
                    target_amount: None,
                    deadline: None,
                },
                now,
            )
            .unwrap();
        assert!(
            context
                .end_game(person.id, PlayerKind::Ai, now)
                .await
                .is_err()
        );
        assert!(!context.load_game(person.id).unwrap().is_over());
    }
}
