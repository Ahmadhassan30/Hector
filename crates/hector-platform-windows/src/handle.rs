use std::ptr;

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE};

use crate::{PlatformFailure, Win32Operation};

/// Unique ownership of one Win32 kernel handle.
#[derive(Debug)]
pub(crate) struct OwnedHandle {
    raw: HANDLE,
}

impl OwnedHandle {
    pub(crate) fn new(raw: HANDLE, operation: Win32Operation) -> Result<Self, PlatformFailure> {
        if raw.is_null() {
            Err(last_error(operation))
        } else {
            Ok(Self { raw })
        }
    }

    pub(crate) const fn raw(&self) -> HANDLE {
        self.raw
    }
}

// SAFETY: the wrapper uniquely owns the handle and exposes no shared
// operations. Moving ownership to an I/O thread preserves unique close.
unsafe impl Send for OwnedHandle {}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.raw.is_null() {
            // SAFETY: `raw` is a uniquely owned, still-open kernel handle.
            unsafe {
                CloseHandle(self.raw);
            }
            self.raw = ptr::null_mut();
        }
    }
}

pub(crate) fn last_error(operation: Win32Operation) -> PlatformFailure {
    // SAFETY: `GetLastError` has no preconditions and reads thread-local state.
    PlatformFailure::new(operation, unsafe { GetLastError() })
}
