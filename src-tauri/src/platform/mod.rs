//! Platform selection for the process probe, drive kinds and system paths.
use station_core::registry::ProcessProbe;
#[cfg(windows)]
use std::path::{Path, PathBuf};

/// What kind of drive a local path (`X:\...`) is on.
#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveKind {
    /// A fixed or removable disk.
    Local,
    /// A mapped network drive.
    Remote,
    /// Anything else (optical, RAM disk, no such root, unknown).
    Other,
}

/// Kind of the drive whose root (`X:\`) is `root`.
#[cfg(windows)]
pub fn drive_kind(root: &Path) -> DriveKind {
    windows::drive_kind(root)
}

/// The System32 directory from `GetSystemDirectoryW` (never `%SystemRoot%`,
/// which the parent process controls).
#[cfg(windows)]
pub fn system_dir() -> PathBuf {
    windows::system_dir()
}

/// The Windows directory from `GetWindowsDirectoryW`.
#[cfg(windows)]
pub fn windows_dir() -> PathBuf {
    windows::windows_dir()
}

/// Absolute path of explorer.exe (never resolved through PATH or the cwd).
#[cfg(windows)]
pub fn explorer_exe() -> PathBuf {
    windows_dir().join("explorer.exe")
}

#[cfg(windows)]
mod focus;
/// Single-instance guard (named mutex + activation event).
#[cfg(windows)]
pub mod single;
#[cfg(windows)]
mod windows;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(unix)]
mod unix;
#[cfg(unix)]
pub use unix::{open_external_terminal, open_folder};
#[cfg(unix)]
pub(crate) use unix::find_program;

/// Opens `dir` (already validated) in Explorer.
#[cfg(windows)]
pub fn open_folder(dir: &Path) -> std::io::Result<()> {
    std::process::Command::new(explorer_exe()).arg(dir).spawn().map(|_| ())
}

/// Brings the Windows Terminal window hosting `pid` forward and selects the
/// tab titled `name`. Err when the process is not in a Windows Terminal.
#[cfg(windows)]
pub fn focus_session_tab(pid: u32, name: &str) -> Result<Option<String>, String> {
    focus::focus_session_tab(pid, name)
}

/// Uppercased names of the persistent user (HKCU\Environment) and machine
/// environment variables, read-only from the registry.
#[cfg(windows)]
pub fn persistent_env_names() -> std::collections::HashSet<String> {
    windows::persistent_env_names()
}

#[cfg(not(windows))]
pub fn persistent_env_names() -> std::collections::HashSet<String> {
    std::collections::HashSet::new()
}

#[cfg(not(windows))]
pub fn focus_session_tab(_pid: u32, _name: &str) -> Result<Option<String>, String> {
    Err(crate::errors::TAB_NOT_FOUND.into())
}

#[cfg(windows)]
pub fn probe() -> Box<dyn ProcessProbe + Send + Sync> {
    Box::new(windows::WinProbe::new())
}

#[cfg(target_os = "linux")]
pub fn probe() -> Box<dyn ProcessProbe + Send + Sync> {
    Box::new(linux::LinuxProbe::new())
}

/// Stub for other platforms: knows no process, so no session is ever live.
#[cfg(not(any(windows, target_os = "linux")))]
struct NullProbe;

#[cfg(not(any(windows, target_os = "linux")))]
impl ProcessProbe for NullProbe {
    fn start_filetime(&self, _pid: u32) -> Option<u64> {
        None
    }
    fn parent_pid(&self, _pid: u32) -> Option<u32> {
        None
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
pub fn probe() -> Box<dyn ProcessProbe + Send + Sync> {
    Box::new(NullProbe)
}
