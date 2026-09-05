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
    #[error(
        "le déchiffrement a échoué : la clé a-t-elle été enregistrée par un autre compte Windows ?"
    )]
    Unprotect,
    #[error("le chiffrement a échoué")]
    Protect,
}

pub fn seal(plaintext: &str) -> Result<Sealed, SecretError> {
    #[cfg(windows)]
    {
        let bytes = windows_dpapi::protect(plaintext.as_bytes())?;
        Ok(Sealed(format!("dpapi:{}", to_hex(&bytes))))
    }
    #[cfg(not(windows))]
    {
        Ok(Sealed(format!("plain:{}", to_hex(plaintext.as_bytes()))))
    }
}

pub fn unseal(sealed: &Sealed) -> Result<String, SecretError> {
    let (scheme, payload) = sealed.0.split_once(':').ok_or(SecretError::Malformed)?;
    let bytes = from_hex(payload)?;
    match scheme {
        "plain" => String::from_utf8(bytes).map_err(|_| SecretError::Malformed),
        #[cfg(windows)]
        "dpapi" => {
            let clear = windows_dpapi::unprotect(&bytes)?;
            String::from_utf8(clear).map_err(|_| SecretError::Malformed)
        }
        #[cfg(not(windows))]
        "dpapi" => Err(SecretError::Unprotect),
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

#[cfg(windows)]
mod windows_dpapi {
    //! The only `unsafe` in the crate. Both calls follow the same shape: hand
    //! Windows a descriptor of our buffer, get one back, copy it out, free it.

    #![allow(
        unsafe_code,
        reason = "DPAPI is a C API; there is no safe wrapper in-tree"
    )]

    use super::SecretError;
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CryptProtectData, CryptUnprotectData,
    };

    /// Binds the blob to this application, so a DPAPI blob sealed by another
    /// program on the same account cannot be swapped in.
    const ENTROPY: &[u8] = b"SafeInvest/api-keys/v1";

    pub(super) fn protect(clear: &[u8]) -> Result<Vec<u8>, SecretError> {
        call(clear, true).ok_or(SecretError::Protect)
    }

    pub(super) fn unprotect(sealed: &[u8]) -> Result<Vec<u8>, SecretError> {
        call(sealed, false).ok_or(SecretError::Unprotect)
    }

    fn call(input: &[u8], encrypt: bool) -> Option<Vec<u8>> {
        let mut in_blob = blob(input);
        let mut entropy = blob(ENTROPY);
        let mut out = CRYPT_INTEGER_BLOB {
            cbData: 0,
            pbData: std::ptr::null_mut(),
        };

        // SAFETY: `in_blob` and `entropy` describe live slices that outlive the
        // call. Every optional pointer is null, which the API documents as
        // "not supplied". On success Windows allocates `out.pbData` with
        // LocalAlloc and we own it until the LocalFree below.
        let ok = unsafe {
            if encrypt {
                CryptProtectData(
                    &raw mut in_blob,
                    std::ptr::null(),
                    &raw mut entropy,
                    std::ptr::null(),
                    std::ptr::null(),
                    0,
                    &raw mut out,
                )
            } else {
                CryptUnprotectData(
                    &raw mut in_blob,
                    std::ptr::null_mut(),
                    &raw mut entropy,
                    std::ptr::null(),
                    std::ptr::null(),
                    0,
                    &raw mut out,
                )
            }
        };

        if ok == 0 || out.pbData.is_null() {
            return None;
        }

        // SAFETY: Windows reported success, so `pbData` points at `cbData`
        // initialised bytes.
        let copied =
            unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize) }.to_vec();
        // SAFETY: `pbData` came from LocalAlloc inside the call above and is
        // freed exactly once, here.
        unsafe { LocalFree(out.pbData.cast()) };
        Some(copied)
    }

    fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
        CRYPT_INTEGER_BLOB {
            cbData: u32::try_from(data.len()).unwrap_or(u32::MAX),
            pbData: data.as_ptr().cast_mut(),
        }
    }
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
