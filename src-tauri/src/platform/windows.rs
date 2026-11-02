//! Windows process probe: creation time via GetProcessTimes, parent PID and
//! exe name via a Toolhelp32 snapshot that is cached for a short TTL (Monitor
//! asks once per session per tick; one snapshot serves them all). The host
//! kind of each pid is computed once per snapshot.
use pano_core::host::{find_host, Host, HostKind, ProcInfo};
use pano_core::registry::ProcessProbe;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_NO_MORE_ITEMS, ERROR_SUCCESS, FILETIME, HANDLE, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegEnumValueW, RegOpenKeyExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;
use windows_sys::Win32::System::SystemInformation::{GetSystemDirectoryW, GetWindowsDirectoryW};

/// `GetDriveTypeW` results (WindowsProgramming constants).
const DRIVE_REMOVABLE: u32 = 2;
const DRIVE_FIXED: u32 = 3;
const DRIVE_REMOTE: u32 = 4;

pub(super) fn drive_kind(root: &Path) -> super::DriveKind {
    let wide: Vec<u16> = root.as_os_str().encode_wide().chain(std::iter::once(0)).collect();
    match unsafe { GetDriveTypeW(wide.as_ptr()) } {
        DRIVE_FIXED | DRIVE_REMOVABLE => super::DriveKind::Local,
        DRIVE_REMOTE => super::DriveKind::Remote,
        _ => super::DriveKind::Other,
    }
}

/// Calls a `Get*DirectoryW` style API; `fallback` when it fails.
fn api_dir(get: unsafe extern "system" fn(*mut u16, u32) -> u32, fallback: &str) -> PathBuf {
    let mut buf = vec![0u16; 260];
    loop {
        let n = unsafe { get(buf.as_mut_ptr(), buf.len() as u32) } as usize;
        if n == 0 || n > 32 * 1024 {
            return PathBuf::from(fallback);
        }
        if n < buf.len() {
            let p = PathBuf::from(std::ffi::OsString::from_wide(&buf[..n]));
            return if p.is_absolute() { p } else { PathBuf::from(fallback) };
        }
        // Too small: `n` is the size needed, including the terminator.
        buf.resize(n, 0);
    }
}

pub(super) fn system_dir() -> PathBuf {
    api_dir(GetSystemDirectoryW, r"C:\Windows\System32")
}

pub(super) fn windows_dir() -> PathBuf {
    api_dir(GetWindowsDirectoryW, r"C:\Windows")
}

const PARENT_MAP_TTL: Duration = Duration::from_millis(500);

struct ProcCache {
    at: Instant,
    table: HashMap<u32, ProcInfo>,
    hosts: HashMap<u32, HostKind>,
}

pub struct WinProbe {
    procs: Mutex<Option<ProcCache>>,
}

impl WinProbe {
    pub fn new() -> Self {
        Self { procs: Mutex::new(None) }
    }

    /// Runs `f` on a process table no older than PARENT_MAP_TTL.
    fn with_procs<R>(&self, f: impl FnOnce(&mut ProcCache) -> R) -> Option<R> {
        let mut guard = self.procs.lock().ok()?;
        let stale = guard.as_ref().is_none_or(|c| c.at.elapsed() >= PARENT_MAP_TTL);
        if stale {
            *guard = Some(ProcCache { at: Instant::now(), table: snapshot_procs(), hosts: HashMap::new() });
        }
        guard.as_mut().map(f)
    }
}

/// Owned Win32 handle, closed on drop.
struct Handle(HANDLE);
impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

/// pid -> (parent pid, exe file name) of every process.
pub(super) fn snapshot_procs() -> HashMap<u32, ProcInfo> {
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
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            let exe = String::from_utf16_lossy(&entry.szExeFile[..len]);
            map.insert(entry.th32ProcessID, ProcInfo { parent: entry.th32ParentProcessID, exe });
            ok = Process32NextW(snap.0, &mut entry);
        }
    }
    map
}

/// Creation FILETIME of `pid`, if it can be opened.
pub(super) fn creation_time(pid: u32) -> Option<u64> {
    WinProbe::new().start_filetime(pid)
}

/// Host of `pid` over `table` (this process counts as the embedded host).
pub(super) fn host_of(pid: u32, table: &HashMap<u32, ProcInfo>) -> Host {
    find_host(pid, std::process::id(), table, creation_time)
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
        self.with_procs(|c| c.table.get(&pid).map(|p| p.parent)).flatten()
    }

    fn host_kind(&self, pid: u32) -> HostKind {
        self.with_procs(|c| {
            if let Some(k) = c.hosts.get(&pid) {
                return *k;
            }
            let k = host_of(pid, &c.table).kind();
            c.hosts.insert(pid, k);
            k
        })
        .unwrap_or_default()
    }
}

/// Value names under one registry key (read-only), uppercased.
fn value_names(root: HKEY, subkey: &str, out: &mut HashSet<String>) {
    let wide: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        let mut key: HKEY = std::ptr::null_mut();
        if RegOpenKeyExW(root, wide.as_ptr(), 0, KEY_QUERY_VALUE, &mut key) != ERROR_SUCCESS {
            return;
        }
        let mut buf = vec![0u16; 16384];
        let mut i = 0u32;
        loop {
            let mut len = buf.len() as u32;
            let r = RegEnumValueW(
                key,
                i,
                buf.as_mut_ptr(),
                &mut len,
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            if r == ERROR_NO_MORE_ITEMS {
                break;
            }
            if r == ERROR_SUCCESS {
                out.insert(String::from_utf16_lossy(&buf[..len as usize]).to_ascii_uppercase());
            }
            i += 1;
            if i > 100_000 {
                break;
            }
        }
        RegCloseKey(key);
    }
}

/// Names of the persistent user and machine environment variables.
pub fn persistent_env_names() -> HashSet<String> {
    let mut out = HashSet::new();
    value_names(HKEY_CURRENT_USER, "Environment", &mut out);
    value_names(
        HKEY_LOCAL_MACHINE,
        r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
        &mut out,
    );
    out
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
    fn snapshot_has_exe_names_and_own_process_is_not_hosted() {
        let t = snapshot_procs();
        let me = t.get(&std::process::id()).expect("own process listed");
        assert!(me.exe.to_ascii_lowercase().ends_with(".exe"), "{}", me.exe);
        assert_eq!(WinProbe::new().host_kind(0xFFFF_FFF0), HostKind::Other);
    }

    #[test]
    fn child_of_this_process_is_embedded() {
        let mut child = std::process::Command::new("cmd.exe")
            .args(["/c", "ping -n 5 127.0.0.1 >nul"])
            .spawn()
            .unwrap();
        let kind = WinProbe::new().host_kind(child.id());
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(kind, HostKind::Embedded);
    }

    #[test]
    fn persistent_env_has_machine_path() {
        // Path is always a persistent machine variable.
        let n = persistent_env_names();
        assert!(n.contains("PATH"), "{n:?}");
        assert!(n.iter().all(|k| *k == k.to_ascii_uppercase()));
    }

    #[test]
    fn nonexistent_pid_is_none() {
        let p = WinProbe::new();
        assert!(p.start_filetime(0xFFFF_FFF0).is_none());
        assert!(p.parent_pid(0xFFFF_FFF0).is_none());
    }
}
