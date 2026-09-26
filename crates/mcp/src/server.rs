//! The fourteen tools an AI plays with.
//!
//! Every one of them goes through `safe-invest-service`, which is also what the
//! window calls. There is no shortcut from here into the engine, so an AI
//! cannot do anything a person could not, and vice versa.

use crate::params::{Amount, Deadline, Kind, Player};
use rmcp::handler::server::tool::ToolRouter;
use rmcp::handler::server::wrapper::{Json, Parameters};
use rmcp::model::{
    ContentBlock, Implementation, IntoContents, JsonObject, ServerCapabilities, ServerInfo,
};
use rmcp::{ServerHandler, tool, tool_handler, tool_router};
use safe_invest_core::model::{AssetKind, PlayerKind, normalize_currency};
use safe_invest_service::{
    BuyRequest, Context, NewGameRequest, SellRequest, ServiceError, SetGoalRequest, TradeSizing,
    view,
};
use schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

/// What the client is told the server is for.
const INSTRUCTIONS: &str = "\
Safe Invest est un simulateur d'investissement à but pédagogique. L'argent est fictif ; \
les cours sont réels, tirés de CoinGecko, Yahoo Finance, CoinMarketCap ou Finnhub, avec un \
repli simulé clairement signalé.

Déroulé : `list_games` puis `open_game`, ou `create_game` avec player_kind=\"ai\". La partie \
ouverte devient celle de cette connexion : les autres outils agissent dessus quand `game_id` \
est omis. Ensuite `search_assets` ou `list_popular_assets` pour trouver un symbole, \
`get_quotes` et `get_price_history` pour juger, `buy` / `sell` pour agir, `get_portfolio` \
pour le résultat.

Règles :
- Une IA ne passe d'ordres que dans une partie IA. Une partie humaine se consulte, sans plus.
- Dans une partie IA, `buy` et `sell` exigent un champ `rationale` non vide. C'est ce \
commentaire qui apparaît dans l'historique lu par la personne qui apprend : écrivez-le pour \
elle, en une ou deux phrases (1000 caractères au plus).
- Une partie se termine seule quand l'objectif est atteint ou la date limite passée : \
regardez `outcome` dans `get_portfolio` avant d'agir.
- Un refus revient comme un résultat en erreur, avec la raison et un conseil. Lisez-le et \
corrigez l'appel plutôt que de le répéter tel quel.

Tout cours porte `sourceId` et `isSimulated`. Ne présentez jamais un cours simulé comme réel.";

#[derive(Clone)]
pub struct SafeInvestServer {
    context: Context,
    /// The game this connection acts on when a call names none.
    ///
    /// Held per server, which is per client: one per stdio process, one per
    /// HTTP session. Two AIs connected at once each keep their own, and the
    /// window — which always names the game it shows — never moves it.
    current: Arc<Mutex<Option<Uuid>>>,
    tool_router: ToolRouter<Self>,
}

impl std::fmt::Debug for SafeInvestServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SafeInvestServer").finish_non_exhaustive()
    }
}

// ------------------------------------------------------------- parameters

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GameRef {
    /// Identifiant de la partie. Omettez-le pour agir sur la partie courante.
    #[serde(default)]
    pub game_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OpenGameArgs {
    /// Identifiant renvoyé par `list_games` ou `create_game`.
    pub game_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CreateGameArgs {
    /// Nom affiché du joueur, par exemple « Claude » ou « Léa ».
    pub player_name: String,
    /// `ai` impose une justification à chaque opération ; `human` ne l'impose pas.
    pub player_kind: Player,
    /// Capital fictif de départ.
    pub starting_cash: Amount,
    /// Code à trois lettres. EUR par défaut.
    #[serde(default)]
    pub currency: Option<String>,
    /// Frais par opération, en pourcentage (0 à 5). 0 par défaut.
    #[serde(default)]
    pub fee_percent: Option<Amount>,
    /// Montant à atteindre. À fournir avec `deadline`.
    #[serde(default)]
    pub target_amount: Option<Amount>,
    /// Date à laquelle `target_amount` doit être atteint.
    #[serde(default)]
    pub deadline: Option<Deadline>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SetGoalArgs {
    #[serde(default)]
    pub game_id: Option<String>,
    /// Montant à atteindre, strictement supérieur au capital de départ.
    pub target_amount: Amount,
    /// Date limite, dans le futur.
    pub deadline: Deadline,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HistoryArgs {
    #[serde(default)]
    pub game_id: Option<String>,
    /// Nombre maximum d'opérations, les plus récentes d'abord. 50 par défaut,
    /// 500 au plus ; `total` dit combien il y en a en tout.
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchArgs {
    /// Symbole ou nom, par exemple « BTC », « bitcoin » ou « Airbus ».
    pub query: String,
    /// Restreint la recherche à un type d'actif.
    #[serde(default)]
    pub kind: Option<Kind>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct KindArgs {
    #[serde(default)]
    pub kind: Option<Kind>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct QuotesArgs {
    /// Symboles, par exemple `["BTC", "ETH"]`.
    pub symbols: Vec<String>,
    /// Type des symboles demandés.
    pub kind: Kind,
    /// Devise de cotation. Celle de la partie courante par défaut, sinon EUR.
    #[serde(default)]
    pub currency: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct HistoryQuoteArgs {
    pub symbol: String,
    pub kind: Kind,
    /// Nombre de jours d'historique (1 à 365). 30 par défaut.
    #[serde(default)]
    pub days: Option<u16>,
    #[serde(default)]
    pub currency: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct BuyArgs {
    #[serde(default)]
    pub game_id: Option<String>,
    pub symbol: String,
    pub kind: Kind,
    /// Nombre d'unités à acheter. Exclusif avec `amount`.
    #[serde(default)]
    pub quantity: Option<Amount>,
    /// Somme à investir, frais compris. Exclusif avec `quantity`.
    #[serde(default)]
    pub amount: Option<Amount>,
    /// Obligatoire en partie IA : pourquoi cette opération, en une phrase.
    #[serde(default)]
    pub rationale: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SellArgs {
    #[serde(default)]
    pub game_id: Option<String>,
    pub symbol: String,
    pub kind: Kind,
    /// Nombre d'unités à vendre.
    #[serde(default)]
    pub quantity: Option<Amount>,
    /// Somme à dégager.
    #[serde(default)]
    pub amount: Option<Amount>,
    /// Vend toute la position.
    #[serde(default)]
    pub all: bool,
    /// Obligatoire en partie IA : pourquoi cette opération, en une phrase.
    #[serde(default)]
    pub rationale: Option<String>,
}

// ------------------------------------------------------------------ tools

/// How many symbols one `get_quotes` call may ask about. Each costs a request
/// against somebody's free tier; a model asking for a hundred at once is a
/// model that should narrow its question.
const MAX_QUOTED_SYMBOLS: usize = 25;

/// How many trades `get_trade_history` returns when no limit is given, and the
/// most it returns at all.
const DEFAULT_HISTORY: usize = 50;
const MAX_HISTORY: usize = 500;

/// Every tool an AI can call, in the order the settings screen lists them.
///
/// It sits beside the router and is checked against it by a test below, so a
/// tool added in one place and forgotten in the other fails the build rather
/// than quietly misleading whoever reads the settings screen.
pub const TOOL_NAMES: &[&str] = &[
    "list_games",
    "create_game",
    "open_game",
    "get_portfolio",
    "set_goal",
    "get_goal_progress",
    "end_game",
    "get_summary",
    "get_trade_history",
    "get_market_sources",
    "search_assets",
    "list_popular_assets",
    "get_quotes",
    "get_price_history",
    "buy",
    "sell",
];

#[tool_router]
#[allow(
    clippy::unnecessary_wraps,
    reason = "every tool shares one fallible signature, including the two that cannot currently fail"
)]
impl SafeInvestServer {
    pub fn new(context: Context) -> Self {
        // Resume where the last run left off, so a client that restarts the
        // server between two conversations finds its game again.
        let remembered = context.remembered_ai_game();
        Self {
            context,
            current: Arc::new(Mutex::new(remembered)),
            tool_router: Self::tool_router(),
        }
    }

    #[tool(
        name = "list_games",
        annotations(title = "Lister les parties", read_only_hint = true, open_world_hint = false),
        output_schema = object_schema(),
        description = "Liste les parties enregistrées, activité la plus récente d'abord. Indique aussi laquelle est la partie courante."
    )]
    fn list_games(&self) -> Result<Json<Value>, ToolFailure> {
        let games = self.context.list_games();
        let current = self.current_game().map(|id| id.to_string());

        Ok(Json(json!({
            "currentGameId": current,
            "games": games.iter().map(|g| json!({
                "gameId": g.id.to_string(),
                "playerName": g.player_name,
                "playerKind": g.player_kind.as_str(),
                "currency": g.currency,
                "startingCash": g.starting_cash.to_string(),
                "cash": g.cash.to_string(),
                "holdingCount": g.holding_count,
                "tradeCount": g.trade_count,
                "createdAt": g.created_at.to_string(),
                "updatedAt": g.updated_at.to_string(),
                "goal": g.goal.map(|goal| json!({
                    "targetAmount": goal.target_amount.to_string(),
                    "deadline": goal.deadline.to_string(),
                })),
            })).collect::<Vec<_>>(),
        })))
    }

    #[tool(
        name = "create_game",
        annotations(title = "Créer une partie", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = false),
        output_schema = object_schema(),
        description = "Démarre une partie et en fait la partie courante. Avec player_kind=\"ai\", chaque achat et chaque vente devra porter une justification."
    )]
    fn create_game(
        &self,
        Parameters(args): Parameters<CreateGameArgs>,
    ) -> Result<Json<Value>, ToolFailure> {
        let session = self
            .context
            .create_game(
                NewGameRequest {
                    player_name: args.player_name,
                    player_kind: args.player_kind.into(),
                    starting_cash: args.starting_cash.0,
                    currency: args.currency,
                    fee_percent: args.fee_percent.map(|a| a.0),
                    target_amount: args.target_amount.map(|a| a.0),
                    deadline: args.deadline.map(|d| d.0),
                },
                jiff::Timestamp::now(),
            )
            .map_err(ToolFailure::from)?;
        self.make_current(session.id);

        Ok(Json(json!({
            "gameId": session.id.to_string(),
            "playerName": session.player_name,
            "playerKind": session.player_kind.as_str(),
            "currency": session.currency,
            "startingCash": session.starting_cash.to_string(),
            "feePercent": session.fee_percent.to_string(),
            "rationaleRequired": session.player_kind == safe_invest_core::model::PlayerKind::Ai,
        })))
    }

    #[tool(
        name = "open_game",
        annotations(title = "Ouvrir une partie", read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false),
        output_schema = object_schema(),
        description = "Ouvre une partie existante et en fait la partie courante, celle sur laquelle agissent les autres outils quand aucun game_id n'est donné."
    )]
    fn open_game(
        &self,
        Parameters(args): Parameters<OpenGameArgs>,
    ) -> Result<Json<Value>, ToolFailure> {
        let id = parse_uuid(&args.game_id)?;
        let session = self.context.load_game(id).map_err(ToolFailure::from)?;
        self.make_current(id);

        Ok(Json(json!({
            "gameId": session.id.to_string(),
            "playerName": session.player_name,
            "playerKind": session.player_kind.as_str(),
            "currency": session.currency,
            "cash": session.cash.to_string(),
            "holdings": session.holdings.len(),
            "trades": session.trades.len(),
        })))
    }

    #[tool(
        name = "get_portfolio",
        annotations(title = "Voir le portefeuille", read_only_hint = true, open_world_hint = true),
        output_schema = object_schema(),
        description = "Valeur actuelle du portefeuille : trésorerie, positions cotées au marché, plus-values latentes et réalisées, et l'avancement vers l'objectif s'il y en a un."
    )]
    async fn get_portfolio(
        &self,
        Parameters(args): Parameters<GameRef>,
    ) -> Result<Json<Value>, ToolFailure> {
        let id = self.game(args.game_id.as_deref())?;
        let report = self
            .context
            .portfolio(id, jiff::Timestamp::now())
            .await
            .map_err(ToolFailure::from)?;

        let snapshot = &report.snapshot;
        Ok(Json(json!({
            "gameId": report.session.id.to_string(),
            "currency": snapshot.currency,
            "totalValue": snapshot.total_value.to_string(),
            "cash": snapshot.cash.to_string(),
            "investedValue": snapshot.market_value.to_string(),
            "startingCash": snapshot.starting_cash.to_string(),
            "totalPnl": snapshot.total_pnl.to_string(),
            "totalPnlPercent": snapshot.total_pnl_percent.to_string(),
            "realizedPnl": snapshot.realized_pnl.to_string(),
            "unrealizedPnl": snapshot.unrealized_pnl.to_string(),
            "containsSimulatedPrices": snapshot.contains_simulated_prices,
            "unpricedSymbols": snapshot.unpriced_symbols,
            "positions": snapshot.positions.iter().map(|p| json!({
                "symbol": p.asset.symbol,
                "name": p.asset.name,
                "kind": p.asset.kind.as_str(),
                "quantity": p.quantity.to_string(),
                "averageCost": p.average_cost.to_string(),
                "price": p.price.map(|v| v.to_string()),
                "marketValue": p.market_value.map(|v| v.to_string()),
                "unrealizedPnl": p.unrealized_pnl.map(|v| v.to_string()),
                "unrealizedPnlPercent": p.unrealized_pnl_percent.map(|v| v.to_string()),
                "changePercent24h": p.change_percent_24h.map(|v| v.to_string()),
                "weightPercent": p.weight_percent.to_string(),
                "sourceId": p.source_id,
                "isSimulated": p.is_simulated,
            })).collect::<Vec<_>>(),
            "goal": report.goal.as_ref().map(|goal| goal_json(goal, &snapshot.currency)),
            // Present means the game is over and every order will be refused.
            "outcome": report.session.outcome.map(|outcome| json!({
                "endedAt": outcome.ended_at.to_string(),
                "reason": outcome.reason.as_str(),
                "reasonLabel": outcome.reason.label(),
                "finalValue": outcome.final_value.to_string(),
            })),
        })))
    }

    #[tool(
        name = "set_goal",
        annotations(title = "Fixer l'objectif", read_only_hint = false, destructive_hint = false, idempotent_hint = true, open_world_hint = false),
        output_schema = object_schema(),
        description = "Fixe ou remplace l'objectif d'une partie : un montant à atteindre et la date à laquelle il doit l'être."
    )]
    fn set_goal(
        &self,
        Parameters(args): Parameters<SetGoalArgs>,
    ) -> Result<Json<Value>, ToolFailure> {
        let session = self
            .context
            .set_goal(
                &SetGoalRequest {
                    game_id: self.game(args.game_id.as_deref())?,
                    actor: PlayerKind::Ai,
                    target_amount: args.target_amount.0,
                    deadline: args.deadline.0,
                },
                jiff::Timestamp::now(),
            )
            .map_err(ToolFailure::from)?;

        Ok(Json(json!({
            "gameId": session.id.to_string(),
            "goal": session.goal.map(|goal| json!({
                "targetAmount": goal.target_amount.to_string(),
                "deadline": goal.deadline.to_string(),
            })),
        })))
    }

    #[tool(
        name = "get_goal_progress",
        annotations(title = "Suivre l'objectif", read_only_hint = true, open_world_hint = true),
        output_schema = object_schema(),
        description = "Avancement vers l'objectif : pourcentage parcouru, montant restant, jours restants, et le rendement annualisé qu'il faudrait encore tenir."
    )]
    async fn get_goal_progress(
        &self,
        Parameters(args): Parameters<GameRef>,
    ) -> Result<Json<Value>, ToolFailure> {
        let id = self.game(args.game_id.as_deref())?;
        let report = self
            .context
            .portfolio(id, jiff::Timestamp::now())
            .await
            .map_err(ToolFailure::from)?;

        Ok(Json(match &report.goal {
            Some(progress) => goal_json(progress, &report.session.currency),
            None => json!({ "goal": null, "message": "Cette partie n'a pas d'objectif." }),
        }))
    }

    #[tool(
        name = "end_game",
        annotations(title = "Terminer la partie", read_only_hint = false, destructive_hint = true, idempotent_hint = true, open_world_hint = true),
        output_schema = object_schema(),
        description = "Termine la partie à sa valeur actuelle. Plus aucun ordre ne sera accepté ensuite. Une partie dont l'objectif est atteint ou dont la date limite est passée se termine d'elle-même : cet outil sert à s'arrêter avant."
    )]
    async fn end_game(
        &self,
        Parameters(args): Parameters<GameRef>,
    ) -> Result<Json<Value>, ToolFailure> {
        let id = self.game(args.game_id.as_deref())?;
        let session = self
            .context
            .end_game(id, PlayerKind::Ai, jiff::Timestamp::now())
            .await
            .map_err(ToolFailure::from)?;

        let outcome = session.outcome.map(|outcome| {
            json!({
                "endedAt": outcome.ended_at.to_string(),
                "reason": outcome.reason.as_str(),
                "reasonLabel": outcome.reason.label(),
                "finalValue": outcome.final_value.to_string(),
            })
        });

        Ok(Json(json!({
            "gameId": session.id.to_string(),
            "outcome": outcome,
        })))
    }

    #[tool(
        name = "get_summary",
        annotations(title = "Lire le bilan", read_only_hint = true, open_world_hint = false),
        output_schema = object_schema(),
        description = "Bilan d'une partie terminée : résultat, durée, meilleur et pire trade, part des ventes gagnantes, et ce que le résultat vaut ramené à l'année. Refuse une partie encore en cours."
    )]
    fn get_summary(
        &self,
        Parameters(args): Parameters<GameRef>,
    ) -> Result<Json<Value>, ToolFailure> {
        let id = self.game(args.game_id.as_deref())?;
        let summary = self.context.summary(id).map_err(ToolFailure::from)?;
        let session = self.context.load_game(id).map_err(ToolFailure::from)?;

        serde_json::to_value(view::summary(&session, &summary))
            .map(Json)
            .map_err(|error| ToolFailure::new(format!("bilan illisible : {error}"), None))
    }

    #[tool(
        name = "get_trade_history",
        annotations(title = "Lire l'historique", read_only_hint = true, open_world_hint = false),
        output_schema = object_schema(),
        description = "Historique daté des achats et des ventes, les plus récents d'abord, avec la justification écrite pour chaque opération d'une IA."
    )]
    fn get_trade_history(
        &self,
        Parameters(args): Parameters<HistoryArgs>,
    ) -> Result<Json<Value>, ToolFailure> {
        let id = self.game(args.game_id.as_deref())?;
        let mut trades = self
            .context
            .trade_history(id, None)
            .map_err(ToolFailure::from)?;
        // A long game has hundreds of trades, and all of them would land in the
        // model's context at once. The recent ones are what a decision needs;
        // `total` says how many more there are to ask for.
        let total = trades.len();
        trades.truncate(args.limit.unwrap_or(DEFAULT_HISTORY).min(MAX_HISTORY));

        Ok(Json(json!({
            "total": total,
            "count": trades.len(),
            "trades": trades.iter().map(|t| json!({
                "id": t.id.to_string(),
                "timestamp": t.timestamp.to_string(),
                "side": t.side.as_str(),
                "symbol": t.asset.symbol,
                "name": t.asset.name,
                "kind": t.asset.kind.as_str(),
                "quantity": t.quantity.to_string(),
                "unitPrice": t.unit_price.to_string(),
                "fees": t.fees.to_string(),
                "total": t.total.to_string(),
                "realizedPnl": t.realized_pnl.map(|v| v.to_string()),
                "rationale": t.rationale,
                "actor": t.actor_kind.as_str(),
                "quoteSourceId": t.quote_source_id,
                "quoteWasSimulated": t.quote_was_simulated,
            })).collect::<Vec<_>>(),
        })))
    }

    #[tool(
        name = "get_market_sources",
        annotations(title = "État des sources de cours", read_only_hint = true, open_world_hint = false),
        output_schema = object_schema(),
        description = "État des sources de cours : laquelle est configurée, laquelle a répondu, laquelle est en échec et pourquoi. À consulter quand un cours manque."
    )]
    async fn get_market_sources(&self) -> Result<Json<Value>, ToolFailure> {
        let statuses = self.context.market_sources().await;

        Ok(Json(json!({
            "sources": statuses.iter().map(|s| json!({
                "id": s.id,
                "label": s.label,
                "kinds": s.kinds.iter().map(|k| k.as_str()).collect::<Vec<_>>(),
                "configured": s.configured,
                "isSimulated": s.is_simulated,
                "healthy": s.healthy,
                "detail": s.detail,
                "lastUsed": s.last_used.map(|t| t.to_string()),
            })).collect::<Vec<_>>(),
        })))
    }

    #[tool(
        name = "search_assets",
        annotations(title = "Chercher un actif", read_only_hint = true, open_world_hint = true),
        output_schema = object_schema(),
        description = "Cherche un actif par symbole ou par nom, dans le catalogue intégré puis chez les fournisseurs. Renvoie le symbole exact à passer à get_quotes, buy ou sell."
    )]
    async fn search_assets(
        &self,
        Parameters(args): Parameters<SearchArgs>,
    ) -> Result<Json<Value>, ToolFailure> {
        let assets = self
            .context
            .search_assets(&args.query, args.kind.map(AssetKind::from))
            .await;

        Ok(Json(json!({
            "count": assets.len(),
            "assets": assets.iter().take(40).map(asset_json).collect::<Vec<_>>(),
        })))
    }

    #[tool(
        name = "list_popular_assets",
        annotations(title = "Catalogue d'actifs", read_only_hint = true, open_world_hint = false),
        output_schema = object_schema(),
        description = "Le catalogue intégré : cryptos, actions et ETF connus, avec leur identifiant chez les fournisseurs. Point de départ quand on ne sait pas quoi chercher."
    )]
    fn list_popular_assets(
        &self,
        Parameters(args): Parameters<KindArgs>,
    ) -> Result<Json<Value>, ToolFailure> {
        let assets = self.context.popular_assets(args.kind.map(AssetKind::from));

        Ok(Json(json!({
            "count": assets.len(),
            "assets": assets.iter().map(asset_json).collect::<Vec<_>>(),
        })))
    }

    #[tool(
        name = "get_quotes",
        annotations(title = "Cours actuels", read_only_hint = true, open_world_hint = true),
        output_schema = object_schema(),
        description = "Cours actuels de plusieurs symboles. Chaque cours indique sa source et s'il est simulé — ne présentez jamais un cours simulé comme réel."
    )]
    async fn get_quotes(
        &self,
        Parameters(args): Parameters<QuotesArgs>,
    ) -> Result<Json<Value>, ToolFailure> {
        let kind = AssetKind::from(args.kind);
        let currency = self.currency_for(args.currency.as_deref())?;

        if args.symbols.len() > MAX_QUOTED_SYMBOLS {
            return Err(ToolFailure::from(ServiceError::rejected(format!(
                "{} symboles demandés : {MAX_QUOTED_SYMBOLS} au plus par appel.",
                args.symbols.len()
            ))));
        }

        let assets: Vec<_> = args
            .symbols
            .iter()
            .map(|symbol| self.context.resolve_asset(kind, symbol))
            .collect::<Result<_, _>>()
            .map_err(ToolFailure::from)?;

        let quotes = self.context.quotes(&assets, &currency).await;

        Ok(Json(json!({
            "currency": currency,
            "quotes": assets.iter().map(|asset| {
                let quote = quotes.get(&asset.key());
                json!({
                    "symbol": asset.symbol,
                    "kind": asset.kind.as_str(),
                    "price": quote.map(|q| q.price.to_string()),
                    "changePercent24h": quote.and_then(|q| q.change_percent_24h).map(|v| v.to_string()),
                    "marketCap": quote.and_then(|q| q.market_cap).map(|v| v.to_string()),
                    "volume24h": quote.and_then(|q| q.volume_24h).map(|v| v.to_string()),
                    "asOf": quote.map(|q| q.as_of.to_string()),
                    "sourceId": quote.map(|q| q.source_id.clone()),
                    "isSimulated": quote.is_some_and(|q| q.is_simulated),
                    "available": quote.is_some(),
                })
            }).collect::<Vec<_>>(),
        })))
    }

    #[tool(
        name = "get_price_history",
        annotations(title = "Historique des cours", read_only_hint = true, open_world_hint = true),
        output_schema = object_schema(),
        description = "Clôtures quotidiennes d'un actif, de la plus ancienne à la plus récente, pour juger une tendance avant d'agir."
    )]
    async fn get_price_history(
        &self,
        Parameters(args): Parameters<HistoryQuoteArgs>,
    ) -> Result<Json<Value>, ToolFailure> {
        let kind = AssetKind::from(args.kind);
        let currency = self.currency_for(args.currency.as_deref())?;
        let asset = self
            .context
            .resolve_asset(kind, &args.symbol)
            .map_err(ToolFailure::from)?;
        let days = args.days.unwrap_or(30).clamp(1, 365);

        let points = self.context.price_history(&asset, days, &currency).await;

        Ok(Json(json!({
            "symbol": asset.symbol,
            "kind": asset.kind.as_str(),
            "currency": currency,
            "days": days,
            "summary": price_summary(&points),
            "points": points.iter().map(|p| json!({
                "at": p.at.to_string(),
                "price": p.price.to_string(),
            })).collect::<Vec<_>>(),
        })))
    }

    #[tool(
        name = "buy",
        annotations(title = "Acheter", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = true),
        output_schema = object_schema(),
        description = "Achète un actif, soit une quantité exacte (quantity), soit pour une somme donnée (amount, frais compris). En partie IA, `rationale` est obligatoire : écrivez une phrase que la personne qui apprend pourra lire dans l'historique."
    )]
    async fn buy(&self, Parameters(args): Parameters<BuyArgs>) -> Result<Json<Value>, ToolFailure> {
        let sizing =
            TradeSizing::from_options(args.quantity.map(|a| a.0), args.amount.map(|a| a.0), false)
                .map_err(ToolFailure::from)?;

        let trade = self
            .context
            .buy(
                BuyRequest {
                    game_id: self.game(args.game_id.as_deref())?,
                    actor: PlayerKind::Ai,
                    symbol: args.symbol,
                    kind: args.kind.into(),
                    sizing,
                    rationale: args.rationale,
                },
                jiff::Timestamp::now(),
            )
            .await
            .map_err(ToolFailure::from)?;

        Ok(Json(trade_json(&trade)))
    }

    #[tool(
        name = "sell",
        annotations(title = "Vendre", read_only_hint = false, destructive_hint = false, idempotent_hint = false, open_world_hint = true),
        output_schema = object_schema(),
        description = "Vend un actif : une quantité (quantity), de quoi dégager une somme (amount), ou toute la position (all=true). En partie IA, `rationale` est obligatoire."
    )]
    async fn sell(
        &self,
        Parameters(args): Parameters<SellArgs>,
    ) -> Result<Json<Value>, ToolFailure> {
        let sizing = TradeSizing::from_options(
            args.quantity.map(|a| a.0),
            args.amount.map(|a| a.0),
            args.all,
        )
        .map_err(ToolFailure::from)?;

        let trade = self
            .context
            .sell(
                SellRequest {
                    game_id: self.game(args.game_id.as_deref())?,
                    actor: PlayerKind::Ai,
                    symbol: args.symbol,
                    kind: args.kind.into(),
                    sizing,
                    rationale: args.rationale,
                },
                jiff::Timestamp::now(),
            )
            .await
            .map_err(ToolFailure::from)?;

        Ok(Json(trade_json(&trade)))
    }

    /// The currency to quote in: the one asked for, else the current game's,
    /// else the configured default.
    fn currency_for(&self, requested: Option<&str>) -> Result<String, ToolFailure> {
        if let Some(requested) = requested.filter(|code| !code.trim().is_empty()) {
            return normalize_currency(requested).ok_or_else(|| {
                ToolFailure::from(ServiceError::rejected(format!(
                    "Devise invalide : « {} ». Attendu : un code à trois lettres, par exemple EUR ou USD.",
                    requested.chars().take(12).collect::<String>()
                )))
            });
        }
        Ok(self
            .current_game()
            .and_then(|id| self.context.load_game(id).ok())
            .map_or_else(|| self.context.settings().default_currency, |g| g.currency))
    }

    fn current_game(&self) -> Option<Uuid> {
        self.current.lock().ok().and_then(|current| *current)
    }

    /// Makes `id` this connection's game, and remembers it for the next run.
    fn make_current(&self, id: Uuid) {
        if let Ok(mut current) = self.current.lock() {
            *current = Some(id);
        }
        // Remembering is a convenience for the next run; failing to write it
        // costs nothing now, so it is logged rather than refused.
        if let Err(error) = self.context.remember_ai_game(id) {
            tracing::warn!(%error, "partie courante de l'IA non mémorisée");
        }
    }

    /// The game a call acts on: the one it names, else this connection's.
    fn game(&self, requested: Option<&str>) -> Result<Uuid, ToolFailure> {
        match optional_uuid(requested)? {
            Some(id) => Ok(id),
            None => self
                .current_game()
                .ok_or_else(|| ToolFailure::from(ServiceError::NoCurrentGame)),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for SafeInvestServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new("safe-invest", safe_invest_core::VERSION)
                    .with_title("Safe Invest"),
            )
            .with_instructions(INSTRUCTIONS)
    }
}

// ----------------------------------------------------------------- helpers

/// A refusal, told to the model as a result it can read.
///
/// The MCP specification separates two kinds of failure. A protocol error —
/// JSON-RPC's `error` — says the call itself was malformed, and many clients
/// hide it from the model or show only a code. A tool result with `isError`
/// says the call was understood and refused, and the model reads why. Every
/// refusal here is the second kind: not enough cash, no rationale, a game that
/// is over. The model should see the sentence and the hint, and try again
/// differently rather than blindly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolFailure {
    pub message: String,
    pub hint: Option<String>,
}

impl ToolFailure {
    fn new(message: impl Into<String>, hint: Option<&str>) -> Self {
        Self {
            message: message.into(),
            hint: hint.map(ToOwned::to_owned),
        }
    }
}

impl From<ServiceError> for ToolFailure {
    fn from(error: ServiceError) -> Self {
        Self::new(error.to_string(), error.hint())
    }
}

impl IntoContents for ToolFailure {
    fn into_contents(self) -> Vec<ContentBlock> {
        let text = match self.hint {
            Some(hint) => format!("{}\nConseil : {hint}", self.message),
            None => self.message,
        };
        vec![ContentBlock::text(text)]
    }
}

/// Declares that a tool answers with a JSON object.
///
/// Every tool here does, but the answers are built as `serde_json::Value`,
/// whose generated schema says nothing at all — not even "object". Clients
/// that validate `tools/list` strictly expect `"type": "object"` on an output
/// schema and would refuse the whole list over it.
fn object_schema() -> Arc<JsonObject> {
    let mut schema = JsonObject::new();
    schema.insert("type".to_owned(), json!("object"));
    Arc::new(schema)
}

fn parse_uuid(value: &str) -> Result<Uuid, ToolFailure> {
    Uuid::parse_str(value.trim()).map_err(|_| {
        ToolFailure::new(
            format!(
                "Identifiant de partie invalide : « {} ».",
                value.chars().take(40).collect::<String>()
            ),
            Some("Utilisez un identifiant renvoyé par `list_games`."),
        )
    })
}

fn optional_uuid(value: Option<&str>) -> Result<Option<Uuid>, ToolFailure> {
    value
        .filter(|v| !v.trim().is_empty())
        .map(parse_uuid)
        .transpose()
}

fn asset_json(asset: &safe_invest_core::model::Asset) -> Value {
    json!({
        "symbol": asset.symbol,
        "name": asset.name,
        "kind": asset.kind.as_str(),
        "providerId": asset.provider_id,
    })
}

fn goal_json(progress: &safe_invest_core::model::GoalProgress, currency: &str) -> Value {
    json!({
        "targetAmount": progress.target_amount.to_string(),
        "deadline": progress.deadline.to_string(),
        "currentValue": progress.current_value.to_string(),
        "progressPercent": progress.progress_percent.to_string(),
        "amountRemaining": progress.amount_remaining.to_string(),
        "daysRemaining": progress.days_remaining,
        "status": progress.status,
        "statusLabel": view::goal(progress, currency).status_label,
        "requiredAnnualisedReturnPercent": progress
            .required_annualised_return_percent
            .map(|v| v.to_string()),
        "achievedAnnualisedReturnPercent": progress
            .achieved_annualised_return_percent
            .map(|v| v.to_string()),
    })
}

/// The shape of a price series in five numbers, so a model can judge a trend
/// without reading every point — and without doing the arithmetic in its head.
fn price_summary(points: &[safe_invest_market::PricePoint]) -> Value {
    let (Some(first), Some(last)) = (points.first(), points.last()) else {
        return Value::Null;
    };
    let low = points.iter().map(|p| p.price).min().unwrap_or(first.price);
    let high = points.iter().map(|p| p.price).max().unwrap_or(first.price);
    let change = safe_invest_core::money::sub(last.price, first.price)
        .and_then(|delta| safe_invest_core::money::percent(delta, first.price))
        .ok()
        .map(|percent| percent.round_dp(2).to_string());

    json!({
        "first": first.price.to_string(),
        "last": last.price.to_string(),
        "low": low.to_string(),
        "high": high.to_string(),
        "changePercent": change,
        "from": first.at.to_string(),
        "to": last.at.to_string(),
    })
}

fn trade_json(trade: &safe_invest_core::model::Trade) -> Value {
    json!({
        "tradeId": trade.id.to_string(),
        "timestamp": trade.timestamp.to_string(),
        "side": trade.side.as_str(),
        "symbol": trade.asset.symbol,
        "name": trade.asset.name,
        "kind": trade.asset.kind.as_str(),
        "quantity": trade.quantity.to_string(),
        "unitPrice": trade.unit_price.to_string(),
        "fees": trade.fees.to_string(),
        "total": trade.total.to_string(),
        "realizedPnl": trade.realized_pnl.map(|v| v.to_string()),
        "rationale": trade.rationale,
        "quoteSourceId": trade.quote_source_id,
        "quoteWasSimulated": trade.quote_was_simulated,
    })
}

#[cfg(test)]
mod tests {
    use super::{SafeInvestServer, TOOL_NAMES};

    /// The settings screen shows this list to explain what an AI may do. A
    /// stale list would understate — or overstate — the access being granted.
    #[test]
    fn the_advertised_tool_list_matches_the_router() {
        let mut served: Vec<String> = SafeInvestServer::tool_router()
            .list_all()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect();
        served.sort();

        let mut advertised: Vec<String> = TOOL_NAMES.iter().map(|n| (*n).to_owned()).collect();
        advertised.sort();

        assert_eq!(served, advertised);
    }
}
