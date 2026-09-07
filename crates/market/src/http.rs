//! The one HTTP client the app uses, and the limits it enforces.
//!
//! Every request in the crate goes through here so the safety rules are stated
//! once: HTTPS only, a real `User-Agent`, bounded time, and — the one that
//! matters most — a bounded response body. A quote endpoint that starts
//! streaming gigabytes, whether through a bug or through malice, must not be
//! able to exhaust the memory of a desktop app.

use crate::error::{ProviderError, ProviderResult};
use std::time::Duration;

/// Sent on every request. CoinGecko sits behind Cloudflare, which answers 403
/// to callers with no `User-Agent` — the exact bug that silently demoted the
/// primary crypto source to the scraper in the previous version.
pub const USER_AGENT: &str = concat!(
    "SafeInvest/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/Kyuwei/Safe-Invest)"
);

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(12);

/// 4 MiB. The largest legitimate response here is a scraped HTML page at a few
/// hundred kilobytes.
const MAX_BODY_BYTES: u64 = 4 * 1024 * 1024;

/// A shared client. Cloning is cheap — `reqwest::Client` is an `Arc` inside —
/// and every clone reuses the same connection pool, which is what keeps a
/// refresh of twenty symbols down to a handful of TCP handshakes.
#[derive(Debug, Clone)]
pub struct HttpClient {
    inner: reqwest::Client,
}

impl HttpClient {
    pub fn new() -> Result<Self, reqwest::Error> {
        install_crypto_provider();

        let inner = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .connect_timeout(CONNECT_TIMEOUT)
            .timeout(REQUEST_TIMEOUT)
            // A redirect chain is a fine way to be walked off HTTPS, so each
            // hop is checked rather than merely counted.
            .redirect(reqwest::redirect::Policy::custom(|attempt| {
                if attempt.previous().len() >= 3 || !is_allowed(attempt.url()) {
                    attempt.stop()
                } else {
                    attempt.follow()
                }
            }))
            .gzip(true)
            .build()?;

        Ok(Self { inner })
    }

    /// GET a JSON document, with `headers` added to the request.
    pub async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        provider: &'static str,
        url: &str,
        headers: &[(&str, &str)],
    ) -> ProviderResult<T> {
        let body = self.get_text(provider, url, headers).await?;
        serde_json::from_str(&body).map_err(|error| ProviderError::Malformed {
            provider,
            detail: error.to_string(),
        })
    }

    /// GET a text document — HTML for the scraper, JSON for everything else.
    pub async fn get_text(
        &self,
        provider: &'static str,
        url: &str,
        headers: &[(&str, &str)],
    ) -> ProviderResult<String> {
        let parsed = reqwest::Url::parse(url).map_err(|_| ProviderError::Malformed {
            provider,
            detail: "URL invalide".into(),
        })?;
        if !is_allowed(&parsed) {
            return Err(ProviderError::Malformed {
                provider,
                detail: "URL refusée : seul HTTPS est autorisé".into(),
            });
        }

        let mut request = self.inner.get(url);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }

        let response = request
            .send()
            .await
            .map_err(|error| transport(provider, &error))?;

        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(ProviderError::RateLimited { provider });
        }
        if !status.is_success() {
            return Err(ProviderError::Http {
                provider,
                status: status.as_u16(),
            });
        }

        // Refuse an oversized body before reading it, when the server is honest
        // enough to declare one...
        if let Some(declared) = response.content_length()
            && declared > MAX_BODY_BYTES
        {
            return Err(ProviderError::Malformed {
                provider,
                detail: format!("réponse de {declared} octets, au-delà de la limite"),
            });
        }

        // ...and stop reading at the limit when it is not.
        read_capped(provider, response).await
    }
}

async fn read_capped(
    provider: &'static str,
    mut response: reqwest::Response,
) -> ProviderResult<String> {
    let mut body = Vec::with_capacity(16 * 1024);
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| transport(provider, &error))?
    {
        if body.len() as u64 + chunk.len() as u64 > MAX_BODY_BYTES {
            return Err(ProviderError::Malformed {
                provider,
                detail: "réponse tronquée : au-delà de la limite de taille".into(),
            });
        }
        body.extend_from_slice(&chunk);
    }

    String::from_utf8(body).map_err(|_| ProviderError::Malformed {
        provider,
        detail: "réponse non UTF-8".into(),
    })
}

/// HTTPS everywhere, with one exception: plain HTTP to the loopback address.
///
/// That exception exists so the test suite can serve recorded API payloads from
/// a local socket without a TLS certificate. It cannot weaken a real request —
/// traffic to 127.0.0.1 never leaves the machine — and it is checked on every
/// redirect hop, not just the first URL.
fn is_allowed(url: &reqwest::Url) -> bool {
    match url.scheme() {
        "https" => true,
        "http" => matches!(
            url.host_str(),
            Some("localhost" | "127.0.0.1" | "[::1]" | "::1")
        ),
        _ => false,
    }
}

fn transport(provider: &'static str, error: &reqwest::Error) -> ProviderError {
    if error.is_timeout() {
        return ProviderError::Transport {
            provider,
            detail: "délai dépassé".into(),
        };
    }
    ProviderError::Transport {
        provider,
        // Deliberately not the full error: a reqwest display can carry the URL,
        // and a URL can carry an API key.
        detail: if error.is_connect() {
            "connexion impossible".into()
        } else {
            "erreur réseau".into()
        },
    }
}

/// rustls needs a crypto provider chosen once per process. Doing it here means
/// no caller has to remember, and a second call is harmless.
fn install_crypto_provider() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}
