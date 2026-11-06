//! IPC commands and the shared application state. Every command is a thin
//! wrapper over a plain function on `AppState` so tests can call the handlers
//! without a webview. All arguments from the UI are untrusted.
use crate::errors;
use station_core::history::{cost_on_rows, HistoryIndex, HistoryRow};
use station_core::locate::{cwd_matches_project_dir, is_session_id, Locator};
use station_core::monitor::{ClosedKey, Monitor, SessionDetail, Snapshot};
use station_core::registry::ProcessProbe;
use station_core::search::{search_transcripts_limited, validate_query, Limits, SearchOutcome};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
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
    /// Today's tokens, counted like `today_cost`.
    pub today_tokens: u64,
    pub today_complete: bool,
    pub indexing: bool,
    /// Cost per day of the live sessions (left out of `rows`), so the cost
    /// charts can count them on every day like `today_cost` does for today.
    pub live: Vec<LiveCost>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveCost {
    pub session_id: String,
    pub cwd: String,
    pub model: Option<String>,
    pub cost_by_day: BTreeMap<chrono::NaiveDate, f64>,
    pub tokens_by_day: BTreeMap<chrono::NaiveDate, u64>,
}

/// Last snapshot plus what history needs from the same refresh: the live ids
/// (= `snap.sessions`) and their cost on `day` and per day.
struct SnapCache {
    at: Instant,
    snap: Snapshot,
    day: chrono::NaiveDate,
    live_cost: f64,
    live_by_day: HashMap<String, BTreeMap<chrono::NaiveDate, f64>>,
    live_tokens_by_day: HashMap<String, BTreeMap<chrono::NaiveDate, u64>>,
}

/// What a resume is bound to: the session's first recorded cwd and the name
/// of the `projects/` folder its transcript is in.
struct ResumeSource {
    first_cwd: String,
    project_dir: String,
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
    /// Generation of the latest content search; a running search whose
    /// generation differs stops at its next line.
    search_gen: AtomicU32,
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
            search_gen: AtomicU32::new(0),
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
        let (snap, live_cost, live_by_day, live_tokens_by_day) = {
            let mut m = lock(&self.monitor);
            let snap = m.refresh(self.probe.as_ref(), SystemTime::now());
            (snap, m.live_cost_on(day).0, m.live_cost_by_day(), m.live_tokens_by_day())
        };
        *lock(&self.snapshot) = Some(SnapCache { at: Instant::now(), snap: snap.clone(), day, live_cost, live_by_day, live_tokens_by_day });
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
        // Live per the latest snapshot: never the closed path (one failed
        // registry read must not turn a running session into a closed one).
        let live_card = lock(&self.snapshot)
            .as_ref()
            .and_then(|c| c.snap.sessions.iter().find(|s| s.session_id == session_id).cloned());
        let closed = {
            let mut m = lock(&self.monitor);
            if let Some(d) = m.live_detail(session_id, self.probe.as_ref(), now) {
                return Some(d);
            }
            if let Some(card) = live_card {
                return Some(m.known_detail(&card));
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
        let (live, day, live_cost, live_rows, live_tokens) = self.fresh_cache(|c| {
            let ids: HashSet<String> = c.snap.sessions.iter().map(|s| s.session_id.clone()).collect();
            let rows: Vec<LiveCost> = c
                .snap
                .sessions
                .iter()
                .filter_map(|s| {
                    let by_day = c.live_by_day.get(&s.session_id)?;
                    Some(LiveCost {
                        session_id: s.session_id.clone(),
                        cwd: s.cwd.clone(),
                        model: s.model.clone(),
                        cost_by_day: by_day.clone(),
                        tokens_by_day: c.live_tokens_by_day.get(&s.session_id).cloned().unwrap_or_default(),
                    })
                })
                .collect();
            let live_tokens: u64 = rows.iter().map(|r| r.tokens_by_day.get(&c.day).copied().unwrap_or(0)).sum();
            (ids, c.day, c.live_cost, rows, live_tokens)
        });
        let rows = lock(&self.history_rows);
        let (closed_cost, today_complete) = cost_on_rows(rows.iter(), day, &live);
        let closed_tokens: u64 = rows
            .iter()
            .filter(|r| !live.contains(&r.session_id))
            .filter_map(|r| r.summary.as_ref())
            .map(|s| s.tokens_by_day.get(&day).copied().unwrap_or(0))
            .sum();
        HistoryView {
            rows: rows.iter().filter(|r| !live.contains(&r.session_id)).cloned().collect(),
            today_cost: closed_cost + live_cost,
            today_tokens: closed_tokens + live_tokens,
            today_complete,
            indexing: self.is_indexing(),
            live: live_rows,
        }
    }

    /// Today's local date and estimated cost across closed and live sessions,
    /// counted like `get_history`'s `today_cost` but without copying the rows.
    pub fn today_cost(&self) -> (chrono::NaiveDate, f64) {
        let (live, day, live_cost) = self.fresh_cache(|c| {
            let ids: HashSet<String> = c.snap.sessions.iter().map(|s| s.session_id.clone()).collect();
            (ids, c.day, c.live_cost)
        });
        let (closed, _) = cost_on_rows(lock(&self.history_rows).iter(), day, &live);
        (day, closed + live_cost)
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

    /// Session id and folder for `claude --resume`. The id must be a lowercase
    /// UUID of a session that is not live; the folder is looked up here (history
    /// rows, then the cached closed detail), never taken from the UI. It is the
    /// session's FIRST recorded cwd, never a later one the session moved to, and
    /// it must be an absolute, local, existing directory whose Claude Code
    /// project-folder name is the folder the transcript sits in. Anything else
    /// is refused with `resume_folder_mismatch[:<folder code>]`.
    pub fn resume_target(&self, session_id: &str) -> Result<(String, PathBuf), String> {
        if !is_session_id(session_id) {
            return Err(errors::SESSION_INVALID.into());
        }
        if self.live_ids().contains(session_id) {
            return Err(errors::SESSION_LIVE.into());
        }
        let src = self.resume_source(session_id).ok_or(errors::SESSION_UNKNOWN)?;
        let dir = validate_open_folder(&src.first_cwd, std::slice::from_ref(&src.first_cwd))
            .map_err(|e| errors::with(errors::RESUME_FOLDER_MISMATCH, e))?;
        if !cwd_matches_project_dir(&src.first_cwd, &src.project_dir) {
            return Err(errors::RESUME_FOLDER_MISMATCH.into());
        }
        Ok((session_id.to_string(), dir))
    }

    /// Pid and name of a live session for "Sekmeye git", both from the
    /// latest snapshot (the UI sends only the id).
    pub fn focus_target(&self, session_id: &str) -> Result<(u32, String), String> {
        if !is_session_id(session_id) {
            return Err(errors::SESSION_INVALID.into());
        }
        self.fresh_cache(|c| {
            c.snap
                .sessions
                .iter()
                .find(|s| s.session_id == session_id && s.pid != 0)
                .map(|s| (s.pid, s.name.clone()))
        })
        .ok_or_else(|| errors::SESSION_NOT_LIVE.into())
    }

    /// First cwd and transcript folder of a closed session. Only an indexed
    /// row carries the transcript's cwd (an unindexed one only has the project
    /// folder name); the cached closed detail is the fallback.
    fn resume_source(&self, session_id: &str) -> Option<ResumeSource> {
        fn folder_of(p: &Path) -> Option<String> {
            p.parent().and_then(Path::file_name).map(|n| n.to_string_lossy().into_owned())
        }
        let from_rows = lock(&self.history_rows).iter().find(|r| r.session_id == session_id).and_then(|r| {
            Some(ResumeSource { first_cwd: r.summary.as_ref()?.first_cwd.clone()?, project_dir: folder_of(&r.path)? })
        });
        from_rows.filter(|s| !s.first_cwd.is_empty()).or_else(|| {
            let cache = lock(&self.closed_cache);
            let (key, d) = cache.as_ref().filter(|(_, d)| d.card.session_id == session_id)?;
            let first_cwd = d.first_cwd.clone().filter(|c| !c.is_empty())?;
            Some(ResumeSource { first_cwd, project_dir: folder_of(&key.0)? })
        })
    }

    /// "İçerikte ara": streams every transcript (live ones too), newest file
    /// first, for user/assistant text containing `query`. The latest call
    /// wins: storing its generation makes any older search still running stop
    /// at its next line and return `cancelled`. Last-writer-wins rather than
    /// "highest wins" so a reloaded UI, whose counter restarts, is not locked
    /// out by a higher number from before the reload.
    pub fn search_history(&self, query: &str, generation: u32) -> Result<SearchOutcome, String> {
        let query = validate_query(query).ok_or(errors::SEARCH_QUERY_INVALID)?;
        self.search_gen.store(generation, Ordering::SeqCst);
        let mut files: Vec<(u64, String, PathBuf)> = Locator::new(self.projects_dir.clone())
            .all_transcripts()
            .into_iter()
            .filter_map(|(p, md)| {
                let id = p.file_stem()?.to_str()?.to_string();
                let modified = md.modified().ok()?.duration_since(SystemTime::UNIX_EPOCH).ok()?.as_millis() as u64;
                Some((modified, id, p))
            })
            .collect();
        files.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
        let files: Vec<(String, PathBuf)> = files.into_iter().map(|(_, id, p)| (id, p)).collect();
        let gen = &self.search_gen;
        Ok(search_transcripts_limited(&files, &query, Limits::DEFAULT, &|| gen.load(Ordering::SeqCst) != generation))
    }

    /// Stops the running content search (History page left or cleared): the
    /// stored generation moves on, so the search sees a mismatch at its next
    /// check and returns `cancelled`. A later search stores its own generation
    /// and runs normally.
    pub fn cancel_search(&self) {
        self.search_gen.fetch_add(1, Ordering::SeqCst);
    }

    pub fn open_folder(&self, path: &str) -> Result<(), String> {
        let mut known: Vec<String> =
            self.get_snapshot().sessions.iter().map(|c| c.cwd.clone()).collect();
        known.extend(lock(&self.history_rows).iter().filter_map(|r| r.cwd.clone()));
        let dir = validate_open_folder(path, &known)?;
        crate::platform::open_folder(&dir).map_err(|e| errors::with(errors::FOLDER_OPEN_FAILED, e))
    }
}

/// Accepts only a path equal to a known session cwd that is an absolute,
/// local, existing directory. explorer.exe would execute a file path, treats
/// `,`/leading `/` as switches, and `is_dir` on a UNC path would make an
/// outbound SMB request. Before the folder itself is touched, its drive must
/// be a local fixed or removable disk (a mapped network drive is refused) and
/// no component of the path may be a symlink or junction (either could point
/// at `\\host\share`). The returned path ends with `\` so explorer always
/// treats it as a folder.
#[cfg(windows)]
pub fn validate_open_folder(path: &str, known_cwds: &[String]) -> Result<PathBuf, String> {
    validate_open_folder_on(path, known_cwds, crate::platform::drive_kind)
}

/// [`validate_open_folder`] with the drive-kind lookup passed in (tests).
#[cfg(windows)]
pub fn validate_open_folder_on(
    path: &str,
    known_cwds: &[String],
    drive_kind: impl Fn(&Path) -> crate::platform::DriveKind,
) -> Result<PathBuf, String> {
    use crate::platform::DriveKind;
    if path.is_empty() || !known_cwds.iter().any(|c| c == path) {
        return Err(errors::FOLDER_UNKNOWN.into());
    }
    if path.starts_with(r"\\") || path.starts_with("//") {
        return Err(errors::FOLDER_NETWORK.into());
    }
    if path.contains(',') || path.starts_with('/') || path.starts_with('-') {
        return Err(errors::FOLDER_INVALID.into());
    }
    let p = Path::new(path);
    if !p.is_absolute() {
        return Err(errors::FOLDER_RELATIVE.into());
    }
    let root = drive_root(path).ok_or(errors::FOLDER_INVALID)?;
    match drive_kind(Path::new(&root)) {
        DriveKind::Local => {}
        DriveKind::Remote => return Err(errors::FOLDER_NETWORK.into()),
        DriveKind::Other => return Err(errors::FOLDER_INVALID.into()),
    }
    // `is_symlink` is true for symlinks and junctions (name surrogates), not
    // for cloud placeholders such as OneDrive folders.
    if p.ancestors().any(|a| std::fs::symlink_metadata(a).is_ok_and(|m| m.file_type().is_symlink())) {
        return Err(errors::FOLDER_INVALID.into());
    }
    if !p.is_dir() {
        return Err(errors::FOLDER_MISSING.into());
    }
    let mut s = p.as_os_str().to_os_string();
    if !path.ends_with('\\') {
        s.push("\\");
    }
    Ok(PathBuf::from(s))
}

/// `X:\` of an absolute, non-UNC Windows path, or None.
#[cfg(windows)]
fn drive_root(path: &str) -> Option<String> {
    let b = path.as_bytes();
    if b.len() >= 3 && b[0].is_ascii_alphabetic() && b[1] == b':' && b[2] == b'\\' {
        return Some(format!("{}\\", &path[..2]));
    }
    None
}

/// Unix: accepts only a path equal to a known session cwd that is an
/// absolute, existing directory (symlinks are followed; `xdg-open` on a
/// file would launch it, so the target must be a directory). `//host/x`
/// style paths and control characters are refused. The path is returned as
/// given: it starts with `/`, so no opener can read it as an option.
#[cfg(unix)]
pub fn validate_open_folder(path: &str, known_cwds: &[String]) -> Result<PathBuf, String> {
    if path.is_empty() || !known_cwds.iter().any(|c| c == path) {
        return Err(errors::FOLDER_UNKNOWN.into());
    }
    if path.starts_with(r"\\") || path.starts_with("//") {
        return Err(errors::FOLDER_NETWORK.into());
    }
    if path.chars().any(char::is_control) {
        return Err(errors::FOLDER_INVALID.into());
    }
    let p = Path::new(path);
    if !p.is_absolute() {
        return Err(errors::FOLDER_RELATIVE.into());
    }
    if !p.is_dir() {
        return Err(errors::FOLDER_MISSING.into());
    }
    Ok(p.to_path_buf())
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
        .map_err(|e| errors::with(errors::TASK_FAILED, e))
}

/// Content search on the blocking pool (it streams every transcript).
/// Both arguments are untrusted: the query is validated (2..=200 chars), the
/// generation is only compared.
#[tauri::command]
pub async fn search_history(app: tauri::AppHandle, query: String, generation: u32) -> Result<SearchOutcome, String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || app.state::<AppState>().search_history(&query, generation))
        .await
        .map_err(|e| errors::with(errors::TASK_FAILED, e))?
}

/// Stops a running content search (no arguments; it only bumps the
/// generation counter).
#[tauri::command]
pub fn cancel_search(state: tauri::State<'_, AppState>) {
    state.cancel_search();
}

#[tauri::command(async)]
pub fn get_history(state: tauri::State<'_, AppState>) -> HistoryView {
    state.get_history()
}

/// "Sekmeye git" for a session in a Windows Terminal tab: brings its window
/// forward and selects the tab (best effort, Win32 + UI Automation on the
/// blocking pool). Only the id comes from the UI. Ok(Some(note code)) when the
/// window was focused but no single tab could be selected.
#[tauri::command]
pub async fn focus_session_tab(app: tauri::AppHandle, session_id: String) -> Result<Option<String>, String> {
    use tauri::Manager;
    let (pid, name) = app.state::<AppState>().focus_target(&session_id)?;
    tauri::async_runtime::spawn_blocking(move || crate::platform::focus_session_tab(pid, &name))
        .await
        .map_err(|e| errors::with(errors::TASK_FAILED, e))?
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
        assert_eq!(validate_open_folder(&real, &["D:\\baska".to_string()]).unwrap_err(), errors::FOLDER_UNKNOWN);
        assert_eq!(validate_open_folder("", &[String::new()]).unwrap_err(), errors::FOLDER_UNKNOWN);
        // Known and existing is accepted.
        assert!(validate_open_folder(&real, std::slice::from_ref(&real)).is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn open_folder_hardening_rejections() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().to_string_lossy().into_owned();
        // Each is listed as "known", so only the hardening rules can reject.
        for (bad, code) in [
            (r"\\sunucu\paylasim".to_string(), errors::FOLDER_NETWORK),
            ("//sunucu/paylasim".to_string(), errors::FOLDER_NETWORK),
            ("/windows".to_string(), errors::FOLDER_INVALID),
            ("-Ac".to_string(), errors::FOLDER_INVALID),
            (r"bagil\yol".to_string(), errors::FOLDER_RELATIVE),
            (format!("{real},/select"), errors::FOLDER_INVALID),
            (r"C:\a,b".to_string(), errors::FOLDER_INVALID),
            (format!(r"{real}\yok"), errors::FOLDER_MISSING),
        ] {
            assert_eq!(validate_open_folder(&bad, std::slice::from_ref(&bad)).unwrap_err(), code, "should reject {bad}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn open_folder_only_on_local_fixed_or_removable_drives() {
        use crate::platform::DriveKind;
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().to_string_lossy().into_owned();
        let known = std::slice::from_ref(&real);
        assert!(validate_open_folder_on(&real, known, |_| DriveKind::Local).is_ok());
        assert_eq!(validate_open_folder_on(&real, known, |_| DriveKind::Remote).unwrap_err(), errors::FOLDER_NETWORK);
        assert_eq!(validate_open_folder_on(&real, known, |_| DriveKind::Other).unwrap_err(), errors::FOLDER_INVALID);
        // The probe is asked about the drive root, before the folder is touched.
        let asked = std::cell::RefCell::new(None);
        let missing = format!(r"{real}\yok");
        let _ = validate_open_folder_on(&missing, std::slice::from_ref(&missing), |r| {
            *asked.borrow_mut() = Some(r.to_path_buf());
            DriveKind::Remote
        });
        assert_eq!(asked.into_inner().unwrap(), PathBuf::from(&real[..3]));
    }

    #[cfg(windows)]
    #[test]
    fn temp_folder_is_on_a_local_drive() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().to_string_lossy().into_owned();
        assert_eq!(crate::platform::drive_kind(Path::new(&real[..3])), crate::platform::DriveKind::Local);
    }

    #[cfg(windows)]
    #[test]
    fn open_folder_rejects_a_junction_anywhere_on_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("hedef");
        fs::create_dir_all(target.join("alt")).unwrap();
        let link = dir.path().join("baglanti");
        let made = std::process::Command::new("cmd.exe")
            .args(["/c", "mklink", "/J"])
            .arg(&link)
            .arg(&target)
            .output()
            .is_ok_and(|o| o.status.success());
        if !made {
            eprintln!("junction could not be created here; skipping");
            return;
        }
        for bad in [link.clone(), link.join("alt")] {
            let p = bad.to_string_lossy().into_owned();
            assert_eq!(validate_open_folder(&p, std::slice::from_ref(&p)).unwrap_err(), errors::FOLDER_INVALID, "{p}");
        }
        let ok = target.join("alt").to_string_lossy().into_owned();
        assert!(validate_open_folder(&ok, std::slice::from_ref(&ok)).is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn open_folder_accepts_exact_known_absolute_dir_with_trailing_backslash() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().to_string_lossy().into_owned();
        let out = validate_open_folder(&real, std::slice::from_ref(&real)).unwrap();
        assert!(out.to_string_lossy().ends_with('\\'));
    }

    #[cfg(unix)]
    #[test]
    fn open_folder_unix_rules() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().to_string_lossy().into_owned();
        // Each is listed as "known", so only the rules themselves can reject.
        for (bad, code) in [
            (r"\\sunucu\paylasim".to_string(), errors::FOLDER_NETWORK),
            ("//sunucu/paylasim".to_string(), errors::FOLDER_NETWORK),
            ("-Ac".to_string(), errors::FOLDER_RELATIVE),
            ("bagil/yol".to_string(), errors::FOLDER_RELATIVE),
            (format!("{real}/sat\nir"), errors::FOLDER_INVALID),
            (format!("{real}/yok"), errors::FOLDER_MISSING),
        ] {
            assert_eq!(validate_open_folder(&bad, std::slice::from_ref(&bad)).unwrap_err(), code, "should reject {bad}");
        }
        // A symlinked folder is fine; the result is the path as given.
        let link = dir.path().join("baglanti");
        std::fs::create_dir(dir.path().join("hedef")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("hedef"), &link).unwrap();
        let l = link.to_string_lossy().into_owned();
        assert_eq!(validate_open_folder(&l, std::slice::from_ref(&l)).unwrap(), link);
        assert_eq!(validate_open_folder(&real, std::slice::from_ref(&real)).unwrap(), dir.path());
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
        // The live session's per-day cost comes along for the charts, with its folder and model.
        assert_eq!(hv.live.len(), 1);
        let live = &hv.live[0];
        assert_eq!((live.session_id.as_str(), live.cwd.as_str()), (LIVE_ID, r"C:\demo"));
        assert_eq!(live.model.as_deref(), Some("claude-sonnet-5-5"));
        let live_total: f64 = live.cost_by_day.values().sum();
        assert!((live_total - one).abs() < 1e-9);
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
    fn live_session_never_served_as_closed_when_registry_read_misses() {
        let tmp = tempfile::tempdir().unwrap();
        let (claude, probe) = two_sessions(tmp.path(), 5);
        let reg = claude.join("sessions").join("4242.json");
        let state = AppState::new(claude, tmp.path().join("c.json"), Box::new(probe));
        assert_eq!(state.get_snapshot().sessions[0].session_id, LIVE_ID);
        // Claude Code is rewriting its registry file at this very moment.
        fs::write(&reg, "").unwrap();
        let d = state.get_session_detail(LIVE_ID).unwrap();
        assert_eq!(d.card.pid, 4242, "still live per the latest snapshot");
        assert_eq!(d.tokens.output, 5, "stats from the live state, not empty");
        assert_eq!(d.card.model.as_deref(), Some("claude-sonnet-5-5"));
        // The registry miss is debounced (process still alive): kept for 2 ticks.
        for _ in 0..2 {
            state.refresh();
            assert_eq!(state.get_session_detail(LIVE_ID).unwrap().card.pid, 4242);
        }
        // Once the miss persists, the closed path takes over.
        state.refresh();
        assert_eq!(state.get_session_detail(LIVE_ID).unwrap().card.pid, 0);
    }

    #[test]
    fn focus_target_takes_pid_and_name_from_the_live_snapshot() {
        let tmp = tempfile::tempdir().unwrap();
        let (claude, probe) = two_sessions(tmp.path(), 5);
        let state = AppState::new(claude, tmp.path().join("c.json"), Box::new(probe));
        let (pid, name) = state.focus_target(LIVE_ID).unwrap();
        assert_eq!(pid, 4242);
        assert_eq!(name, "demo", "card name (cwd folder fallback)");
        assert_eq!(state.focus_target(CLOSED_ID).unwrap_err(), errors::SESSION_NOT_LIVE);
        for bad in ["", "../x", "11111111-2222-4333-8444-55555555555G", "x; calc", "AAAAAAAA-2222-4333-8444-555555555555"] {
            assert_eq!(state.focus_target(bad).unwrap_err(), errors::SESSION_INVALID, "{bad}");
        }
    }

    const OK_ID: &str = "bbbbbbbb-2222-4333-8444-555555555555";
    const UNC_ID: &str = "cccccccc-2222-4333-8444-555555555555";
    const REL_ID: &str = "dddddddd-2222-4333-8444-555555555555";
    const UNKNOWN_ID: &str = "eeeeeeee-2222-4333-8444-555555555555";
    const MULTI_ID: &str = "ffffffff-2222-4333-8444-555555555555";
    const MISMATCH_ID: &str = "abababab-2222-4333-8444-555555555555";

    /// `<dir>/<id>.jsonl` with one user line per cwd, in order.
    fn write_session(dir: &Path, id: &str, cwds: &[&str]) {
        fs::create_dir_all(dir).unwrap();
        let ts = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let body: String = cwds
            .iter()
            .map(|c| {
                let cwd = serde_json::to_string(c).unwrap();
                format!(r#"{{"type":"user","timestamp":"{ts}","cwd":{cwd},"message":{{"content":"x"}}}}"#) + "\n"
            })
            .collect();
        fs::write(dir.join(format!("{id}.jsonl")), body).unwrap();
    }

    /// Live LIVE_ID plus closed sessions:
    /// - OK_ID in the project folder of an existing `work` folder;
    /// - UNC_ID / REL_ID naming a network / relative path;
    /// - MULTI_ID started in `work`, later moved to the existing `work\other`
    ///   (the multi-cwd case from the audit's needs-validation note);
    /// - MISMATCH_ID whose first cwd is `work` but which sits in `C--demo`.
    fn resume_env(tmp: &Path) -> (AppState, PathBuf) {
        let (claude, probe) = two_sessions(tmp, 5);
        let projects = claude.join("projects");
        let demo = projects.join("C--demo");
        let work = tmp.join("is klasoru");
        let other = work.join("other");
        fs::create_dir_all(&other).unwrap();
        let (w, o) = (work.to_string_lossy().into_owned(), other.to_string_lossy().into_owned());
        let home = projects.join(station_core::locate::project_dir_name(&w));
        write_session(&home, OK_ID, &[&w]);
        write_session(&home, MULTI_ID, &[&w, &o, &o]);
        write_session(&demo, UNC_ID, &[r"\\sunucu\pay"]);
        write_session(&demo, REL_ID, &[r"bagil\yol"]);
        write_session(&demo, MISMATCH_ID, &[&w]);
        (AppState::new(claude, tmp.join("c.json"), Box::new(probe)), work)
    }

    fn indexed(state: &AppState) {
        state.rescan_history();
        while state.index_step(Duration::from_secs(5)) {}
    }

    #[test]
    fn resume_rejects_ids_that_are_not_lowercase_uuids() {
        let tmp = tempfile::tempdir().unwrap();
        let (state, _) = resume_env(tmp.path());
        indexed(&state);
        for bad in ["", "../x", "BBBBBBBB-2222-4333-8444-555555555555", "bbbbbbbb-2222-4333-8444-55555555555", "x; calc"] {
            assert_eq!(state.resume_target(bad).unwrap_err(), errors::SESSION_INVALID, "{bad}");
        }
    }

    #[test]
    fn resume_rejects_unknown_live_unc_and_relative() {
        let tmp = tempfile::tempdir().unwrap();
        let (state, _) = resume_env(tmp.path());
        indexed(&state);
        assert_eq!(state.resume_target(UNKNOWN_ID).unwrap_err(), errors::SESSION_UNKNOWN);
        assert_eq!(state.resume_target(LIVE_ID).unwrap_err(), errors::SESSION_LIVE);
        assert_eq!(state.resume_target(UNC_ID).unwrap_err(), "resume_folder_mismatch:folder_network");
        assert_eq!(state.resume_target(REL_ID).unwrap_err(), "resume_folder_mismatch:folder_relative");
    }

    fn same_dir(dir: &Path, want: &Path) {
        assert_eq!(dir.to_string_lossy().trim_end_matches('\\'), want.to_string_lossy());
    }

    #[test]
    fn resume_uses_the_first_cwd_when_the_session_moved_later() {
        let tmp = tempfile::tempdir().unwrap();
        let (state, work) = resume_env(tmp.path());
        indexed(&state);
        let (_, dir) = state.resume_target(MULTI_ID).unwrap();
        same_dir(&dir, &work);
    }

    #[test]
    fn resume_refuses_a_first_cwd_outside_the_project_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let (state, _) = resume_env(tmp.path());
        indexed(&state);
        assert_eq!(state.resume_target(MISMATCH_ID).unwrap_err(), errors::RESUME_FOLDER_MISMATCH);
    }

    #[test]
    fn resume_binding_also_applies_to_the_closed_detail_path() {
        let tmp = tempfile::tempdir().unwrap();
        let (state, work) = resume_env(tmp.path());
        // Not indexed: the cwd comes from the cached closed detail.
        state.get_session_detail(MULTI_ID).unwrap();
        let (_, dir) = state.resume_target(MULTI_ID).unwrap();
        same_dir(&dir, &work);
        state.get_session_detail(MISMATCH_ID).unwrap();
        assert_eq!(state.resume_target(MISMATCH_ID).unwrap_err(), errors::RESUME_FOLDER_MISMATCH);
    }

    #[test]
    fn resume_binding_applies_to_a_tampered_history_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let (state, work) = resume_env(tmp.path());
        indexed(&state);
        assert!(state.save_history_if_dirty().unwrap());
        let cache = tmp.path().join("c.json");
        // Someone rewrites the cached first cwd to another existing folder.
        let other = work.join("other").to_string_lossy().into_owned();
        let mut v: serde_json::Value = serde_json::from_str(&fs::read_to_string(&cache).unwrap()).unwrap();
        for e in v["entries"].as_array_mut().unwrap() {
            if e["path"].as_str().unwrap().contains(OK_ID) {
                e["summary"]["firstCwd"] = other.clone().into();
                e["summary"]["cwd"] = other.clone().into();
            }
        }
        fs::write(&cache, v.to_string()).unwrap();
        let (claude, probe) = (tmp.path().join("claude"), FakeProbe(HashMap::from([(4242, 777)])));
        let reloaded = AppState::new(claude, cache, Box::new(probe));
        assert_eq!(reloaded.get_history().rows.iter().find(|r| r.session_id == OK_ID).unwrap().cwd.as_deref(), Some(&*other));
        assert_eq!(reloaded.resume_target(OK_ID).unwrap_err(), errors::RESUME_FOLDER_MISMATCH);
    }

    #[test]
    fn resume_cwd_comes_from_history_row() {
        let tmp = tempfile::tempdir().unwrap();
        let (state, work) = resume_env(tmp.path());
        indexed(&state);
        let (id, dir) = state.resume_target(OK_ID).unwrap();
        assert_eq!(id, OK_ID);
        assert_eq!(dir.to_string_lossy().trim_end_matches('\\'), work.to_string_lossy());
    }

    #[test]
    fn resume_cwd_comes_from_closed_detail_before_indexing() {
        let tmp = tempfile::tempdir().unwrap();
        let (state, work) = resume_env(tmp.path());
        assert!(state.resume_target(OK_ID).is_err(), "nothing known yet");
        assert_eq!(state.get_session_detail(OK_ID).unwrap().card.pid, 0);
        assert_eq!(state.get_session_detail(OK_ID).unwrap().card.pid, 0);
        let (_, dir) = state.resume_target(OK_ID).unwrap();
        assert_eq!(dir.to_string_lossy().trim_end_matches('\\'), work.to_string_lossy());
    }

    #[test]
    fn resume_happy_path_spawns_in_the_session_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let (state, work) = resume_env(tmp.path());
        indexed(&state);
        let (id, dir) = state.resume_target(OK_ID).unwrap();
        // Same code path as pty_spawn_resume, with a stub program instead of powershell/claude.
        let ptys = PtyManager::new();
        let (tx, rx) = std::sync::mpsc::channel::<Option<Vec<u8>>>();
        let tx2 = tx.clone();
        #[cfg(windows)]
        let args = crate::pty::stub::run(&format!("cd & echo {id}"));
        #[cfg(unix)]
        let args = crate::pty::stub::run(&format!("pwd; echo {id}"));
        let info = ptys
            .spawn_in(std::ffi::OsStr::new(crate::pty::stub::IDLE), &args, &dir, 500, 24, move |b| tx.send(Some(b.to_vec())).is_ok(), move |_| {
                let _ = tx2.send(None);
            })
            .unwrap();
        let mut text = String::new();
        while let Some(b) = rx.recv_timeout(Duration::from_secs(20)).expect("timed out") {
            text.push_str(&String::from_utf8_lossy(&b));
            if text.contains("\x1b[6n") {
                text = text.replace("\x1b[6n", "");
                let _ = ptys.write(info.id, "\x1b[1;1R");
            }
        }
        let flat: String = text.chars().filter(|c| *c != '\r' && *c != '\n').collect();
        assert!(flat.contains(&*work.to_string_lossy()), "cwd not used: {text:?}");
        assert!(flat.contains(OK_ID), "{text:?}");
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

    fn user_line(text: &str) -> String {
        serde_json::json!({"type":"user","timestamp":"2026-11-02T10:00:00Z","message":{"content":text}}).to_string()
    }

    #[test]
    fn search_history_validates_the_query_and_finds_live_and_closed_text() {
        let tmp = tempfile::tempdir().unwrap();
        let (claude, probe) = two_sessions(tmp.path(), 5);
        let proj = claude.join("projects").join("C--demo");
        fs::write(proj.join(format!("{CLOSED_ID}.jsonl")), user_line("Kalibrasyon ADC") + "\n").unwrap();
        fs::write(proj.join(format!("{LIVE_ID}.jsonl")), user_line("ADC KALİBRASYONU") + "\n").unwrap();
        let state = AppState::new(claude, tmp.path().join("c.json"), Box::new(probe));
        assert_eq!(state.search_history("a", 1), Err(errors::SEARCH_QUERY_INVALID.into()));
        assert_eq!(state.search_history(&"x".repeat(201), 1), Err(errors::SEARCH_QUERY_INVALID.into()));
        let out = state.search_history("  kalibrasyon ", 2).unwrap();
        let mut ids: Vec<&str> = out.hits.iter().map(|h| h.session_id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, [LIVE_ID, CLOSED_ID]);
        assert!(!out.cancelled);
    }

    #[test]
    fn a_newer_search_generation_cancels_the_running_one() {
        let tmp = tempfile::tempdir().unwrap();
        let (claude, probe) = two_sessions(tmp.path(), 5);
        let proj = claude.join("projects").join("C--demo");
        // A large closed transcript so the first search is still running.
        let body: String = (0..200_000).map(|i| user_line(&format!("satır {i}")) + "\n").collect();
        fs::write(proj.join(format!("{CLOSED_ID}.jsonl")), body).unwrap();
        let state = std::sync::Arc::new(AppState::new(claude, tmp.path().join("c.json"), Box::new(probe)));
        let s1 = state.clone();
        let first = std::thread::spawn(move || s1.search_history("bulunmayan metin", 1).unwrap());
        // Wait until the first search has registered its generation.
        while state.search_gen.load(Ordering::SeqCst) != 1 {
            std::thread::yield_now();
        }
        let second = state.search_history("satır 5", 2).unwrap();
        assert!(!second.cancelled);
        assert!(!second.hits.is_empty());
        assert!(first.join().unwrap().cancelled, "the older search stops");
        // A stale generation arriving later still runs (last writer wins).
        assert!(!state.search_history("satır 7", 1).unwrap().cancelled);
    }

    #[test]
    fn cancel_search_stops_the_running_search_and_later_searches_still_run() {
        let tmp = tempfile::tempdir().unwrap();
        let (claude, probe) = two_sessions(tmp.path(), 5);
        let proj = claude.join("projects").join("C--demo");
        let body: String = (0..200_000).map(|i| user_line(&format!("satır {i}")) + "\n").collect();
        fs::write(proj.join(format!("{CLOSED_ID}.jsonl")), body).unwrap();
        let state = std::sync::Arc::new(AppState::new(claude, tmp.path().join("c.json"), Box::new(probe)));
        let s1 = state.clone();
        let first = std::thread::spawn(move || s1.search_history("bulunmayan metin", 7).unwrap());
        while state.search_gen.load(Ordering::SeqCst) != 7 {
            std::thread::yield_now();
        }
        state.cancel_search();
        assert!(first.join().unwrap().cancelled, "the search stops after cancel_search");
        // The same UI generation counter keeps working afterwards.
        let next = state.search_history("satır 5", 8).unwrap();
        assert!(!next.cancelled && !next.hits.is_empty());
    }
}

// ---- terminals (ConPTY) ----------------------------------------------------
// The UI cannot choose a program or arguments: `pty_spawn` always starts
// the platform shell (see `shell.rs`). Unknown ids come back as Err.

use crate::pty::{PtyInfo, PtyManager};
use tauri::ipc::Channel;

/// The launch of the shell chosen in the settings, resuming `resume` if set.
fn chosen_launch(settings: &SettingsStore, resume: Option<&str>) -> Result<crate::integration::Launch, String> {
    let s = settings.get();
    let shell = crate::shell::for_setting(&s.shell)?;
    let startup = crate::integration::Startup {
        resume: resume.map(str::to_string),
        ..crate::butaglyph::startup(&s.prompt, &s.prompt_preset)
    };
    crate::integration::launch(&shell, &startup)
}

#[tauri::command(async)]
pub fn pty_spawn(
    settings: tauri::State<'_, SettingsStore>,
    state: tauri::State<'_, PtyManager>,
    cols: u16,
    rows: u16,
    on_output: Channel<Vec<u8>>,
    on_exit: Channel<i32>,
) -> Result<PtyInfo, String> {
    state.spawn_shell(
        &chosen_launch(&settings, None)?,
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

/// New terminal resuming a closed session: the UI sends only the session id;
/// folder and command line are derived here (see `AppState::resume_target`).
/// A session still held by the terminal that resumed it is refused with
/// `session_resume_terminal_open:<terminal>` before the generic live check.
#[tauri::command(async)]
pub fn pty_spawn_resume(
    settings: tauri::State<'_, SettingsStore>,
    app: tauri::State<'_, AppState>,
    state: tauri::State<'_, PtyManager>,
    session_id: String,
    cols: u16,
    rows: u16,
    on_output: Channel<Vec<u8>>,
    on_exit: Channel<i32>,
) -> Result<PtyInfo, String> {
    state.check_resume_terminal(&session_id)?;
    let (id, cwd) = app.resume_target(&session_id)?;
    state.spawn_resume(
        &chosen_launch(&settings, Some(&id))?,
        &cwd,
        &id,
        cols,
        rows,
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

/// Shell ids (settings `shell`) installed on this machine, `auto` first.
#[tauri::command(async)]
pub fn list_shells() -> Vec<String> {
    crate::shell::installed()
}

/// Whether butaglyph is installed, its version and its presets (runs
/// `buta`, so on the blocking pool).
#[tauri::command(async)]
pub fn butaglyph_info() -> crate::butaglyph::Info {
    crate::butaglyph::info()
}

/// Two sample prompts (ANSI) drawn by buta with `preset` ("" = the user's
/// own config). The name is validated before anything runs.
#[tauri::command(async)]
pub fn butaglyph_preview(preset: String) -> Result<Vec<String>, String> {
    crate::butaglyph::preview(&preset)
}

/// Opens butaglyph's own config editor (`buta config`) in the browser.
#[tauri::command(async)]
pub fn open_butaglyph_editor() -> Result<(), String> {
    crate::butaglyph::open_editor()
}

/// Opens a terminal outside the app in the home folder. Takes nothing from
/// the UI: program and arguments are fixed. Windows: a new PowerShell tab in
/// Windows Terminal (see `wt.rs`); Unix: the user's terminal emulator.
#[tauri::command(async)]
pub fn open_system_terminal() -> Result<(), String> {
    #[cfg(windows)]
    return crate::wt::open();
    #[cfg(unix)]
    return crate::platform::open_external_terminal();
}

#[cfg(test)]
mod wt_command_tests {
    #[test]
    fn open_system_terminal_takes_no_arguments() {
        // Compile-time check: any parameter would change the fn type.
        let _f: fn() -> Result<(), String> = super::open_system_terminal;
    }
}

// ---- settings, notifications, tray ------------------------------------------

use crate::notifier::Notifier;
use crate::settings::{Settings, SettingsStore};

#[tauri::command(async)]
pub fn get_settings(store: tauri::State<'_, SettingsStore>) -> Settings {
    store.get()
}

/// Validated (unknown language refused), unknown keys ignored, missing keys
/// defaulted; `trayNoticeShown` from the UI is ignored. Returns what is stored.
#[tauri::command(async)]
pub fn set_settings(store: tauri::State<'_, SettingsStore>, settings: Settings) -> Result<Settings, String> {
    store.set_from_ui(settings)
}

/// The sessions the UI shows (the detail view's session, or the Claude
/// sessions in the visible terminal panes; at most two, empty for any other
/// view); their notifications are held back while the window is focused,
/// and the ones already shown are closed. Each id is validated; more than
/// two are refused.
#[tauri::command(async)]
pub fn set_focus_context(
    app: tauri::AppHandle,
    notifier: tauri::State<'_, Notifier>,
    session_ids: Vec<String>,
) -> Result<(), String> {
    notifier.set_focus(session_ids)?;
    crate::notifier::withdraw_shown(&app);
    Ok(())
}

/// Window close with "minimize to tray" on: hides the window (terminals and
/// monitoring keep running). Takes nothing from the UI.
#[tauri::command(async)]
pub fn hide_to_tray(app: tauri::AppHandle) -> Result<(), String> {
    crate::notifier::hide_to_tray(&app)
}

fn app_data(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    use tauri::Manager;
    app.path().app_data_dir().map_err(|e| e.to_string())
}

/// Status line data for the per-terminal Claude bars.
#[tauri::command]
pub async fn claude_status(app: tauri::AppHandle) -> Result<crate::statusline::ClaudeStatus, String> {
    let data = app_data(&app)?;
    tauri::async_runtime::spawn_blocking(move || crate::statusline::status_now(&data)).await.map_err(|e| e.to_string())
}

/// "Show Claude usage limits": the status line bridge on or off.
#[tauri::command]
pub async fn set_usage_bridge(app: tauri::AppHandle, on: bool) -> Result<bool, String> {
    let data = app_data(&app)?;
    tauri::async_runtime::spawn_blocking(move || crate::statusline::set_bridge(&data, on))
        .await
        .map_err(|e| e.to_string())?
}

/// Whether the session folder has modified tracked files (`None`: unknown).
#[tauri::command]
pub async fn git_dirty(cwd: String) -> Option<bool> {
    tauri::async_runtime::spawn_blocking(move || crate::gitstat::dirty(&cwd)).await.ok().flatten()
}
