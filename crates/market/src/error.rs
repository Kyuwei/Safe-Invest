//! What can go wrong when asking the internet for a price.

/// Every failure here is expected: an API is down, out of quota, or has quietly
/// changed its JSON. None of them may take the app down — the caller falls
/// through to the next source in the chain.
#[derive(Debug, thiserror::Error)]
pub enum ProviderError {
    #[error("{provider} : pas de clé API configurée")]
    MissingKey { provider: &'static str },

    #[error("{provider} : quota atteint, nouvelle tentative plus tard")]
    RateLimited { provider: &'static str },

    #[error("{provider} : réponse HTTP {status}")]
    Http { provider: &'static str, status: u16 },

    #[error("{provider} : réponse illisible ({detail})")]
    Malformed {
        provider: &'static str,
        detail: String,
    },

    #[error("{provider} : ne cote pas {symbol}")]
    NotSupported {
        provider: &'static str,
        symbol: String,
    },

    #[error("{provider} : réseau indisponible ({detail})")]
    Transport {
        provider: &'static str,
        detail: String,
    },
}

impl ProviderError {
    /// True when retrying the *same* provider in a moment could work. The chain
    /// uses this to decide between "skip for now" and "this source is broken".
    pub fn is_transient(&self) -> bool {
        match self {
            Self::RateLimited { .. } | Self::Transport { .. } => true,
            Self::Http { status, .. } => *status >= 500 || *status == 429,
            _ => false,
        }
    }

    pub fn provider(&self) -> &'static str {
        match self {
            Self::MissingKey { provider }
            | Self::RateLimited { provider }
            | Self::Http { provider, .. }
            | Self::Malformed { provider, .. }
            | Self::NotSupported { provider, .. }
            | Self::Transport { provider, .. } => provider,
        }
    }
}

pub type ProviderResult<T> = Result<T, ProviderError>;
