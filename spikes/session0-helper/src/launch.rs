//! Getting a process from session 0 into a session that can see a screen.
//!
//! This is the whole spike. A service runs in session 0, which is isolated and
//! has no desktop at all - DXGI returns DXGI_ERROR_NOT_CURRENTLY_AVAILABLE
//! there and always will. So the supervisor/helper split is not a design
//! preference; there is no single-process version of this.
//!
//! Reference implementations worth reading, all more battle-tested than this:
//! MeshAgent `microstack/ILibProcessPipe.c` (Apache-2.0, and the one to copy),
//! RustDesk `src/platform/windows.cc`, Sunshine `tools/sunshinesvc.cpp`.

use anyhow::{Context, Result};
use std::ffi::c_void;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Security::{
    DuplicateTokenEx, SecurityImpersonation, TOKEN_ALL_ACCESS, TOKEN_DUPLICATE, TOKEN_QUERY,
    TokenPrimary, TokenSessionId,
};
use windows::Win32::System::Environment::{CreateEnvironmentBlock, DestroyEnvironmentBlock};
use windows::Win32::System::RemoteDesktop::{WTSGetActiveConsoleSessionId, WTSQueryUserToken};
use windows::Win32::System::Threading::{
    CREATE_NEW_CONSOLE, CREATE_UNICODE_ENVIRONMENT, GetCurrentProcess, OpenProcessToken,
    PROCESS_INFORMATION, STARTUPINFOW,
};
use windows::core::PWSTR;

use crate::handle::OwnedHandle;
use crate::logf;

const ROLE: &str = "launch";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn active_console_session() -> u32 {
    unsafe { WTSGetActiveConsoleSessionId() }
}

/// The token of the user logged into `session`, if there is one.
///
/// Needs SeTcbPrivilege, which SYSTEM has and almost nothing else does - this
/// is the single API that makes the whole design possible, and it is also why
/// the supervisor must be a SYSTEM service rather than merely an elevated
/// process.
fn user_token(session: u32) -> Result<OwnedHandle> {
    unsafe {
        let mut token = HANDLE::default();
        WTSQueryUserToken(session, &mut token)
            .with_context(|| format!("WTSQueryUserToken(session {session})"))?;
        Ok(OwnedHandle(token))
    }
}

/// Our own token - we are SYSTEM - retargeted at another session.
///
/// This is the path that matters for the headline question. With nobody logged
/// in there IS no user token, so WTSQueryUserToken fails and a design that
/// depends on it can never capture a logged-out machine. Duplicating the
/// service's own SYSTEM token and moving it to the console session gives a
/// process that can open the Winlogon desktop.
fn system_token_for_session(session: u32) -> Result<OwnedHandle> {
    unsafe {
        let mut own = HANDLE::default();
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_DUPLICATE | TOKEN_QUERY,
            &mut own,
        )
        .context("OpenProcessToken on ourselves")?;
        let own = OwnedHandle(own);

        let mut dup = HANDLE::default();
        DuplicateTokenEx(
            own.0,
            TOKEN_ALL_ACCESS,
            None,
            SecurityImpersonation,
            TokenPrimary,
            &mut dup,
        )
        .context("DuplicateTokenEx of the SYSTEM token")?;
        let dup = OwnedHandle(dup);

        set_session(&dup, session)?;
        Ok(dup)
    }
}

/// Move a token into `session`.
///
/// Easy to omit and expensive to omit. A token obtained in session 0 carries
/// session id 0, and setting `lpDesktop` alone does NOT move it - the process
/// starts in session 0, sees no desktop, and fails in a way that points at the
/// capture code rather than at the token.
fn set_session(token: &OwnedHandle, session: u32) -> Result<()> {
    unsafe {
        windows::Win32::Security::SetTokenInformation(
            token.0,
            TokenSessionId,
            &session as *const u32 as *const c_void,
            std::mem::size_of::<u32>() as u32,
        )
        .with_context(|| format!("SetTokenInformation(TokenSessionId = {session})"))
    }
}

/// Which desktop within the interactive window station to start on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Desktop {
    /// The ordinary user desktop.
    Default,
    /// The lock screen, the logon screen, and the UAC secure desktop.
    ///
    /// Only a process running as LOCAL_SYSTEM may open this; anything else gets
    /// E_ACCESSDENIED. MeshAgent's one-liner is the whole trick:
    ///   `info.lpDesktop = L"Winsta0\\Winlogon";`
    Winlogon,
}

impl Desktop {
    pub fn as_str(self) -> &'static str {
        match self {
            Desktop::Default => r"Winsta0\Default",
            Desktop::Winlogon => r"Winsta0\Winlogon",
        }
    }
}

/// How the child's token was obtained. Recorded because "it worked" and "it
/// worked the way we think" are different claims.
#[derive(Clone, Copy, Debug)]
pub enum TokenSource {
    LoggedInUser,
    SystemRetargeted,
}

pub struct Launched {
    pub pid: u32,
    pub token_source: TokenSource,
}

/// Start `exe args` in `session` on `desktop`.
pub fn launch(exe: &str, args: &str, session: u32, desktop: Desktop) -> Result<Launched> {
    // Prefer the logged-in user's token so the helper runs with that user's
    // profile and environment. Fall back to a retargeted SYSTEM token, which is
    // the only option when nobody is logged in - and the only one that can open
    // the Winlogon desktop.
    let (token, token_source) = match user_token(session) {
        Ok(t) if desktop == Desktop::Default => {
            set_session(&t, session)?;
            (t, TokenSource::LoggedInUser)
        }
        Ok(_) | Err(_) => (
            system_token_for_session(session)?,
            TokenSource::SystemRetargeted,
        ),
    };

    unsafe {
        // Without an environment block the child gets no %APPDATA%, no %TEMP%,
        // and a broken profile - which surfaces much later as unrelated-looking
        // failures inside whatever the child tries to do.
        let mut env: *mut c_void = std::ptr::null_mut();
        let have_env = CreateEnvironmentBlock(&mut env, Some(token.0), false).is_ok();

        let mut desktop_w = wide(desktop.as_str());
        let mut cmd = wide(&format!("\"{exe}\" {args}"));

        let si = STARTUPINFOW {
            cb: std::mem::size_of::<STARTUPINFOW>() as u32,
            lpDesktop: PWSTR(desktop_w.as_mut_ptr()),
            ..Default::default()
        };
        let mut pi = PROCESS_INFORMATION::default();

        let result = windows::Win32::System::Threading::CreateProcessAsUserW(
            Some(token.0),
            None,
            Some(PWSTR(cmd.as_mut_ptr())),
            None,
            None,
            false,
            CREATE_UNICODE_ENVIRONMENT | CREATE_NEW_CONSOLE,
            if have_env { Some(env) } else { None },
            None,
            &si,
            &mut pi,
        );

        if have_env {
            let _ = DestroyEnvironmentBlock(env);
        }

        result.with_context(|| {
            format!("CreateProcessAsUserW into session {session} on {}", desktop.as_str())
        })?;

        let pid = pi.dwProcessId;
        let _ = windows::Win32::Foundation::CloseHandle(pi.hThread);
        let _ = windows::Win32::Foundation::CloseHandle(pi.hProcess);

        logf!(
            ROLE,
            "launched pid={pid} session={session} desktop={} token={:?}",
            desktop.as_str(),
            token_source
        );
        Ok(Launched { pid, token_source })
    }
}
