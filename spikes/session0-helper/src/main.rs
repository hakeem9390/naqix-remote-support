//! Spike A — can a SYSTEM service get frames from the lock screen, a logged-out
//! machine, and the UAC secure desktop?
//!
//! Ranked most-likely-fatal in the original plan, because it has no ecosystem
//! support, cannot be tested in CI, and fails only on real customer machines.
//! Largely DE-RISKED by choosing MeshCentral: MeshAgent already solves this and
//! is Apache-2.0, so this exists to build the understanding a from-scratch
//! build would need - and to check the claim rather than take it on faith.
//!
//! KILL CRITERION: if you cannot get frames from the lock screen and from a
//! logged-out machine, the unattended value proposition is gone. Stop and
//! self-host RustDesk.
//!
//! Usage (all from an elevated prompt):
//!   spike-a helper <tag>   run the capture side directly, no service involved
//!   spike-a probe          print session and desktop state and exit
//!   spike-a install        register the service (auto-start)
//!   spike-a uninstall      remove it
//!   spike-a service        service entry point; Windows calls this, you do not

mod desktop;
mod handle;
mod helper;
mod launch;
pub mod log;
mod service;

use anyhow::{Result, anyhow};
use std::ffi::OsString;
use std::time::Duration;
use windows_service::service::{
    ServiceAccess, ServiceErrorControl, ServiceInfo, ServiceStartType, ServiceType,
};
use windows_service::service_manager::{ServiceManager, ServiceManagerAccess};

const ROLE: &str = "main";

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("service") => service::run(),
        Some("helper") => helper::run(args.get(2).map(String::as_str).unwrap_or("manual")),
        Some("probe") => probe(),
        Some("install") => install(),
        Some("uninstall") => uninstall(),
        _ => {
            eprintln!("usage: spike-a [probe|helper <tag>|install|uninstall|service]");
            eprintln!("start with `probe`, then `helper manual`, before installing anything.");
            Ok(())
        }
    }
}

/// Cheapest possible orientation. Run this first, and run it again while
/// locked, to see what the machine reports before any service is involved.
fn probe() -> Result<()> {
    let session = launch::active_console_session();
    logf!(ROLE, "active console session = {session}");
    match desktop::input_desktop_name() {
        Ok(name) => logf!(ROLE, "input desktop = '{name}'"),
        Err(e) => logf!(
            ROLE,
            "input desktop unavailable: {e:#} (expected unless running as SYSTEM)"
        ),
    }
    match desktop::current_desktop_name() {
        Ok(name) => logf!(ROLE, "this thread's desktop = '{name}'"),
        Err(e) => logf!(ROLE, "current desktop unknown: {e:#}"),
    }
    logf!(ROLE, "log file = {}", log::log_path().display());
    logf!(ROLE, "frames   = {}", helper::out_dir().display());
    Ok(())
}

fn install() -> Result<()> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CREATE_SERVICE)
        .map_err(|e| anyhow!("open SCM (are you elevated?): {e}"))?;

    let exe = std::env::current_exe()?;
    let info = ServiceInfo {
        name: OsString::from(service::SERVICE_NAME),
        display_name: OsString::from("Naqix Spike A (session 0 capture probe)"),
        service_type: ServiceType::OWN_PROCESS,
        // AutoStart on purpose: the interesting test is a machine rebooted with
        // nobody logged in, which only happens if the service starts on its own.
        start_type: ServiceStartType::AutoStart,
        error_control: ServiceErrorControl::Normal,
        executable_path: exe,
        launch_arguments: vec![OsString::from("service")],
        dependencies: vec![],
        // LocalSystem is not a convenience. Only a LOCAL_SYSTEM process may open
        // the secure desktop; anything else gets E_ACCESSDENIED.
        account_name: None,
        account_password: None,
    };

    let service = manager
        .create_service(&info, ServiceAccess::CHANGE_CONFIG | ServiceAccess::START)
        .map_err(|e| anyhow!("create service: {e}"))?;
    service
        .set_description("Spike A: probes lock screen / logged-out / UAC capture. Uninstall when finished.")
        .ok();
    service.start::<OsString>(&[]).map_err(|e| anyhow!("start service: {e}"))?;

    logf!(ROLE, "installed and started {}", service::SERVICE_NAME);
    println!("watch: Get-Content -Wait '{}'", log::log_path().display());
    Ok(())
}

fn uninstall() -> Result<()> {
    let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)
        .map_err(|e| anyhow!("open SCM: {e}"))?;
    let service = manager
        .open_service(
            service::SERVICE_NAME,
            ServiceAccess::STOP | ServiceAccess::DELETE | ServiceAccess::QUERY_STATUS,
        )
        .map_err(|e| anyhow!("open service: {e}"))?;
    let _ = service.stop();
    // The SCM only deletes once every handle closes, so a delete that appears
    // to do nothing usually just needs a moment and a closed Services window.
    std::thread::sleep(Duration::from_millis(500));
    service.delete().map_err(|e| anyhow!("delete service: {e}"))?;
    logf!(ROLE, "uninstalled {}", service::SERVICE_NAME);
    Ok(())
}
