//! Getting a windowed executable back onto the terminal that launched it.

/// Attaches this process to the console of whatever started it, if there is one.
///
/// The Windows release build is a GUI-subsystem executable, so that
/// double-clicking it does not flash a console window. The price is that a
/// process started that way has *null* standard handles — and `println!` panics
/// when the write fails. Under `panic = "abort"` that is a bare non-zero exit
/// code with nothing printed, which is how `safe-invest.exe --version` first
/// failed its own smoke test.
///
/// So this does two things, and the second is the one that is easy to miss:
/// attach to the parent's console, then open the console device and install it
/// as the standard handles. Attaching alone leaves them null.
///
/// It leaves a *redirected* handle alone. A slot pointing at a file or a pipe
/// was set up by whoever started the process — `> log.txt`, a shell's `|`, an
/// MCP client's stdio — and installing the console device over it throws that
/// output away without a word. That is not hypothetical: it is how
/// `safe-invest.exe --version`, redirected to a file by the release check, came
/// back with an exit code of zero and an empty file. A console handle, or no
/// handle at all, is ours to replace.
///
/// Returns `true` when there is somewhere to write. A caller that gets `false`
/// — launched from Explorer, say — has nowhere to print and should not try; the
/// output helpers swallow the failure either way.
pub fn attach() -> bool {
    #[cfg(windows)]
    {
        windows::attach()
    }
    #[cfg(not(windows))]
    {
        // Every other platform starts console programs with real handles.
        true
    }
}

#[cfg(windows)]
mod windows {
    #![allow(
        unsafe_code,
        reason = "console attachment is a C API with no safe wrapper in-tree"
    )]

    use windows_sys::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TYPE_DISK, FILE_TYPE_PIPE,
        GetFileType, OPEN_EXISTING,
    };
    use windows_sys::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_HANDLE,
        STD_INPUT_HANDLE, STD_OUTPUT_HANDLE, SetStdHandle,
    };

    pub(super) fn attach() -> bool {
        // Attaching is cheap and harmless — it simply fails when there is no
        // parent console — and it is what makes `CONOUT$` openable below.
        // SAFETY: no pointers. The call either attaches this process to the
        // console of whatever launched it, or reports that there is none.
        unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };

        let out = keep_or_adopt(STD_OUTPUT_HANDLE, "CONOUT$", Access::Write);
        keep_or_adopt(STD_ERROR_HANDLE, "CONOUT$", Access::Write);
        keep_or_adopt(STD_INPUT_HANDLE, "CONIN$", Access::Read);
        out
    }

    /// Keeps a redirected handle; installs the console device otherwise.
    fn keep_or_adopt(slot: STD_HANDLE, device: &str, access: Access) -> bool {
        is_redirected(slot) || adopt(device, slot, access)
    }

    /// Whether this slot points at a file or a pipe rather than a console.
    ///
    /// This is the distinction that matters: a file or a pipe is somebody
    /// else's arrangement and must survive untouched, while a console handle —
    /// or an empty slot — is ours to replace, and after `AttachConsole` our own
    /// `CONOUT$` is the one that reliably works.
    fn is_redirected(slot: STD_HANDLE) -> bool {
        // SAFETY: no pointers. `GetStdHandle` returns a handle or a sentinel,
        // and `GetFileType` only reads the handle it is given.
        unsafe {
            let handle = GetStdHandle(slot);
            if handle.is_null() || handle == INVALID_HANDLE_VALUE {
                return false;
            }
            matches!(GetFileType(handle), FILE_TYPE_DISK | FILE_TYPE_PIPE)
        }
    }

    #[derive(Clone, Copy)]
    enum Access {
        Read,
        Write,
    }

    /// Opens one console device and installs it as a standard handle.
    fn adopt(device: &str, slot: STD_HANDLE, access: Access) -> bool {
        let name: Vec<u16> = device.encode_utf16().chain(std::iter::once(0)).collect();
        let rights = match access {
            Access::Read => GENERIC_READ,
            Access::Write => GENERIC_WRITE,
        };

        // SAFETY: `name` is a live, nul-terminated UTF-16 buffer for the whole
        // call. Every optional pointer is null, which the API documents as
        // "not supplied".
        let handle = unsafe {
            CreateFileW(
                name.as_ptr(),
                rights,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };

        if handle == INVALID_HANDLE_VALUE {
            return false;
        }

        // SAFETY: `handle` is the console handle just opened. Windows keeps it
        // for the life of the process, which is exactly how long it is wanted.
        unsafe { SetStdHandle(slot, handle) != 0 }
    }
}
