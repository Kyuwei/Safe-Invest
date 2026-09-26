//! Telling a person something when there is no console to tell it on.

/// Shows `message` in a modal error box.
///
/// The windowed Windows build has no console: launched from Explorer, anything
/// it prints goes nowhere. When the window cannot even open — no WebView2, a
/// data folder it cannot write — this is the only way the person who
/// double-clicked finds out why, rather than watching nothing happen.
///
/// Does nothing elsewhere: every other platform starts the program from a
/// terminal that shows standard error.
pub fn error(title: &str, message: &str) {
    #[cfg(windows)]
    {
        windows::error(title, message);
    }
    #[cfg(not(windows))]
    {
        let _ = (title, message);
    }
}

#[cfg(windows)]
mod windows {
    #![allow(
        unsafe_code,
        reason = "MessageBoxW is a C API with no safe wrapper in-tree"
    )]

    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MB_ICONERROR, MB_OK, MB_SETFOREGROUND, MessageBoxW,
    };

    pub(super) fn error(title: &str, message: &str) {
        let title = wide(title);
        let message = wide(message);
        // SAFETY: both buffers are live, nul-terminated UTF-16 for the whole
        // call. A null owner window is documented as "no owner".
        unsafe {
            MessageBoxW(
                std::ptr::null_mut(),
                message.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONERROR | MB_SETFOREGROUND,
            );
        }
    }

    /// UTF-16 with a terminating nul, and no interior nul to cut it short.
    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16()
            .filter(|unit| *unit != 0)
            .chain(std::iter::once(0))
            .collect()
    }
}
