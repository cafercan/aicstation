//! Background history indexer: per-transcript summaries with an on-disk cache.
use crate::agents::{build_tree, AgentKind, AgentNode};
use crate::locate::Locator;
use crate::model::Usage;
use crate::stats::SessionStats;
use crate::transcript::{ReadEvent, TranscriptReader};
use chrono::{Local, NaiveDate, NaiveTime};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistorySummary {
    pub name: String,
    /// Last recorded cwd (display, folder filter).
    pub cwd: Option<String>,
    /// First recorded cwd: the project folder a resume is bound to.
    #[serde(default)]
    pub first_cwd: Option<String>,
    pub model: Option<String>,
    pub first_at_ms: u64,
    pub last_at_ms: u64,
    pub agents_total: u32,
    pub cost_usd: f64,
    pub cost_by_day: BTreeMap<NaiveDate, f64>,
    /// Main transcript plus agents; the exact numbers (cost is an estimate).
    pub tokens: Usage,
    pub tokens_by_day: BTreeMap<NaiveDate, u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryRow {
    pub session_id: String,
    /// Not sent to the UI (unused there; a non-UTF-8 path would fail serialization).
    #[serde(skip)]
    pub path: PathBuf,
    pub modified_ms: u64,
    pub size: u64,
    pub name: String,
    pub cwd: Option<String>,
    /// `None` until indexed (the UI shows "?").
    pub summary: Option<HistorySummary>,
}

/// On-disk cache schema. A cache of any other version (or the bare array of
/// version 1, which had no first cwd) is ignored and rebuilt (version 3 added the token totals).
pub const CACHE_VERSION: u32 = 3;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CacheEntry {
    path: PathBuf,
    size: u64,
    modified_ms: u64,
    summary: HistorySummary,
}

pub struct HistoryIndex {
    cache_path: PathBuf,
    rows: Vec<HistoryRow>,
    cache: HashMap<PathBuf, (u64, u64, HistorySummary)>,
    /// Files that could not be read; skipped so `step` cannot spin on them.
    failed: HashSet<PathBuf>,
    /// Sessions live at the last `rescan`; `step` leaves them alone (a growing
    /// transcript is indexed once, after it closes).
    live: HashSet<String>,
}

fn ms(t: SystemTime) -> u64 {
    t.duration_since(SystemTime::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

fn new_row(path: &Path, md: &fs::Metadata) -> Option<HistoryRow> {
    let stem = path.file_stem().and_then(|s| s.to_str())?;
    Some(HistoryRow {
        session_id: stem.to_string(),
        modified_ms: md.modified().map_or(0, ms),
        size: md.len(),
        name: stem.to_string(),
        cwd: path.parent().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()),
        summary: None,
        path: path.to_path_buf(),
    })
}

#[derive(Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    entries: Vec<CacheEntry>,
}

fn load_cache(path: &Path) -> HashMap<PathBuf, (u64, u64, HistorySummary)> {
    let Ok(text) = fs::read_to_string(path) else { return HashMap::new() };
    let Ok(file) = serde_json::from_str::<CacheFile>(&text) else { return HashMap::new() };
    if file.version != CACHE_VERSION {
        return HashMap::new();
    }
    file.entries.into_iter().map(|e| (e.path, (e.size, e.modified_ms, e.summary))).collect()
}

/// Last component of a Windows or POSIX path.
fn last_component(p: &str) -> Option<&str> {
    p.rsplit(['/', '\\']).find(|s| !s.is_empty())
}

/// Running totals of a summary being built.
struct Totals {
    cost: f64,
    cost_by_day: BTreeMap<NaiveDate, f64>,
    tokens: Usage,
    tokens_by_day: BTreeMap<NaiveDate, u64>,
}

/// Add cost and tokens of every `Agent` node (recursing through workflow/phase groups).
fn add_agent_totals(nodes: &[AgentNode], t: &mut Totals) {
    for n in nodes {
        if n.kind == AgentKind::Agent {
            t.cost += n.stats.cost_usd;
            for (d, c) in &n.stats.cost_by_day {
                *t.cost_by_day.entry(*d).or_insert(0.0) += c;
            }
            t.tokens.add(&n.stats.tokens);
            for (d, k) in &n.stats.tokens_by_day {
                let e = t.tokens_by_day.entry(*d).or_insert(0);
                *e = e.saturating_add(*k);
            }
        }
        add_agent_totals(&n.children, t);
    }
}

/// Stream one transcript line by line; never holds the whole file.
fn summarize(path: &Path, locator: &mut Locator, id: &str) -> io::Result<HistorySummary> {
    let mut stats = SessionStats::default();
    TranscriptReader::new(path.to_path_buf()).read_each(|ev| {
        if let ReadEvent::Entry(e) = ev {
            stats.apply(&e);
        }
    })?;
    let mut totals = Totals {
        cost: stats.cost_usd,
        cost_by_day: stats.cost_by_day.clone(),
        tokens: stats.tokens,
        tokens_by_day: stats.tokens_by_day.clone(),
    };
    let mut agents_total = 0;
    if let Some(dir) = locator.subagents_dir(id) {
        let tree = build_tree(&dir, false, SystemTime::now());
        agents_total = tree.total;
        add_agent_totals(&tree.roots, &mut totals);
    }
    let name = stats
        .title
        .clone()
        .filter(|t| !t.is_empty())
        .or_else(|| stats.cwd.as_deref().and_then(last_component).map(String::from))
        .unwrap_or_else(|| id.to_string());
    Ok(HistorySummary {
        name,
        cwd: stats.cwd.clone(),
        first_cwd: stats.first_cwd.clone(),
        model: stats.model.clone(),
        first_at_ms: stats.first_at.map_or(0, |t| t.timestamp_millis().max(0) as u64),
        last_at_ms: stats.last_at.map_or(0, |t| t.timestamp_millis().max(0) as u64),
        agents_total,
        cost_usd: totals.cost,
        cost_by_day: totals.cost_by_day,
        tokens: totals.tokens,
        tokens_by_day: totals.tokens_by_day,
    })
}

impl HistoryIndex {
    /// Cache plus file list. Rows without a (still valid) cached summary come back
    /// immediately with `summary: None`.
    pub fn load(cache_path: PathBuf, locator: &Locator) -> Self {
        let cached = load_cache(&cache_path);
        let mut cache = HashMap::new();
        let mut rows = Vec::new();
        for (path, md) in locator.all_transcripts() {
            let Some(mut row) = new_row(&path, &md) else { continue };
            let (size, modified_ms) = (row.size, row.modified_ms);
            if let Some((s, m, sum)) = cached.get(&path) {
                if *s == size && *m == modified_ms {
                    row.name = sum.name.clone();
                    row.cwd = sum.cwd.clone().or(row.cwd);
                    row.summary = Some(sum.clone());
                    cache.insert(path, (size, modified_ms, sum.clone()));
                }
            }
            rows.push(row);
        }
        Self { cache_path, rows, cache, failed: HashSet::new(), live: HashSet::new() }
    }

    /// Re-lists the transcripts: adds new files, drops vanished ones and marks
    /// closed files whose size/mtime changed for re-indexing. Ids in `live_ids`
    /// are left untouched and skipped by `step` until they close.
    /// Returns true when `step` has work afterwards.
    pub fn rescan(&mut self, locator: &Locator, live_ids: &HashSet<String>) -> bool {
        self.live = live_ids.clone();
        let files = locator.all_transcripts();
        let present: HashSet<&PathBuf> = files.iter().map(|(p, _)| p).collect();
        self.rows.retain(|r| present.contains(&r.path));
        let mut at: HashMap<PathBuf, usize> =
            self.rows.iter().enumerate().map(|(i, r)| (r.path.clone(), i)).collect();
        for (path, md) in &files {
            let Some(fresh) = new_row(path, md) else { continue };
            if live_ids.contains(&fresh.session_id) {
                continue;
            }
            match at.get(path) {
                Some(&i) => {
                    let r = &mut self.rows[i];
                    if r.size != fresh.size || r.modified_ms != fresh.modified_ms {
                        r.size = fresh.size;
                        r.modified_ms = fresh.modified_ms;
                        r.summary = None;
                        self.cache.remove(path);
                        self.failed.remove(path);
                    }
                }
                None => {
                    at.insert(path.clone(), self.rows.len());
                    self.rows.push(fresh);
                }
            }
        }
        self.has_work()
    }

    fn pending(&self, i: usize) -> bool {
        let r = &self.rows[i];
        r.summary.is_none() && !self.failed.contains(&r.path) && !self.live.contains(&r.session_id)
    }

    /// Whether `step` has anything left to index.
    pub fn has_work(&self) -> bool {
        (0..self.rows.len()).any(|i| self.pending(i))
    }

    /// Index unsummarized files, newest first, until the budget is spent. The budget is
    /// checked between files; a single file is always indexed completely.
    /// Returns true while work remains.
    pub fn step(&mut self, budget: Duration) -> bool {
        let start = Instant::now();
        let mut pending: Vec<usize> = (0..self.rows.len()).filter(|&i| self.pending(i)).collect();
        pending.sort_by(|&a, &b| self.rows[b].modified_ms.cmp(&self.rows[a].modified_ms));
        // Locator rooted at the projects dir, derived per file (subagents_dir needs it).
        for (n, &i) in pending.iter().enumerate() {
            if n > 0 && start.elapsed() >= budget {
                return true;
            }
            let (path, id, size, modified_ms) = {
                let r = &self.rows[i];
                (r.path.clone(), r.session_id.clone(), r.size, r.modified_ms)
            };
            let Some(root) = path.parent().and_then(|p| p.parent()) else {
                self.failed.insert(path);
                continue;
            };
            let mut locator = Locator::new(root.to_path_buf());
            match summarize(&path, &mut locator, &id) {
                Ok(sum) => {
                    let r = &mut self.rows[i];
                    r.name = sum.name.clone();
                    r.cwd = sum.cwd.clone().or(r.cwd.take());
                    r.summary = Some(sum.clone());
                    self.cache.insert(path, (size, modified_ms, sum));
                }
                Err(_) => {
                    self.failed.insert(path);
                }
            }
        }
        false
    }

    /// Atomic write: temp file next to the cache, then rename.
    pub fn save(&self) -> io::Result<()> {
        let live: HashSet<&PathBuf> = self.rows.iter().map(|r| &r.path).collect();
        let list: Vec<CacheEntry> = self
            .cache
            .iter()
            .filter(|(p, _)| live.contains(p))
            .map(|(p, (size, m, s))| CacheEntry {
                path: p.clone(),
                size: *size,
                modified_ms: *m,
                summary: s.clone(),
            })
            .collect();
        let json = serde_json::to_vec(&CacheFile { version: CACHE_VERSION, entries: list }).map_err(io::Error::other)?;
        if let Some(dir) = self.cache_path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut tmp = self.cache_path.clone().into_os_string();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        fs::write(&tmp, json)
            .and_then(|()| fs::rename(&tmp, &self.cache_path))
            .inspect_err(|_| {
                let _ = fs::remove_file(&tmp);
            })
    }

    /// Closed sessions only (live ids excluded), newest first.
    pub fn rows(&self, live_ids: &HashSet<String>) -> Vec<&HistoryRow> {
        let mut v: Vec<&HistoryRow> =
            self.rows.iter().filter(|r| !live_ids.contains(&r.session_id)).collect();
        v.sort_by(|a, b| b.modified_ms.cmp(&a.modified_ms).then_with(|| a.session_id.cmp(&b.session_id)));
        v
    }

    /// (cost on that local day across all sessions except `live_ids` - the caller
    /// adds those from the live monitor -, whether every such file that could hold
    /// that day - modified on/after its local midnight - has been indexed).
    pub fn cost_on(&self, day: NaiveDate, live_ids: &HashSet<String>) -> (f64, bool) {
        cost_on_rows(&self.rows, day, live_ids)
    }
}

/// [`HistoryIndex::cost_on`] over an already published row list.
pub fn cost_on_rows<'a>(
    rows: impl IntoIterator<Item = &'a HistoryRow>,
    day: NaiveDate,
    live_ids: &HashSet<String>,
) -> (f64, bool) {
    let midnight = day
        .and_time(NaiveTime::MIN)
        .and_local_timezone(Local)
        .earliest()
        .map_or(0, |t| t.timestamp_millis().max(0) as u64);
    let mut total = 0.0;
    let mut complete = true;
    for r in rows.into_iter().filter(|r| !live_ids.contains(&r.session_id)) {
        match &r.summary {
            Some(s) => total += s.cost_by_day.get(&day).copied().unwrap_or(0.0),
            None => {
                if r.modified_ms >= midnight {
                    complete = false;
                }
            }
        }
    }
    (total, complete)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration as CDur;

    const ID_A: &str = "aaaaaaaa-1111-4222-8333-444444444444";
    const ID_B: &str = "bbbbbbbb-1111-4222-8333-444444444444";
    const ID_C: &str = "cccccccc-1111-4222-8333-444444444444";

    fn line(ts: chrono::DateTime<Local>, msg: &str, input: u64) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"{}","cwd":"C:\\demo\\proj-x","message":{{"id":"{msg}","model":"claude-opus-5-5","content":[],"usage":{{"input_tokens":{input},"output_tokens":10}}}}}}"#,
            ts.to_utc().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        )
    }
    fn put(root: &Path, proj: &str, id: &str, body: &str, age_secs: u64) -> PathBuf {
        let p = root.join(proj).join(format!("{id}.jsonl"));
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(&p, body).unwrap();
        set_age(&p, age_secs);
        p
    }
    fn set_age(p: &Path, age_secs: u64) {
        let f = fs::OpenOptions::new().write(true).open(p).unwrap();
        f.set_modified(SystemTime::now() - Duration::from_secs(age_secs)).unwrap();
    }
    fn load(root: &Path, cache: &Path) -> HistoryIndex {
        HistoryIndex::load(cache.to_path_buf(), &Locator::new(root.to_path_buf()))
    }
    fn now() -> chrono::DateTime<Local> {
        Local::now()
    }

    #[test]
    fn rows_available_before_indexing() {
        let t = tempfile::tempdir().unwrap();
        put(t.path(), "d--x", ID_A, &(line(now(), "m1", 100) + "\n"), 10);
        let idx = load(t.path(), &t.path().join("cache.json"));
        let rows = idx.rows(&HashSet::new());
        assert_eq!(rows.len(), 1);
        let r = rows[0];
        assert!(r.summary.is_none());
        assert_eq!(r.session_id, ID_A);
        assert!(!r.name.is_empty());
        assert_eq!(r.cwd.as_deref(), Some("d--x"));
        assert!(r.modified_ms > 0);
        assert!(r.size > 0);
    }

    #[test]
    fn step_indexes_newest_first() {
        let t = tempfile::tempdir().unwrap();
        put(t.path(), "d--x", ID_A, &(line(now(), "m1", 1) + "\n"), 300);
        put(t.path(), "d--x", ID_B, &(line(now(), "m2", 1) + "\n"), 100);
        put(t.path(), "d--x", ID_C, &(line(now(), "m3", 1) + "\n"), 200);
        let mut idx = load(t.path(), &t.path().join("cache.json"));
        assert!(idx.step(Duration::ZERO), "work remains");
        let rows = idx.rows(&HashSet::new());
        let ids: Vec<_> = rows.iter().map(|r| r.session_id.as_str()).collect();
        assert_eq!(ids, [ID_B, ID_C, ID_A], "newest to oldest");
        assert!(rows[0].summary.is_some());
        assert!(rows[1].summary.is_none() && rows[2].summary.is_none());
        assert!(idx.step(Duration::ZERO));
        assert!(idx.rows(&HashSet::new())[1].summary.is_some());
        assert!(!idx.step(Duration::from_secs(5)), "all done");
        let s = idx.rows(&HashSet::new())[0].summary.clone().unwrap();
        assert_eq!(s.model.as_deref(), Some("claude-opus-5-5"));
        assert!(s.cost_usd > 0.0);
        assert_eq!(idx.rows(&HashSet::new())[0].name, "proj-x");
    }

    #[test]
    fn cache_skips_unchanged_files() {
        let t = tempfile::tempdir().unwrap();
        let cache = t.path().join("out").join("history-cache.json");
        let proj = t.path().join("projects");
        put(&proj, "d--x", ID_A, &(line(now(), "m1", 1) + "\n"), 50);
        let mut idx = load(&proj, &cache);
        while idx.step(Duration::from_secs(5)) {}
        idx.save().unwrap();
        assert!(cache.exists());
        assert!(!PathBuf::from(format!("{}.tmp", cache.display())).exists(), "temp file renamed away");
        let before = idx.rows(&HashSet::new())[0].summary.clone();
        let mut idx2 = load(&proj, &cache);
        let r = idx2.rows(&HashSet::new())[0];
        assert_eq!(r.summary, before, "summary comes from cache, with name/cwd");
        assert_eq!(r.name, "proj-x");
        assert!(!idx2.step(Duration::from_secs(5)), "nothing to re-read");
    }

    #[test]
    fn summary_keeps_first_and_last_cwd() {
        let t = tempfile::tempdir().unwrap();
        let first = line(now(), "m1", 1);
        let last = line(now(), "m2", 1).replace(r"C:\\demo\\proj-x", r"C:\\demo\\proj-x\\sub");
        put(t.path(), "C--demo-proj-x", ID_A, &(first + "\n" + &last + "\n"), 10);
        let mut idx = load(t.path(), &t.path().join("c.json"));
        while idx.step(Duration::from_secs(5)) {}
        let s = idx.rows(&HashSet::new())[0].summary.clone().unwrap();
        assert_eq!(s.first_cwd.as_deref(), Some(r"C:\demo\proj-x"));
        assert_eq!(s.cwd.as_deref(), Some(r"C:\demo\proj-x\sub"));
    }

    #[test]
    fn summary_has_exact_token_totals() {
        let t = tempfile::tempdir().unwrap();
        let body = line(now(), "m1", 100) + "
" + &line(now(), "m2", 50) + "
";
        put(t.path(), "d--x", ID_A, &body, 10);
        let mut idx = load(t.path(), &t.path().join("c.json"));
        while idx.step(Duration::from_secs(5)) {}
        let s = idx.rows(&HashSet::new())[0].summary.clone().unwrap();
        assert_eq!(s.tokens.input, 150);
        assert_eq!(s.tokens.output, 20);
        assert_eq!(s.tokens.total(), 170);
        assert_eq!(s.tokens_by_day.values().sum::<u64>(), 170);
    }

    #[test]
    fn cache_of_an_older_schema_is_rebuilt() {
        let t = tempfile::tempdir().unwrap();
        let proj = t.path().join("projects");
        let p = put(&proj, "d--x", ID_A, &(line(now(), "m1", 1) + "\n"), 50);
        let md = fs::metadata(&p).unwrap();
        let entry = serde_json::json!({
            "path": p,
            "size": md.len(),
            "modifiedMs": md.modified().map_or(0, ms),
            "summary": {"name": "eski", "cwd": "C:\\elsewhere", "model": null, "firstAtMs": 0,
                        "lastAtMs": 0, "agentsTotal": 0, "costUsd": 0.0, "costByDay": {}}
        });
        // v1: a bare array of entries (no first cwd).
        let cache = t.path().join("c.json");
        fs::write(&cache, serde_json::to_string(&vec![entry.clone()]).unwrap()).unwrap();
        assert!(load(&proj, &cache).rows(&HashSet::new())[0].summary.is_none(), "v1 cache ignored");
        // An envelope with another version is ignored too.
        let other = serde_json::json!({"version": CACHE_VERSION + 1, "entries": [entry]});
        fs::write(&cache, other.to_string()).unwrap();
        assert!(load(&proj, &cache).rows(&HashSet::new())[0].summary.is_none());
        // The current version round-trips.
        let mut idx = load(&proj, &cache);
        while idx.step(Duration::from_secs(5)) {}
        idx.save().unwrap();
        let v: serde_json::Value = serde_json::from_str(&fs::read_to_string(&cache).unwrap()).unwrap();
        assert_eq!(v["version"], CACHE_VERSION);
        let again = load(&proj, &cache);
        assert_eq!(again.rows(&HashSet::new())[0].summary.as_ref().unwrap().first_cwd.as_deref(), Some(r"C:\demo\proj-x"));
    }

    #[test]
    fn corrupt_cache_starts_empty() {
        let t = tempfile::tempdir().unwrap();
        let proj = t.path().join("projects");
        put(&proj, "d--x", ID_A, &(line(now(), "m1", 1) + "\n"), 50);
        let cache = t.path().join("c.json");
        fs::write(&cache, "{not json").unwrap();
        let idx = load(&proj, &cache);
        assert!(idx.rows(&HashSet::new())[0].summary.is_none());
    }

    #[test]
    fn changed_file_reindexed() {
        let t = tempfile::tempdir().unwrap();
        let cache = t.path().join("c.json");
        let proj = t.path().join("projects");
        let p = put(&proj, "d--x", ID_A, &(line(now(), "m1", 1000) + "\n"), 50);
        let mut idx = load(&proj, &cache);
        while idx.step(Duration::from_secs(5)) {}
        idx.save().unwrap();
        let old = idx.rows(&HashSet::new())[0].summary.clone().unwrap().cost_usd;
        let more = line(now(), "m1", 1000) + "\n" + &line(now(), "m2", 1000) + "\n";
        fs::write(&p, more).unwrap();
        let mut idx2 = load(&proj, &cache);
        assert!(idx2.rows(&HashSet::new())[0].summary.is_none(), "stale cache dropped");
        while idx2.step(Duration::from_secs(5)) {}
        let new = idx2.rows(&HashSet::new())[0].summary.clone().unwrap().cost_usd;
        assert!(new > old * 1.5, "{new} vs {old}");
    }

    #[test]
    fn live_sessions_excluded_from_rows() {
        let t = tempfile::tempdir().unwrap();
        put(t.path(), "d--x", ID_A, "{}\n", 10);
        put(t.path(), "d--x", ID_B, "{}\n", 20);
        let idx = load(t.path(), &t.path().join("c.json"));
        let live: HashSet<String> = [ID_A.to_string()].into();
        let rows = idx.rows(&live);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].session_id, ID_B);
    }

    #[test]
    fn cost_on_today_sums_across_sessions_and_reports_incomplete() {
        let t = tempfile::tempdir().unwrap();
        let today = now().date_naive();
        put(t.path(), "d--x", ID_A, &(line(now(), "m1", 1000) + "\n"), 10);
        put(t.path(), "d--y", ID_B, &(line(now(), "m2", 2000) + "\n"), 20);
        // closed long ago: not relevant to today's completeness
        let old = now() - CDur::days(5);
        put(t.path(), "d--z", ID_C, &(line(old, "m3", 3000) + "\n"), 5 * 86400);
        let mut idx = load(t.path(), &t.path().join("c.json"));
        assert!(!idx.cost_on(today, &HashSet::new()).1, "unindexed -> incomplete");
        idx.step(Duration::ZERO); // newest only
        let (partial, done) = idx.cost_on(today, &HashSet::new());
        assert!(!done && partial > 0.0);
        idx.step(Duration::ZERO); // second file
        let (total, done) = idx.cost_on(today, &HashSet::new());
        assert!(done, "all of today's files indexed; the old one is irrelevant");
        assert!(total > partial);
        while idx.step(Duration::from_secs(5)) {}
        let (total2, done2) = idx.cost_on(today, &HashSet::new());
        assert!(done2);
        assert!((total2 - total).abs() < 1e-12, "old session adds nothing to today");
        let (old_cost, _) = idx.cost_on(old.date_naive(), &HashSet::new());
        assert!(old_cost > 0.0);
    }

    fn loc(root: &Path) -> Locator {
        Locator::new(root.to_path_buf())
    }

    #[test]
    fn rescan_adds_new_files() {
        let t = tempfile::tempdir().unwrap();
        put(t.path(), "d--x", ID_A, &(line(now(), "m1", 1) + "\n"), 50);
        let mut idx = load(t.path(), &t.path().join("c.json"));
        while idx.step(Duration::from_secs(5)) {}
        assert!(!idx.rescan(&loc(t.path()), &HashSet::new()), "nothing new");
        put(t.path(), "d--y", ID_B, &(line(now(), "m2", 1) + "\n"), 10);
        assert!(idx.rescan(&loc(t.path()), &HashSet::new()), "new file -> work");
        let rows = idx.rows(&HashSet::new());
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].session_id, ID_B);
        assert!(rows[0].summary.is_none());
        assert!(rows[1].summary.is_some(), "indexed row kept");
        while idx.step(Duration::from_secs(5)) {}
        assert!(idx.rows(&HashSet::new())[0].summary.is_some());
        // a vanished file disappears from the rows
        fs::remove_file(t.path().join("d--x").join(format!("{ID_A}.jsonl"))).unwrap();
        idx.rescan(&loc(t.path()), &HashSet::new());
        assert_eq!(idx.rows(&HashSet::new()).len(), 1);
    }

    #[test]
    fn rescan_skips_live_ids() {
        let t = tempfile::tempdir().unwrap();
        let mut idx = load(t.path(), &t.path().join("c.json"));
        put(t.path(), "d--x", ID_A, &(line(now(), "m1", 1) + "\n"), 10);
        let live: HashSet<String> = [ID_A.to_string()].into();
        assert!(!idx.rescan(&loc(t.path()), &live), "a live file is not work");
        assert!(!idx.step(Duration::from_secs(5)));
        assert!(idx.rows(&HashSet::new()).is_empty(), "not added while live");
        // live at launch (row exists from load): step leaves it alone
        put(t.path(), "d--x", ID_B, &(line(now(), "m2", 1) + "\n"), 10);
        let mut idx = load(t.path(), &t.path().join("c.json"));
        let live: HashSet<String> = [ID_A.to_string(), ID_B.to_string()].into();
        assert!(!idx.rescan(&loc(t.path()), &live));
        assert!(!idx.step(Duration::from_secs(5)));
        assert!(idx.rows(&HashSet::new()).iter().all(|r| r.summary.is_none()));
        // once closed it is indexed
        assert!(idx.rescan(&loc(t.path()), &HashSet::new()));
        while idx.step(Duration::from_secs(5)) {}
        assert!(idx.rows(&HashSet::new()).iter().all(|r| r.summary.is_some()));
    }

    #[test]
    fn rescan_invalidates_changed_closed_file() {
        let t = tempfile::tempdir().unwrap();
        let p = put(t.path(), "d--x", ID_A, &(line(now(), "m1", 1000) + "\n"), 50);
        let mut idx = load(t.path(), &t.path().join("c.json"));
        while idx.step(Duration::from_secs(5)) {}
        let old = idx.rows(&HashSet::new())[0].summary.clone().unwrap().cost_usd;
        fs::write(&p, line(now(), "m1", 1000) + "\n" + &line(now(), "m2", 1000) + "\n").unwrap();
        assert!(idx.rescan(&loc(t.path()), &HashSet::new()));
        assert!(idx.rows(&HashSet::new())[0].summary.is_none(), "stale summary dropped");
        while idx.step(Duration::from_secs(5)) {}
        let new = idx.rows(&HashSet::new())[0].summary.clone().unwrap().cost_usd;
        assert!(new > old * 1.5, "{new} vs {old}");
        idx.save().unwrap();
        let idx2 = load(t.path(), &t.path().join("c.json"));
        assert!(idx2.rows(&HashSet::new())[0].summary.is_some(), "re-indexed summary cached");
    }

    #[test]
    fn cost_on_excludes_live_ids_and_their_completeness() {
        let t = tempfile::tempdir().unwrap();
        let today = now().date_naive();
        put(t.path(), "d--x", ID_A, &(line(now(), "m1", 1000) + "\n"), 10);
        put(t.path(), "d--y", ID_B, &(line(now(), "m2", 1000) + "\n"), 20);
        let mut idx = load(t.path(), &t.path().join("c.json"));
        let live: HashSet<String> = [ID_A.to_string()].into();
        idx.rescan(&loc(t.path()), &live);
        while idx.step(Duration::from_secs(5)) {}
        let (closed, done) = idx.cost_on(today, &live);
        assert!(done, "the unindexed live file does not make today incomplete");
        assert!(closed > 0.0);
        assert!(!idx.cost_on(today, &HashSet::new()).1, "without exclusion it is incomplete");
    }

    #[test]
    fn rows_do_not_serialize_path() {
        let t = tempfile::tempdir().unwrap();
        put(t.path(), "d--x", ID_A, "{}\n", 10);
        let idx = load(t.path(), &t.path().join("c.json"));
        let v = serde_json::to_value(idx.rows(&HashSet::new())[0]).unwrap();
        assert!(v.get("path").is_none());
        assert!(v.get("sessionId").is_some());
    }

    #[test]
    fn subagent_cost_is_added_to_session() {
        let t = tempfile::tempdir().unwrap();
        let p = put(t.path(), "d--x", ID_A, &(line(now(), "m1", 1000) + "\n"), 10);
        let alone = {
            let mut i = load(t.path(), &t.path().join("c1.json"));
            while i.step(Duration::from_secs(5)) {}
            i.rows(&HashSet::new())[0].summary.clone().unwrap().cost_usd
        };
        let sub = p.parent().unwrap().join(ID_A).join("subagents");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("agent-a1.jsonl"), line(now(), "s1", 1000) + "\n").unwrap();
        fs::write(sub.join("agent-a1.meta.json"), r#"{"agentType":"x"}"#).unwrap();
        let mut i = load(t.path(), &t.path().join("c2.json"));
        while i.step(Duration::from_secs(5)) {}
        let s = i.rows(&HashSet::new())[0].summary.clone().unwrap();
        assert_eq!(s.agents_total, 1);
        assert!((s.cost_usd - alone * 2.0).abs() < 1e-12);
        let by_day: f64 = s.cost_by_day.values().sum();
        assert!((by_day - s.cost_usd).abs() < 1e-12);
    }
}
