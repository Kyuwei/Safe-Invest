//! Sealing a secret with the platform's own key store.

/// What can go wrong while sealing or unsealing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum SecretError {
    #[error("le chiffrement a échoué")]
    Protect,
    #[error(
        "le déchiffrement a échoué : la clé a-t-elle été enregistrée par un autre compte Windows ?"
    )]
    Unprotect,
}

/// True when this platform can actually seal a secret.
///
/// `false` elsewhere, and the caller is expected to say so out loud rather than
/// implying a protection that is not there.
pub const fn is_supported() -> bool {
    cfg!(windows)
}

/// Seals `clear` under the current user account.
pub fn protect(clear: &[u8]) -> Result<Vec<u8>, SecretError> {
    #[cfg(windows)]
    {
        imp::protect(clear)
    }
    #[cfg(not(windows))]
    {
        let _ = clear;
        Err(SecretError::Protect)
    }
}

/// Unseals what [`protect`] produced, under the same account.
pub fn unprotect(sealed: &[u8]) -> Result<Vec<u8>, SecretError> {
    #[cfg(windows)]
    {
        imp::unprotect(sealed)
    }
    #[cfg(not(windows))]
    {
        let _ = sealed;
        Err(SecretError::Unprotect)
    }
}

#[cfg(windows)]
mod imp {
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
