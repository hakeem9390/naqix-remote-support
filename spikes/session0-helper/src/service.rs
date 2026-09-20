//! The supervisor: a SYSTEM service in session 0 that keeps a helper running
//! on whichever desktop currently has input.
//!
//! Session 0 can see no screen, so this process never captures anything. Its
//! entire job is to notice desktop and session changes and respawn the helper
//! on the other side of that boundary.

use anyhow::{Result, anyhow};
use std::ffi::OsString;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use windows_service::service::{
    ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus,
    ServiceType,
};
use windows_service::service_control_handler::{self, ServiceControlHandlerResult};
use windows_service::{define_windows_service, service_dispatcher};

use crate::launch::{self, Desktop};
use crate::logf;

pub const SERVICE_NAME: &str = "NaqixSpikeA";
const ROLE: &str = "service";

define_windows_service!(ffi_service_main, service_main);

pub fn run() -> Result<()> {
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)
        .map_err(|e| anyhow!("service_dispatcher::start: {e}"))
}

fn service_main(_args: Vec<OsString>) {
    if let Err(e) = serve() {
        logf!(ROLE, "service exited with error: {e:#}");
    }
}

fn serve() -> Result<()> {
    let stop = Arc::new(AtomicBool::new(false));

    let handler_stop = Arc::clone(&stop);
    let handler = move |control| -> ServiceControlHandlerResult {
        match control {
            ServiceControl::Stop | ServiceControl::Shutdown => {
                handler_stop.store(true, Ordering::SeqCst);
                ServiceControlHandlerResult::NoError
            }
            ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,
            // Logging these is half the value of the spike: they say exactly
            // when Windows thinks a lock, logoff or fast-user-switch happened,
            // which is what the captured frames get correlated against.
            ServiceControl::SessionChange(param) => {
                logf!(
                    ROLE,
                    "SESSION CHANGE {:?} session={}",
                    param.reason,
                    param.notification.session_id
                );
                ServiceControlHandlerResult::NoError
            }
            _ => ServiceControlHandlerResult::NotImplemented,
        }
    };

    let status_handle = service_control_handler::register(SERVICE_NAME, handler)
        .map_err(|e| anyhow!("register handler: {e}"))?;

    let running = |state, accept| ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: state,
        controls_accepted: accept,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    };

    status_handle.set_service_status(running(
        ServiceState::Running,
        // SESSION_CHANGE must be declared or the notifications never arrive.
        ServiceControlAccept::STOP | ServiceControlAccept::SESSION_CHANGE,
    ))?;

    logf!(ROLE, "service started; log is {}", crate::log::log_path().display());
    supervise(&stop);

    status_handle.set_service_status(running(
        ServiceState::Stopped,
        ServiceControlAccept::empty(),
    ))?;
    Ok(())
}

/// Keep a helper alive on the desktop that currently has input.
fn supervise(stop: &AtomicBool) {
    let exe = std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    let mut current: Option<(String, u32)> = None;

    while !stop.load(Ordering::SeqCst) {
        let session = launch::active_console_session();

        // 0xFFFFFFFF means no console session is attached at all, which happens
        // transiently around logoff and switch-user. Respawning into it fails
        // noisily and pointlessly, so wait it out.
        if session == u32::MAX {
            if current.take().is_some() {
                logf!(ROLE, "no console session attached; helper dropped");
            }
            std::thread::sleep(Duration::from_millis(500));
            continue;
        }

        let desk = match crate::desktop::input_desktop_name() {
            Ok(name) => name,
            Err(e) => {
                logf!(ROLE, "input_desktop_name failed: {e:#}");
                std::thread::sleep(Duration::from_millis(500));
                continue;
            }
        };

        let changed = match &current {
            Some((name, sess)) => name != &desk || *sess != session,
            None => true,
        };

        if changed {
            logf!(ROLE, "input desktop is '{desk}' on session {session}; (re)starting helper");
            // Winlogon covers the lock screen, the logon screen and the UAC
            // secure desktop. Anything else is treated as the user desktop.
            let target = if desk.eq_ignore_ascii_case("Winlogon") {
                Desktop::Winlogon
            } else {
                Desktop::Default
            };
            let tag = format!("{}-s{}", desk.to_lowercase(), session);
            match launch::launch(&exe, &format!("helper {tag}"), session, target) {
                Ok(l) => {
                    logf!(ROLE, "helper pid={} token={:?}", l.pid, l.token_source);
                    current = Some((desk, session));
                }
                Err(e) => {
                    // The headline failure. If this is where it dies on a
                    // logged-out machine, the unattended proposition is gone.
                    logf!(ROLE, "LAUNCH FAILED on '{desk}': {e:#}");
                    std::thread::sleep(Duration::from_secs(2));
                }
            }
        }

        // 250ms: the switch to the secure desktop raises no event we can
        // receive, so this poll is the only way to notice it.
        std::thread::sleep(Duration::from_millis(250));
    }

    logf!(ROLE, "supervisor stopping");
}
