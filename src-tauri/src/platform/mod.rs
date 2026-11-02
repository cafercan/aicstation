//! Platform selection for the process probe.
use pano_core::registry::ProcessProbe;

#[cfg(windows)]
mod windows;

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
