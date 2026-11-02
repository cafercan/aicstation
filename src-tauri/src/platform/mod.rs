//! Platform selection for the process probe.
use pano_core::registry::ProcessProbe;

#[cfg(windows)]
mod focus;
#[cfg(windows)]
mod windows;

/// Brings the Windows Terminal window hosting `pid` forward and selects the
/// tab titled `name`. Err when the process is not in a Windows Terminal.
#[cfg(windows)]
pub fn focus_session_tab(pid: u32, name: &str) -> Result<Option<String>, String> {
    focus::focus_session_tab(pid, name)
}

#[cfg(not(windows))]
pub fn focus_session_tab(_pid: u32, _name: &str) -> Result<Option<String>, String> {
    Err("Sekme bulunamadı".into())
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
