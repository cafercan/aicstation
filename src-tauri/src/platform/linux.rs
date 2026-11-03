//! Linux process probe over `/proc`. Claude Code writes `procStart` as the
//! `starttime` field of `/proc/<pid>/stat` (clock ticks since boot), so the
//! same value read back here guards against pid reuse exactly as the
//! creation FILETIME does on Windows. The process table (parent pid and
//! `comm`) is cached for a short TTL, like the Toolhelp snapshot.
use station_core::host::{find_host, HostKind, ProcInfo};
use station_core::registry::ProcessProbe;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const TABLE_TTL: Duration = Duration::from_millis(500);

/// The fields of `/proc/<pid>/stat` this probe needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Stat {
    pub comm: String,
    pub ppid: u32,
    pub starttime: u64,
}

/// Parses one `/proc/<pid>/stat` line. `comm` may itself contain spaces and
/// parentheses, so the fields after it are taken from the LAST `)`.
pub(super) fn parse_stat(line: &str) -> Option<Stat> {
    let open = line.find('(')?;
    let close = line.rfind(')')?;
    if close < open {
        return None;
    }
    let comm = line[open + 1..close].to_string();
    // Fields after comm start at field 3 (state); ppid is field 4 and
    // starttime field 22, i.e. indices 1 and 19 here.
    let rest: Vec<&str> = line[close + 1..].split_whitespace().collect();
    Some(Stat { comm, ppid: rest.get(1)?.parse().ok()?, starttime: rest.get(19)?.parse().ok()? })
}

fn read_stat(pid: u32) -> Option<Stat> {
    parse_stat(&std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?)
}

/// pid -> (parent pid, comm) of every process visible in `/proc`.
fn snapshot_procs() -> HashMap<u32, ProcInfo> {
    let mut map = HashMap::new();
    let Ok(dir) = std::fs::read_dir("/proc") else { return map };
    for e in dir.flatten() {
        let Some(pid) = e.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else { continue };
        if let Some(s) = read_stat(pid) {
            map.insert(pid, ProcInfo { parent: s.ppid, exe: s.comm });
        }
    }
    map
}

struct ProcCache {
    at: Instant,
    table: HashMap<u32, ProcInfo>,
    hosts: HashMap<u32, HostKind>,
}

pub struct LinuxProbe {
    procs: Mutex<Option<ProcCache>>,
}

impl LinuxProbe {
    pub fn new() -> Self {
        Self { procs: Mutex::new(None) }
    }

    fn with_procs<R>(&self, f: impl FnOnce(&mut ProcCache) -> R) -> Option<R> {
        let mut guard = self.procs.lock().ok()?;
        if guard.as_ref().is_none_or(|c| c.at.elapsed() >= TABLE_TTL) {
            *guard = Some(ProcCache { at: Instant::now(), table: snapshot_procs(), hosts: HashMap::new() });
        }
        guard.as_mut().map(f)
    }
}

impl ProcessProbe for LinuxProbe {
    fn start_filetime(&self, pid: u32) -> Option<u64> {
        read_stat(pid).map(|s| s.starttime)
    }

    fn parent_pid(&self, pid: u32) -> Option<u32> {
        self.with_procs(|c| c.table.get(&pid).map(|p| p.parent)).flatten()
    }

    fn host_kind(&self, pid: u32) -> HostKind {
        self.with_procs(|c| {
            if let Some(k) = c.hosts.get(&pid) {
                return *k;
            }
            let k = find_host(pid, std::process::id(), &c.table, |p| read_stat(p).map(|s| s.starttime)).kind();
            c.hosts.insert(pid, k);
            k
        })
        .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_line_with_spaces_and_parens_in_comm() {
        let line = "4242 (my (odd) prog) S 17 4242 4242 0 -1 4194560 100 0 0 0 1 2 0 0 20 0 1 0 837561 1000 10";
        assert_eq!(parse_stat(line), Some(Stat { comm: "my (odd) prog".into(), ppid: 17, starttime: 837561 }));
        assert_eq!(parse_stat("garbage"), None);
        assert_eq!(parse_stat("1 (x) S"), None);
    }

    #[test]
    fn own_process_is_readable() {
        let p = LinuxProbe::new();
        assert!(p.start_filetime(std::process::id()).is_some());
        assert_eq!(p.parent_pid(std::process::id()), Some(std::os::unix::process::parent_id()));
        assert_eq!(p.host_kind(0xFFFF_FFF0), HostKind::Other);
    }

    #[test]
    fn child_of_this_process_is_embedded() {
        let mut child = std::process::Command::new("/bin/sh").args(["-c", "sleep 5"]).spawn().unwrap();
        let kind = LinuxProbe::new().host_kind(child.id());
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(kind, HostKind::Embedded);
    }
}
