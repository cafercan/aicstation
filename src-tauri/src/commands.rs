//! IPC commands and the shared application state. Every command is a thin
//! wrapper over a plain function on `AppState` so tests can call the handlers
//! without a webview. All arguments from the UI are untrusted.
use pano_core::history::{cost_on_rows, HistoryIndex, HistoryRow};
use pano_core::locate::Locator;
use pano_core::monitor::{ClosedKey, Monitor, SessionDetail, Snapshot};
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

/// Last snapshot plus what history needs from the same refresh: the live ids
/// (= `snap.sessions`) and their cost on `day`.
struct SnapCache {
    at: Instant,
    snap: Snapshot,
    day: chrono::NaiveDate,
    live_cost: f64,
}

pub struct AppState {
    monitor: Mutex<Monitor>,
    history: Mutex<HistoryIndex>,
    /// Rows published by the indexer thread after each step so commands never
    /// wait on `HistoryIndex::step`.
    history_rows: Mutex<Vec<HistoryRow>>,
    projects_dir: PathBuf,
    probe: Box<dyn ProcessProbe + Send + Sync>,
    snapshot: Mutex<Option<SnapCache>>,
    indexing: AtomicBool,
    /// Set after repeated indexer panics; indexing stays off for this run.
    index_disabled: AtomicBool,
    /// Index changed since the last save.
    history_dirty: AtomicBool,
    /// Last built closed-session detail, keyed by transcript (path, len, mtime).
    closed_cache: Mutex<Option<(ClosedKey, SessionDetail)>>,
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
        let projects_dir = claude_dir.join("projects");
        let history = HistoryIndex::load(cache_path, &Locator::new(projects_dir.clone()));
        let state = Self {
            monitor: Mutex::new(Monitor::new(claude_dir)),
            history: Mutex::new(history),
            history_rows: Mutex::new(Vec::new()),
            projects_dir,
            probe,
            snapshot: Mutex::new(None),
            indexing: AtomicBool::new(true),
            index_disabled: AtomicBool::new(false),
            history_dirty: AtomicBool::new(false),
            closed_cache: Mutex::new(None),
        };
        state.publish_history();
        state
    }

    /// Copies the index rows into the published list (brief history lock).
    pub fn publish_history(&self) {
        let rows: Vec<HistoryRow> = lock(&self.history).rows(&HashSet::new()).into_iter().cloned().collect();
        *lock(&self.history_rows) = rows;
    }

    /// Recomputes the snapshot and stores it. The monitor lock is released
    /// before the snapshot lock is taken, and before the caller emits.
    pub fn refresh(&self) -> Snapshot {
        let day = chrono::Local::now().date_naive();
        let (snap, live_cost) = {
            let mut m = lock(&self.monitor);
            let snap = m.refresh(self.probe.as_ref(), SystemTime::now());
            (snap, m.live_cost_on(day).0)
        };
        *lock(&self.snapshot) = Some(SnapCache { at: Instant::now(), snap: snap.clone(), day, live_cost });
        snap
    }

    fn fresh_cache<R>(&self, f: impl Fn(&SnapCache) -> R) -> R {
        if let Some(r) = lock(&self.snapshot).as_ref().filter(|c| c.at.elapsed() < SNAPSHOT_MAX_AGE).map(&f) {
            return r;
        }
        self.refresh();
        f(lock(&self.snapshot).as_ref().expect("refresh stores a snapshot"))
    }

    pub fn get_snapshot(&self) -> Snapshot {
        self.fresh_cache(|c| c.snap.clone())
    }

    fn live_ids(&self) -> HashSet<String> {
        self.fresh_cache(|c| c.snap.sessions.iter().map(|s| s.session_id.clone()).collect())
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
        // Transcript streaming happens without the monitor lock; an unchanged
        // closed transcript is served from the cache.
        let key = closed.key();
        if let Some(k) = &key {
            if let Some((ck, d)) = lock(&self.closed_cache).as_ref() {
                if ck == k {
                    return Some(d.clone());
                }
            }
        }
        let d = closed.run(session_id, now)?;
        if let Some(k) = key {
            *lock(&self.closed_cache) = Some((k, d.clone()));
        }
        Some(d)
    }

    /// Rows exclude live sessions. Today's cost = indexed cost of the closed
    /// sessions + today's slice of each live session (and its agents) from the
    /// monitor; both use the live set of the same refresh, so nothing is counted
    /// twice. `today_complete` covers the closed part only.
    pub fn get_history(&self) -> HistoryView {
        let (live, day, live_cost) = self.fresh_cache(|c| {
            let ids: HashSet<String> = c.snap.sessions.iter().map(|s| s.session_id.clone()).collect();
            (ids, c.day, c.live_cost)
        });
        let rows = lock(&self.history_rows);
        let (closed_cost, today_complete) = cost_on_rows(rows.iter(), day, &live);
        HistoryView {
            rows: rows.iter().filter(|r| !live.contains(&r.session_id)).cloned().collect(),
            today_cost: closed_cost + live_cost,
            today_complete,
            indexing: self.is_indexing(),
        }
    }

    /// Whether history indexing still has work (false once everything is indexed).
    pub fn is_indexing(&self) -> bool {
        self.indexing.load(Ordering::Relaxed)
    }

    /// Gives up indexing for this run (after repeated panics) and publishes what exists.
    pub fn disable_indexing(&self) {
        self.index_disabled.store(true, Ordering::Relaxed);
        self.indexing.store(false, Ordering::Relaxed);
        self.publish_history();
    }

    /// Re-lists transcripts (new / changed closed sessions; live ones skipped)
    /// and publishes. Returns whether indexing has work.
    pub fn rescan_history(&self) -> bool {
        if self.index_disabled.load(Ordering::Relaxed) {
            return false;
        }
        let live = self.live_ids();
        let more = lock(&self.history).rescan(&Locator::new(self.projects_dir.clone()), &live);
        self.publish_history();
        self.indexing.store(more, Ordering::Relaxed);
        more
    }

    /// Runs one indexing slice, then publishes the result. Only the indexer
    /// thread calls this; commands read the published rows and never wait on it.
    pub fn index_step(&self, budget: Duration) -> bool {
        if self.index_disabled.load(Ordering::Relaxed) {
            return false;
        }
        let more = {
            let mut h = lock(&self.history);
            let had_work = h.has_work();
            let more = h.step(budget);
            if had_work {
                self.history_dirty.store(true, Ordering::Relaxed);
            }
            more
        };
        self.publish_history();
        self.indexing.store(more, Ordering::Relaxed);
        more
    }

    /// Saves the cache only when the index changed since the last save.
    /// Returns Ok(true) when something was written.
    pub fn save_history_if_dirty(&self) -> std::io::Result<bool> {
        if !self.history_dirty.swap(false, Ordering::Relaxed) {
            return Ok(false);
        }
        lock(&self.history).save().inspect_err(|_| self.history_dirty.store(true, Ordering::Relaxed))?;
        Ok(true)
    }

    /// Exit-time save that never waits on an indexing step in progress.
    pub fn save_history_on_exit(&self) {
        if !self.history_dirty.load(Ordering::Relaxed) {
            return;
        }
        if let Ok(h) = self.history.try_lock() {
            if let Err(e) = h.save() {
                eprintln!("geçmiş önbelleği kaydedilemedi: {e}");
            }
        }
    }

    pub fn open_folder(&self, path: &str) -> Result<(), String> {
        let mut known: Vec<String> =
            self.get_snapshot().sessions.iter().map(|c| c.cwd.clone()).collect();
        known.extend(lock(&self.history_rows).iter().filter_map(|r| r.cwd.clone()));
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

/// Runs on the blocking pool: a closed session's transcript may be hundreds
/// of MB and must not occupy an async worker (pty_write shares them).
#[tauri::command]
pub async fn get_session_detail(
    app: tauri::AppHandle,
    session_id: String,
) -> Result<Option<SessionDetail>, String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || app.state::<AppState>().get_session_detail(&session_id))
        .await
        .map_err(|e| e.to_string())
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
        assert!(validate_open_folder(&real, std::slice::from_ref(&real)).is_ok());
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
            assert!(validate_open_folder(&bad, std::slice::from_ref(&bad)).is_err(), "should reject {bad}");
        }
    }

    #[test]
    fn open_folder_accepts_exact_known_absolute_dir_with_trailing_backslash() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().to_string_lossy().into_owned();
        let out = validate_open_folder(&real, std::slice::from_ref(&real)).unwrap();
        assert!(out.to_string_lossy().ends_with('\\'));
    }

    #[test]
    fn open_folder_rejects_file_path() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("calistir.exe");
        fs::write(&f, b"x").unwrap();
        let p = f.to_string_lossy().into_owned();
        assert!(validate_open_folder(&p, std::slice::from_ref(&p)).is_err());
    }

    const LIVE_ID: &str = "11111111-2222-4333-8444-555555555555";
    const CLOSED_ID: &str = "aaaaaaaa-2222-4333-8444-555555555555";

    fn asst(mid: &str, out: u64, ts: &str) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"{ts}","cwd":"C:\\demo","message":{{"id":"{mid}","model":"claude-sonnet-5-5","usage":{{"input_tokens":0,"output_tokens":{out}}},"content":[]}}}}"#
        )
    }

    /// claude dir with one live session (pid 4242) and one closed session,
    /// each with one assistant line of `out` output tokens today.
    fn two_sessions(tmp: &Path, out: u64) -> (PathBuf, FakeProbe) {
        let claude = tmp.join("claude");
        let sessions = claude.join("sessions");
        let proj = claude.join("projects").join("C--demo");
        fs::create_dir_all(&sessions).unwrap();
        fs::create_dir_all(&proj).unwrap();
        fs::write(
            sessions.join("4242.json"),
            format!(r#"{{"pid":4242,"sessionId":"{LIVE_ID}","cwd":"C:\\demo","procStart":"777","status":"busy"}}"#),
        )
        .unwrap();
        let ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        fs::write(proj.join(format!("{LIVE_ID}.jsonl")), asst("m1", out, &ts) + "\n").unwrap();
        fs::write(proj.join(format!("{CLOSED_ID}.jsonl")), asst("m2", out, &ts) + "\n").unwrap();
        (claude, FakeProbe(HashMap::from([(4242, 777)])))
    }

    #[test]
    fn today_cost_combines_closed_index_and_live_monitor_without_double_counting() {
        let tmp = tempfile::tempdir().unwrap();
        let (claude, probe) = two_sessions(tmp.path(), 1_000_000);
        let state = AppState::new(claude, tmp.path().join("c.json"), Box::new(probe));
        state.rescan_history();
        while state.index_step(Duration::from_secs(5)) {}
        let hv = state.get_history();
        assert!(hv.today_complete);
        assert_eq!(hv.rows.len(), 1, "live session is not a history row");
        let one = hv.rows[0].summary.as_ref().unwrap().cost_usd;
        assert!(one > 0.0);
        assert!((hv.today_cost - 2.0 * one).abs() < 1e-9, "{} vs 2 x {one}", hv.today_cost);
    }

    #[test]
    fn closed_detail_served_from_cache_until_transcript_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let (claude, probe) = two_sessions(tmp.path(), 5);
        let path = claude.join("projects").join("C--demo").join(format!("{CLOSED_ID}.jsonl"));
        let state = AppState::new(claude, tmp.path().join("c.json"), Box::new(probe));
        assert_eq!(state.get_session_detail(CLOSED_ID).unwrap().tokens.output, 5);
        // Same length and mtime, different content: a cache hit returns the old detail.
        let mtime = fs::metadata(&path).unwrap().modified().unwrap();
        let text = fs::read_to_string(&path).unwrap().replace("\"output_tokens\":5", "\"output_tokens\":7");
        fs::write(&path, text).unwrap();
        fs::OpenOptions::new().write(true).open(&path).unwrap().set_modified(mtime).unwrap();
        assert_eq!(state.get_session_detail(CLOSED_ID).unwrap().tokens.output, 5, "cache hit");
        // Grown file: rebuilt.
        let ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let mut f = fs::OpenOptions::new().append(true).open(&path).unwrap();
        std::io::Write::write_all(&mut f, (asst("m3", 1, &ts) + "\n").as_bytes()).unwrap();
        drop(f);
        assert_eq!(state.get_session_detail(CLOSED_ID).unwrap().tokens.output, 8);
    }

    #[test]
    fn history_saved_only_when_dirty() {
        let tmp = tempfile::tempdir().unwrap();
        let (claude, probe) = two_sessions(tmp.path(), 5);
        let cache = tmp.path().join("c.json");
        let state = AppState::new(claude, cache.clone(), Box::new(probe));
        assert!(!state.save_history_if_dirty().unwrap(), "nothing indexed yet");
        assert!(!cache.exists());
        state.rescan_history();
        while state.index_step(Duration::from_secs(5)) {}
        assert!(state.save_history_if_dirty().unwrap());
        assert!(cache.exists());
        assert!(!state.save_history_if_dirty().unwrap(), "unchanged -> no write");
        state.index_step(Duration::from_secs(5)); // no work left
        assert!(!state.save_history_if_dirty().unwrap());
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

// ---- terminals (ConPTY) ----------------------------------------------------
// The UI cannot choose a program or arguments: `pty_spawn` always starts
// powershell.exe. Unknown ids come back as Err.

use crate::pty::{PtyInfo, PtyManager};
use tauri::ipc::Channel;

#[tauri::command(async)]
pub fn pty_spawn(
    state: tauri::State<'_, PtyManager>,
    cols: u16,
    rows: u16,
    on_output: Channel<Vec<u8>>,
    on_exit: Channel<i32>,
) -> Result<PtyInfo, String> {
    state.spawn_shell(
        cols,
        rows,
        // A failed send means the webview side is gone (e.g. reload); the
        // manager then kills the orphaned terminal.
        move |b| on_output.send(b.to_vec()).is_ok(),
        move |code| {
            let _ = on_exit.send(code);
        },
    )
}

#[tauri::command(async)]
pub fn pty_write(state: tauri::State<'_, PtyManager>, id: u32, data: String) -> Result<(), String> {
    state.write(id, &data)
}

#[tauri::command(async)]
pub fn pty_resize(
    state: tauri::State<'_, PtyManager>,
    id: u32,
    cols: u16,
    rows: u16,
) -> Result<(), String> {
    state.resize(id, cols, rows)
}

#[tauri::command(async)]
pub fn pty_kill(state: tauri::State<'_, PtyManager>, id: u32) -> Result<(), String> {
    state.kill(id)
}

#[tauri::command(async)]
pub fn pty_list(state: tauri::State<'_, PtyManager>) -> Vec<PtyInfo> {
    state.list()
}
