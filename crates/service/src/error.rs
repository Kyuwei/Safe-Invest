//! One error type for every operation.

use safe_invest_core::engine::TradeError;
use safe_invest_core::factory::NewGameError;
use safe_invest_core::store::StoreError;

/// A failure worth showing someone.
///
/// Each variant carries a French sentence for the player, and [`Self::hint`]
/// adds a line aimed at whoever called — an AI reading a tool error needs to
/// know what to do differently, not just that something went wrong.
#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("{0}")]
    Rejected(String),

    #[error("Aucune partie n'est ouverte.")]
    NoCurrentGame,

    #[error("Partie introuvable.")]
    GameNotFound,

    #[error("Aucun cours disponible pour {symbol} : toutes les sources ont échoué.")]
    NoQuote { symbol: String },

    #[error("Actif inconnu : {query}")]
    UnknownAsset { query: String },

    #[error("Erreur de stockage : {0}")]
    Storage(String),
}

impl ServiceError {
    /// What to try instead. Shown to the AI alongside the message.
    pub fn hint(&self) -> Option<&'static str> {
        match self {
            Self::NoCurrentGame => Some(
                "Appelez `list_games` puis `open_game`, ou `create_game` pour en démarrer une.",
            ),
            Self::GameNotFound => {
                Some("Utilisez `list_games` pour obtenir les identifiants valides.")
            }
            Self::NoQuote { .. } => Some(
                "Réessayez dans un instant, ou vérifiez l'état des sources avec `get_market_sources`.",
            ),
            Self::UnknownAsset { .. } => {
                Some("Cherchez d'abord le symbole exact avec `search_assets`.")
            }
            Self::Rejected(_) => None,
            Self::Storage(_) => {
                Some("Vérifiez que le dossier de données est accessible en écriture.")
            }
        }
    }

    pub fn rejected(message: impl Into<String>) -> Self {
        Self::Rejected(message.into())
    }
}

impl From<TradeError> for ServiceError {
    fn from(error: TradeError) -> Self {
        Self::Rejected(error.to_string())
    }
}

impl From<NewGameError> for ServiceError {
    fn from(error: NewGameError) -> Self {
        Self::Rejected(error.to_string())
    }
}

impl From<StoreError> for ServiceError {
    fn from(error: StoreError) -> Self {
        match error {
            StoreError::NotFound(_) => Self::GameNotFound,
            other => Self::Storage(other.to_string()),
        }
    }
}

pub type ServiceResult<T> = Result<T, ServiceError>;
