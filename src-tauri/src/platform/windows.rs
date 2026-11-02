//! Windows process probe: creation time via GetProcessTimes, parent PID via a
//! Toolhelp32 snapshot that is cached for a short TTL (Monitor asks once per
//! session per tick; one snapshot serves them all).
use pano_core::registry::ProcessProbe;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{CloseHandle, FILETIME, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};

const PARENT_MAP_TTL: Duration = Duration::from_millis(500);

pub struct WinProbe {
    parents: Mutex<Option<(Instant, HashMap<u32, u32>)>>,
}

impl WinProbe {
    pub fn new() -> Self {
        Self { parents: Mutex::new(None) }
    }
}

/// Owned Win32 handle, closed on drop.
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

fn snapshot_parents() -> HashMap<u32, u32> {
    let mut map = HashMap::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snap == INVALID_HANDLE_VALUE {
            return map;
        }
        let snap = Handle(snap);
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(snap.0, &mut entry);
        while ok != 0 {
            map.insert(entry.th32ProcessID, entry.th32ParentProcessID);
            ok = Process32NextW(snap.0, &mut entry);
        }
    }
    map
}

impl ProcessProbe for WinProbe {
    fn start_filetime(&self, pid: u32) -> Option<u64> {
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return None;
            }
            let h = Handle(h);
            let zero = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
            let (mut creation, mut exit, mut kernel, mut user) = (zero, zero, zero, zero);
            if GetProcessTimes(h.0, &mut creation, &mut exit, &mut kernel, &mut user) == 0 {
                return None;
            }
            Some(((creation.dwHighDateTime as u64) << 32) | creation.dwLowDateTime as u64)
        }
    }

    fn parent_pid(&self, pid: u32) -> Option<u32> {
        let mut guard = self.parents.lock().ok()?;
        let stale = guard.as_ref().is_none_or(|(t, _)| t.elapsed() >= PARENT_MAP_TTL);
        if stale {
            *guard = Some((Instant::now(), snapshot_parents()));
        }
        guard.as_ref().and_then(|(_, m)| m.get(&pid).copied())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_process_start_time_is_some() {
        assert!(WinProbe::new().start_filetime(std::process::id()).is_some());
    }
    #[test]
    fn own_parent_pid_is_some() {
        assert!(WinProbe::new().parent_pid(std::process::id()).is_some());
    }
    #[test]
    fn nonexistent_pid_is_none() {
        let p = WinProbe::new();
        assert!(p.start_filetime(0xFFFF_FFF0).is_none());
        assert!(p.parent_pid(0xFFFF_FFF0).is_none());
    }
}
