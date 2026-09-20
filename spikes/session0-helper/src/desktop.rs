//! Which desktop currently has input, and following it.
//!
//! Windows switches the input desktop out from under you: locking goes to
//! `Winlogon`, a UAC prompt goes to the secure desktop, unlocking goes back to
//! `Default`. Nothing notifies the desktop that is losing focus, so polling is
//! not laziness here - it is what shipping products do, because there is no
//! event you can receive from the losing side.

use anyhow::{Context, Result};
use windows::Win32::System::StationsAndDesktops::{
    GetThreadDesktop, GetUserObjectInformationW, OpenInputDesktop, SetThreadDesktop,
    UOI_NAME,
};
use windows::Win32::System::Threading::GetCurrentThreadId;

use crate::handle::OwnedDesktop;

/// Name of the desktop currently receiving input, e.g. "Default", "Winlogon",
/// or a screen-saver desktop.
pub fn input_desktop_name() -> Result<String> {
    unsafe {
        let hdesk = OpenInputDesktop(
            Default::default(),
            false,
            windows::Win32::System::StationsAndDesktops::DESKTOP_ACCESS_FLAGS(
                windows::Win32::Foundation::GENERIC_READ.0,
            ),
        )
        .context("OpenInputDesktop - only SYSTEM may open the secure desktop")?;
        let hdesk = OwnedDesktop(hdesk);
        name_of(&hdesk)
    }
}

/// Name of the desktop this thread is currently attached to.
pub fn current_desktop_name() -> Result<String> {
    unsafe {
        let hdesk = GetThreadDesktop(GetCurrentThreadId()).context("GetThreadDesktop")?;
        // Not owned: GetThreadDesktop does not give us a handle to close.
        let mut buf = [0u16; 256];
        let mut needed = 0u32;
        GetUserObjectInformationW(
            windows::Win32::Foundation::HANDLE(hdesk.0),
            UOI_NAME,
            Some(buf.as_mut_ptr() as *mut _),
            (buf.len() * 2) as u32,
            Some(&mut needed),
        )
        .context("GetUserObjectInformationW(UOI_NAME)")?;
        Ok(trim(&buf))
    }
}

fn name_of(hdesk: &OwnedDesktop) -> Result<String> {
    unsafe {
        let mut buf = [0u16; 256];
        let mut needed = 0u32;
        GetUserObjectInformationW(
            windows::Win32::Foundation::HANDLE(hdesk.0.0),
            UOI_NAME,
            Some(buf.as_mut_ptr() as *mut _),
            (buf.len() * 2) as u32,
            Some(&mut needed),
        )
        .context("GetUserObjectInformationW(UOI_NAME)")?;
        Ok(trim(&buf))
    }
}

fn trim(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

/// Attach this thread to whichever desktop has input.
///
/// Capture APIs bind to the calling thread's desktop, so a helper that does not
/// follow the switch keeps capturing a desktop nobody is looking at - producing
/// frames that are technically valid and completely stale, which is worse than
/// failing.
pub fn follow_input_desktop() -> Result<String> {
    unsafe {
        let hdesk = OpenInputDesktop(
            Default::default(),
            false,
            windows::Win32::System::StationsAndDesktops::DESKTOP_ACCESS_FLAGS(
                windows::Win32::Foundation::GENERIC_ALL.0,
            ),
        )
        .context("OpenInputDesktop(GENERIC_ALL)")?;
        let owned = OwnedDesktop(hdesk);
        let name = name_of(&owned)?;
        SetThreadDesktop(hdesk).context("SetThreadDesktop")?;
        // Deliberately leaked: the desktop must outlive this call for as long
        // as the thread stays attached to it, and closing it here would pull
        // the desktop out from under the thread that is now using it.
        std::mem::forget(owned);
        Ok(name)
    }
}
