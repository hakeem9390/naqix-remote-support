//! File logging.
//!
//! Not a nicety. Everything this spike tests happens when you cannot see the
//! screen - the machine is locked, logged out, or showing a secure desktop that
//! by design does not render into your capture. The log is the only instrument,
//! so it records what was attempted and what Windows said, on both processes.

use std::fmt::Write as _;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::PathBuf;
use windows::Win32::System::SystemInformation::GetLocalTime;

pub fn log_path() -> PathBuf {
    // ProgramData, not the user profile: the service runs as SYSTEM and the
    // helper may run as a different user or as SYSTEM on the secure desktop.
    // They must all write to one file or the sequence is unreconstructable.
    PathBuf::from(r"C:\ProgramData\naqix-spike-a\spike.log")
}

pub fn log(role: &str, msg: &str) {
    let st = unsafe { GetLocalTime() };
    let mut line = String::new();
    let _ = writeln!(
        line,
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:03} [{}] pid={} {}",
        st.wYear, st.wMonth, st.wDay, st.wHour, st.wMinute, st.wSecond, st.wMilliseconds,
        role,
        std::process::id(),
        msg
    );

    let path = log_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = f.write_all(line.as_bytes());
    }
    // Also to stdout, so `spike-a helper` can be run by hand in a console
    // before anything is installed as a service.
    print!("{line}");
}

#[macro_export]
macro_rules! logf {
    ($role:expr, $($arg:tt)*) => {
        $crate::log::log($role, &format!($($arg)*))
    };
}
