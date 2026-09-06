//! Starting a new game.

use crate::model::{GameSession, Goal, PlayerKind};
use crate::money::round_money;
use jiff::Timestamp;
use rust_decimal::Decimal;
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NewGameError {
    #[error("Le capital de départ doit être strictement positif.")]
    StartingCash,
    #[error("Le nom du joueur ne peut pas être vide.")]
    PlayerName,
    #[error("La devise doit être un code à trois lettres, par exemple EUR.")]
    Currency,
    #[error("Les frais doivent être compris entre 0 % et 5 %.")]
    FeePercent,
    #[error("Le montant à atteindre doit dépasser le capital de départ.")]
    GoalTooLow,
    #[error("La date limite doit être dans le futur.")]
    GoalDeadline,
}

#[derive(Debug, Clone)]
pub struct NewGame {
    pub player_name: String,
    pub player_kind: PlayerKind,
    pub currency: String,
    pub starting_cash: Decimal,
    pub fee_percent: Decimal,
    pub goal: Option<Goal>,
}

/// Validates the choices made on the "new game" screen and builds the session.
///
/// The checks are here rather than in the UI so the MCP server gets exactly the
/// same ones — an AI cannot start a game a human could not.
pub fn create(request: NewGame, now: Timestamp) -> Result<GameSession, NewGameError> {
    let NewGame {
        player_name,
        player_kind,
        currency,
        starting_cash,
        fee_percent,
        goal,
    } = request;

    let player_name = player_name.trim().to_owned();
    if player_name.is_empty() {
        return Err(NewGameError::PlayerName);
    }

    let currency = currency.trim().to_uppercase();
    if currency.len() != 3 || !currency.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(NewGameError::Currency);
    }

    if starting_cash <= Decimal::ZERO {
        return Err(NewGameError::StartingCash);
    }

    if fee_percent < Decimal::ZERO || fee_percent > Decimal::from(5) {
        return Err(NewGameError::FeePercent);
    }

    let starting_cash = round_money(starting_cash);

    if let Some(goal) = goal {
        if goal.target_amount <= starting_cash {
            return Err(NewGameError::GoalTooLow);
        }
        if goal.deadline <= now {
            return Err(NewGameError::GoalDeadline);
        }
    }

    Ok(GameSession {
        id: Uuid::new_v4(),
        player_name,
        player_kind,
        currency,
        starting_cash,
        cash: starting_cash,
        holdings: Vec::new(),
        trades: Vec::new(),
        // The curve starts where the game starts, so day one is not a blank chart.
        value_history: vec![crate::model::ValuePoint {
            at: now,
            total_value: starting_cash,
        }],
        goal,
        outcome: None,
        fee_percent,
        created_at: now,
        updated_at: now,
        schema_version: GameSession::CURRENT_SCHEMA_VERSION,
    })
}
