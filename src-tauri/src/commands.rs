//! IPC commands and the shared application state. Every command is a thin
//! wrapper over a plain function on `AppState` so tests can call the handlers
//! without a webview. All arguments from the UI are untrusted.
use pano_core::history::{HistoryIndex, HistoryRow};
use pano_core::locate::Locator;
use pano_core::monitor::{Monitor, SessionDetail, Snapshot};
use pano_core::registry::ProcessProbe;
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime};

const MAX_ID_LEN: usize = 64;
/// A cached snapshot older than this is recomputed by `get_snapshot` (guards
/// against a stalled background thread).
const SNAPSHOT_MAX_AGE: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryView {
    pub rows: Vec<HistoryRow>,
    pub today_cost: f64,
    pub today_complete: bool,
    pub indexing: bool,
}

/// Published by the background thread after each index step so commands never
/// wait on `HistoryIndex::step`.
#[derive(Default)]
struct HistoryCache {
    rows: Vec<HistoryRow>,
    today_cost: f64,
    today_complete: bool,
}

pub struct AppState {
    monitor: Mutex<Monitor>,
    history: Mutex<HistoryIndex>,
    history_cache: Mutex<HistoryCache>,
    probe: Box<dyn ProcessProbe + Send + Sync>,
    snapshot: Mutex<Option<(Instant, Snapshot)>>,
    indexing: AtomicBool,
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl AppState {
    pub fn new(
        claude_dir: PathBuf,
        cache_path: PathBuf,
        probe: Box<dyn ProcessProbe + Send + Sync>,
    ) -> Self {
        let locator = Locator::new(claude_dir.join("projects"));
        let history = HistoryIndex::load(cache_path, &locator);
        let state = Self {
            monitor: Mutex::new(Monitor::new(claude_dir)),
            history: Mutex::new(history),
            history_cache: Mutex::new(HistoryCache::default()),
            probe,
            snapshot: Mutex::new(None),
            indexing: AtomicBool::new(true),
        };
        state.publish_history();
        state
    }

    /// Rebuilds the cached history view (brief history lock, no indexing).
    pub fn publish_history(&self) {
        let cache = {
            let h = lock(&self.history);
            let (today_cost, today_complete) = h.cost_on(chrono::Local::now().date_naive());
            HistoryCache {
                rows: h.rows(&HashSet::new()).into_iter().cloned().collect(),
                today_cost,
                today_complete,
            }
        };
        *lock(&self.history_cache) = cache;
    }

    /// Recomputes the snapshot and stores it. The monitor lock is released
    /// before the snapshot lock is taken, and before the caller emits.
    pub fn refresh(&self) -> Snapshot {
        let snap = lock(&self.monitor).refresh(self.probe.as_ref(), SystemTime::now());
        *lock(&self.snapshot) = Some((Instant::now(), snap.clone()));
        snap
    }

    pub fn get_snapshot(&self) -> Snapshot {
        let cached = lock(&self.snapshot)
            .as_ref()
            .filter(|(t, _)| t.elapsed() < SNAPSHOT_MAX_AGE)
            .map(|(_, s)| s.clone());
        cached.unwrap_or_else(|| self.refresh())
    }

    pub fn get_session_detail(&self, session_id: &str) -> Option<SessionDetail> {
        if session_id.is_empty() || session_id.len() > MAX_ID_LEN {
            return None;
        }
        let now = SystemTime::now();
        let closed = {
            let mut m = lock(&self.monitor);
            if let Some(d) = m.live_detail(session_id, self.probe.as_ref(), now) {
                return Some(d);
            }
            m.closed_job(session_id)?
        };
        // Transcript streaming happens without the monitor lock.
        closed.run(session_id, now)
    }

    pub fn get_history(&self) -> HistoryView {
        let snap = self.get_snapshot();
        let live: HashSet<String> = snap.sessions.iter().map(|c| c.session_id.clone()).collect();
        let c = lock(&self.history_cache);
        HistoryView {
            rows: c.rows.iter().filter(|r| !live.contains(&r.session_id)).cloned().collect(),
            today_cost: c.today_cost,
            today_complete: c.today_complete,
            indexing: self.indexing.load(Ordering::Relaxed),
        }
    }

    /// Whether history indexing still has work (false once everything is indexed).
    pub fn is_indexing(&self) -> bool {
        self.indexing.load(Ordering::Relaxed)
    }

    /// Runs one indexing slice, then publishes the result. Only the background
    /// thread calls this; commands read the published cache and never wait on it.
    pub fn index_step(&self, budget: Duration) -> bool {
        let more = lock(&self.history).step(budget);
        self.publish_history();
        self.indexing.store(more, Ordering::Relaxed);
        more
    }

    pub fn save_history(&self) -> std::io::Result<()> {
        lock(&self.history).save()
    }

    pub fn open_folder(&self, path: &str) -> Result<(), String> {
        let mut known: Vec<String> =
            self.get_snapshot().sessions.iter().map(|c| c.cwd.clone()).collect();
        {
            let c = lock(&self.history_cache);
            known.extend(c.rows.iter().filter_map(|r| r.cwd.clone()));
        }
        let dir = validate_open_folder(path, &known)?;
        std::process::Command::new("explorer.exe")
            .arg(dir)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Klasör açılamadı: {e}"))
    }
}

/// Accepts only a path equal to a known session cwd that is an absolute,
/// local, existing directory. explorer.exe would execute a file path, treats
/// `,`/leading `/` as switches, and `is_dir` on a UNC path would make an
/// outbound SMB request. The returned path ends with `\` so explorer always
/// treats it as a folder.
pub fn validate_open_folder(path: &str, known_cwds: &[String]) -> Result<PathBuf, String> {
    if path.is_empty() || !known_cwds.iter().any(|c| c == path) {
        return Err("Bilinmeyen klasör".into());
    }
    if path.starts_with(r"\\") || path.starts_with("//") {
        return Err("Ağ yolları desteklenmiyor".into());
    }
    if path.contains(',') || path.starts_with('/') || path.starts_with('-') {
        return Err("Geçersiz yol".into());
    }
    let p = Path::new(path);
    if !p.is_absolute() {
        return Err("Yol mutlak olmalı".into());
    }
    if !p.is_dir() {
        return Err("Yol var olan bir klasör değil".into());
    }
    let mut s = p.as_os_str().to_os_string();
    if !path.ends_with('\\') {
        s.push("\\");
    }
    Ok(PathBuf::from(s))
}

#[tauri::command(async)]
pub fn get_snapshot(state: tauri::State<'_, AppState>) -> Snapshot {
    state.get_snapshot()
}

#[tauri::command(async)]
pub fn get_session_detail(
    state: tauri::State<'_, AppState>,
    session_id: String,
) -> Option<SessionDetail> {
    state.get_session_detail(&session_id)
}

#[tauri::command(async)]
pub fn get_history(state: tauri::State<'_, AppState>) -> HistoryView {
    state.get_history()
}

#[tauri::command(async)]
pub fn open_folder(state: tauri::State<'_, AppState>, path: String) -> Result<(), String> {
    state.open_folder(&path)
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
            Some(1)
        }
    }

    #[test]
    fn open_folder_rejects_unknown_path() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().to_string_lossy().into_owned();
        // An existing directory that no session uses is refused.
        assert!(validate_open_folder(&real, &["D:\\baska".to_string()]).is_err());
        assert!(validate_open_folder("", &[String::new()]).is_err());
        // Known and existing is accepted.
        assert!(validate_open_folder(&real, &[real.clone()]).is_ok());
    }

    #[test]
    fn open_folder_hardening_rejections() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().to_string_lossy().into_owned();
        // Each is listed as "known", so only the hardening rules can reject.
        for bad in [
            r"\\sunucu\paylasim".to_string(),
            "//sunucu/paylasim".to_string(),
            "/windows".to_string(),
            "-Ac".to_string(),
            r"bagil\yol".to_string(),
            format!("{real},/select"),
            r"C:\a,b".to_string(),
        ] {
            assert!(validate_open_folder(&bad, &[bad.clone()]).is_err(), "should reject {bad}");
        }
    }

    #[test]
    fn open_folder_accepts_exact_known_absolute_dir_with_trailing_backslash() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().to_string_lossy().into_owned();
        let out = validate_open_folder(&real, &[real.clone()]).unwrap();
        assert!(out.to_string_lossy().ends_with('\\'));
    }

    #[test]
    fn open_folder_rejects_file_path() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("calistir.exe");
        fs::write(&f, b"x").unwrap();
        let p = f.to_string_lossy().into_owned();
        assert!(validate_open_folder(&p, &[p.clone()]).is_err());
    }

    #[test]
    fn handlers_report_fake_live_session_and_history() {
        let tmp = tempfile::tempdir().unwrap();
        let claude = tmp.path().join("claude");
        let sessions = claude.join("sessions");
        fs::create_dir_all(&sessions).unwrap();
        fs::create_dir_all(claude.join("projects")).unwrap();
        let cwd = tmp.path().join("proje");
        fs::create_dir_all(&cwd).unwrap();
        let cwd_json = serde_json::to_string(&cwd.to_string_lossy()).unwrap();
        fs::write(
            sessions.join("4242.json"),
            format!(
                r#"{{"pid":4242,"sessionId":"11111111-2222-3333-4444-555555555555","cwd":{cwd_json},"startedAt":1000,"procStart":"777","name":"ornek","status":"busy","updatedAt":2000}}"#
            ),
        )
        .unwrap();
        let probe = FakeProbe(HashMap::from([(4242, 777)]));
        let cache = tmp.path().join("app").join("history-cache.json");
        let state = AppState::new(claude, cache, Box::new(probe));

        let snap = state.get_snapshot();
        assert_eq!(snap.sessions.len(), 1);
        assert_eq!(snap.sessions[0].pid, 4242);
        assert_eq!(snap.sessions[0].parent_pid, Some(1));

        // Detail: valid id works, oversized / malformed ids yield None.
        assert!(state.get_session_detail("11111111-2222-3333-4444-555555555555").is_some());
        assert!(state.get_session_detail(&"a".repeat(65)).is_none());
        assert!(state.get_session_detail("../etc").is_none());

        // History is empty (no transcripts); the live session is not in it.
        let hv = state.get_history();
        assert!(hv.rows.is_empty());
        assert!(!state.index_step(std::time::Duration::from_millis(5)));
        assert!(!state.get_history().indexing);

        // open_folder refuses anything that is not a known history/live cwd.
        assert!(state.open_folder("C:\\Windows").is_err());
    }
}
