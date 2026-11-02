//! Platform selection for the process probe, drive kinds and system paths.
use station_core::registry::ProcessProbe;
use std::path::{Path, PathBuf};

/// What kind of drive a local path (`X:\...`) is on.
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

#[cfg(not(windows))]
pub fn drive_kind(_root: &Path) -> DriveKind {
    DriveKind::Local
}

/// The System32 directory from `GetSystemDirectoryW` (never `%SystemRoot%`,
/// which the parent process controls).
#[cfg(windows)]
pub fn system_dir() -> PathBuf {
    windows::system_dir()
}

#[cfg(not(windows))]
pub fn system_dir() -> PathBuf {
    PathBuf::from(r"C:\Windows\System32")
}

/// The Windows directory from `GetWindowsDirectoryW`.
#[cfg(windows)]
pub fn windows_dir() -> PathBuf {
    windows::windows_dir()
}

#[cfg(not(windows))]
pub fn windows_dir() -> PathBuf {
    PathBuf::from(r"C:\Windows")
}

/// Absolute path of explorer.exe (never resolved through PATH or the cwd).
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

/// Non-Windows stub: knows no process, so no session is ever considered live.
#[cfg(not(windows))]
struct NullProbe;

#[cfg(not(windows))]
impl ProcessProbe for NullProbe {
    fn start_filetime(&self, _pid: u32) -> Option<u64> {
        None
    }
    fn parent_pid(&self, _pid: u32) -> Option<u32> {
        None
    }
}

#[cfg(not(windows))]
pub fn probe() -> Box<dyn ProcessProbe + Send + Sync> {
    Box::new(NullProbe)
}
