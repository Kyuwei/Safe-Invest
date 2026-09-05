//! Protecting the API keys at rest.
//!
//! On Windows the key is sealed with DPAPI under the current user account, so
//! another account on the same machine cannot read it even with the file in
//! hand. Elsewhere — the CI runner, a developer on Linux — there is no
//! equivalent, and the value is stored as-is inside an owner-only directory.
//! The stored value carries its own scheme tag so the two cases can never be
//! confused for one another.

/// A value as it appears in `settings.json`: `dpapi:<hex>` or `plain:<hex>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sealed(String);

impl Sealed {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn from_stored(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// True when the value is stored in the clear — the Settings screen says so
    /// out loud rather than implying a protection that is not there.
    pub fn is_plaintext(&self) -> bool {
        self.0.starts_with("plain:")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("format de secret non reconnu")]
    Malformed,
    #[error(transparent)]
    Platform(#[from] safe_invest_platform::secret::SecretError),
}

pub fn seal(plaintext: &str) -> Result<Sealed, SecretError> {
    if safe_invest_platform::secret::is_supported() {
        let bytes = safe_invest_platform::secret::protect(plaintext.as_bytes())?;
        return Ok(Sealed(format!("dpapi:{}", to_hex(&bytes))));
    }
    Ok(Sealed(format!("plain:{}", to_hex(plaintext.as_bytes()))))
}

pub fn unseal(sealed: &Sealed) -> Result<String, SecretError> {
    let (scheme, payload) = sealed.0.split_once(':').ok_or(SecretError::Malformed)?;
    let bytes = from_hex(payload)?;
    match scheme {
        "plain" => String::from_utf8(bytes).map_err(|_| SecretError::Malformed),
        "dpapi" => {
            let clear = safe_invest_platform::secret::unprotect(&bytes)?;
            String::from_utf8(clear).map_err(|_| SecretError::Malformed)
        }
        _ => Err(SecretError::Malformed),
    }
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn from_hex(text: &str) -> Result<Vec<u8>, SecretError> {
    if !text.len().is_multiple_of(2) {
        return Err(SecretError::Malformed);
    }
    let raw = text.as_bytes();
    raw.chunks_exact(2)
        .map(|pair| {
            let s = std::str::from_utf8(pair).map_err(|_| SecretError::Malformed)?;
            u8::from_str_radix(s, 16).map_err(|_| SecretError::Malformed)
        })
        .collect()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::unchecked_time_subtraction,
    reason = "a test that trips is a test that failed"
)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_key() {
        let sealed = seal("cmc-secret-key").unwrap();
        assert_eq!(unseal(&sealed).unwrap(), "cmc-secret-key");
    }

    #[test]
    fn stored_form_never_contains_the_plaintext() {
        let sealed = seal("cmc-secret-key").unwrap();
        assert!(!sealed.as_str().contains("cmc-secret-key"));
    }

    #[test]
    fn garbage_is_rejected_not_guessed() {
        assert!(unseal(&Sealed::from_stored("nonsense")).is_err());
        assert!(unseal(&Sealed::from_stored("plain:zz")).is_err());
        assert!(unseal(&Sealed::from_stored("other:6162")).is_err());
    }
}
