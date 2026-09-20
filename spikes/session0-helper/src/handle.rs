//! Minimal RAII wrappers.
//!
//! Every path below acquires a token or a desktop and can fail partway. Leaking
//! a token handle from a service that runs for weeks is a slow resource leak
//! nobody attributes to this code.

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::StationsAndDesktops::{CloseDesktop, HDESK};

pub struct OwnedHandle(pub HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }
}

pub struct OwnedDesktop(pub HDESK);

impl Drop for OwnedDesktop {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            unsafe {
                let _ = CloseDesktop(self.0);
            }
        }
    }
}
