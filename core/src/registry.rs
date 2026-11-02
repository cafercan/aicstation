//! Live session registry: `~/.claude/sessions/<pid>.json` -> `LiveSession`.
use serde::{Deserialize, Serialize, Serializer};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionStatus {
    Busy,
    Waiting,
    Idle,
    Other(String),
}

impl Serialize for SessionStatus {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(match self {
            SessionStatus::Busy => "busy",
            SessionStatus::Waiting => "waiting",
            SessionStatus::Idle => "idle",
            SessionStatus::Other(o) => o,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveSession {
    pub pid: u32,
    pub session_id: String,
    pub cwd: String,
    pub name: Option<String>,
    pub status: SessionStatus,
    pub waiting_for: Option<String>,
    pub started_at_ms: u64,
    pub status_updated_at_ms: u64,
    pub proc_start: u64,
}

pub trait ProcessProbe {
    fn start_filetime(&self, pid: u32) -> Option<u64>;
    fn parent_pid(&self, pid: u32) -> Option<u32>;
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawSession {
    pid: u32,
    session_id: String,
    #[serde(default)]
    cwd: String,
    name: Option<String>,
    status: Option<String>,
    waiting_for: Option<String>,
    started_at: Option<u64>,
    updated_at: Option<u64>,
    status_updated_at: Option<u64>,
    proc_start: Option<String>,
}

fn parse_status(s: Option<String>) -> SessionStatus {
    match s.as_deref() {
        Some("busy") => SessionStatus::Busy,
        Some("waiting") => SessionStatus::Waiting,
        Some("idle") => SessionStatus::Idle,
        Some(o) => SessionStatus::Other(o.to_string()),
        None => SessionStatus::Other(String::new()),
    }
}

/// Reads every `*.json` in `sessions_dir` (never `*.key`), keeping only
/// sessions whose process is alive and whose creation time matches
/// `procStart` (guards against PID reuse). Unreadable/bad files are skipped.
pub fn read_live_sessions(sessions_dir: &Path, probe: &dyn ProcessProbe) -> Vec<LiveSession> {
    let Ok(entries) = std::fs::read_dir(sessions_dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let is_json = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("json"));
        if !is_json {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(raw) = serde_json::from_str::<RawSession>(&text) else {
            continue;
        };
        let Some(proc_start) = raw.proc_start.as_deref().and_then(|s| s.trim().parse::<u64>().ok())
        else {
            continue;
        };
        if probe.start_filetime(raw.pid) != Some(proc_start) {
            continue;
        }
        let started = raw.started_at.unwrap_or(0);
        out.push(LiveSession {
            pid: raw.pid,
            session_id: raw.session_id,
            cwd: raw.cwd,
            name: raw.name,
            status: parse_status(raw.status),
            waiting_for: raw.waiting_for,
            started_at_ms: started,
            status_updated_at_ms: raw.status_updated_at.or(raw.updated_at).unwrap_or(started),
            proc_start,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::fs;

    struct FakeProbe(HashMap<u32, u64>);
    impl ProcessProbe for FakeProbe {
        fn start_filetime(&self, pid: u32) -> Option<u64> {
            self.0.get(&pid).copied()
        }
        fn parent_pid(&self, _pid: u32) -> Option<u32> {
            None
        }
    }

    fn json(pid: u32, status: &str, extra: &str) -> String {
        format!(
            r#"{{"pid":{pid},"sessionId":"sid-{pid}","cwd":"D:\\Projects\\ornek","startedAt":1000,"procStart":"777","version":"1.0.0","kind":"interactive","entrypoint":"cli","name":"ornek","status":"{status}","updatedAt":2000,"statusUpdatedAt":3000,"futureField":{{"a":1}}{extra}}}"#
        )
    }

    fn probe(pid: u32, ft: u64) -> FakeProbe {
        FakeProbe(HashMap::from([(pid, ft)]))
    }

    #[test]
    fn reads_live_session_fields() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("10.json"), json(10, "busy", "")).unwrap();
        fs::write(
            d.path().join("11.json"),
            json(11, "waiting", r#","waitingFor":"input needed""#),
        )
        .unwrap();
        let p = FakeProbe(HashMap::from([(10, 777), (11, 777)]));
        let mut v = read_live_sessions(d.path(), &p);
        v.sort_by_key(|s| s.pid);
        assert_eq!(v.len(), 2);
        let a = &v[0];
        assert_eq!(a.session_id, "sid-10");
        assert_eq!(a.cwd, r"D:\Projects\ornek");
        assert_eq!(a.name.as_deref(), Some("ornek"));
        assert_eq!(a.status, SessionStatus::Busy);
        assert_eq!(a.waiting_for, None);
        assert_eq!((a.started_at_ms, a.status_updated_at_ms, a.proc_start), (1000, 3000, 777));
        assert_eq!(v[1].status, SessionStatus::Waiting);
        assert_eq!(v[1].waiting_for.as_deref(), Some("input needed"));
    }

    #[test]
    fn dead_pid_is_excluded() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("10.json"), json(10, "idle", "")).unwrap();
        assert!(read_live_sessions(d.path(), &FakeProbe(HashMap::new())).is_empty());
    }

    #[test]
    fn reused_pid_is_excluded() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("10.json"), json(10, "idle", "")).unwrap();
        assert!(read_live_sessions(d.path(), &probe(10, 999)).is_empty());
    }

    #[test]
    fn unknown_status_is_other() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("10.json"), json(10, "compacting", "")).unwrap();
        let v = read_live_sessions(d.path(), &probe(10, 777));
        assert_eq!(v[0].status, SessionStatus::Other("compacting".into()));
        assert_eq!(serde_json::to_string(&v[0].status).unwrap(), "\"compacting\"");
    }

    #[test]
    fn status_time_falls_back_to_updated_then_started() {
        let d = tempfile::tempdir().unwrap();
        let no_status_ts = json(10, "idle", "").replace(r#""statusUpdatedAt":3000,"#, "");
        fs::write(d.path().join("10.json"), no_status_ts).unwrap();
        let v = read_live_sessions(d.path(), &probe(10, 777));
        assert_eq!(v[0].status_updated_at_ms, 2000);
        let neither = json(11, "idle", "")
            .replace(r#""statusUpdatedAt":3000,"#, "")
            .replace(r#""updatedAt":2000,"#, "");
        fs::write(d.path().join("11.json"), neither).unwrap();
        let v = read_live_sessions(d.path(), &probe(11, 777));
        assert_eq!(v[0].status_updated_at_ms, 1000);
    }

    #[test]
    fn registry_skips_key_bad_and_vanished_files() {
        let d = tempfile::tempdir().unwrap();
        fs::write(d.path().join("10.json"), json(10, "idle", "")).unwrap();
        fs::write(d.path().join("x.key"), "gizli").unwrap();
        fs::write(d.path().join("12.key"), json(12, "idle", "")).unwrap();
        fs::write(d.path().join("bozuk.json"), "{\"pid\": 5, \"sess").unwrap();
        // a directory named like a json file: unreadable, must be skipped
        fs::create_dir(d.path().join("dir.json")).unwrap();
        let p = FakeProbe(HashMap::from([(10, 777), (12, 777), (5, 777)]));
        let v = read_live_sessions(d.path(), &p);
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].pid, 10);
        // missing directory: no panic
        assert!(read_live_sessions(&d.path().join("yok"), &p).is_empty());
    }
}
