//! Live monitor: registry + transcripts + agent trees -> UI snapshots.
use crate::agents::{AgentCache, AgentKind, AgentNode, AgentState, AgentTree};
use crate::locate::Locator;
use crate::model::{ToolCall, Usage};
use crate::registry::{read_live_sessions, LiveSession, ProcessProbe, SessionStatus};
use crate::stats::SessionStats;
use crate::transcript::{ReadEvent, TranscriptReader};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// Minimum gap between transcript lookups for a session whose file was not found.
const LOOKUP_RETRY: Duration = Duration::from_secs(10);
/// Transcript bytes read per live session per refresh; a huge transcript fills
/// in over several ticks instead of delaying the first snapshot.
pub const PUMP_BUDGET: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCard {
    pub session_id: String,
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub name: String,
    pub cwd: String,
    pub status: SessionStatus,
    pub waiting_for: Option<String>,
    pub model: Option<String>,
    pub context_pct: Option<f32>,
    pub cost_usd: f64,
    pub agents_total: u32,
    pub agents_finished: u32,
    pub last_tool: Option<ToolCall>,
    pub started_at_ms: u64,
    pub last_activity_ms: u64,
    pub git_branch: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub sessions: Vec<SessionCard>,
    pub running_agents: u32,
    pub waiting_count: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionDetail {
    pub card: SessionCard,
    pub tokens: Usage,
    pub tool_call_count: u64,
    pub tree: AgentTree,
    pub main_tool_calls: Vec<ToolCall>,
}

struct SessionState {
    reader: Option<TranscriptReader>,
    stats: SessionStats,
    agents: AgentCache,
    /// Agent tree from the last update (today-cost of live sessions).
    tree: AgentTree,
    /// When a failed transcript lookup last happened.
    last_lookup: Option<SystemTime>,
    /// Entries applied so far (incremental-read check in tests).
    #[cfg(test)]
    applied: usize,
}

impl SessionState {
    fn new() -> Self {
        Self {
            reader: None,
            stats: SessionStats::default(),
            agents: AgentCache::default(),
            tree: AgentTree::default(),
            last_lookup: None,
            #[cfg(test)]
            applied: 0,
        }
    }

    /// Reads newly appended transcript lines into `stats`.
    fn pump(&mut self, budget: u64) {
        let Some(reader) = self.reader.as_mut() else { return };
        let stats = &mut self.stats;
        #[cfg(test)]
        let applied = &mut self.applied;
        let _ = reader.read_each_limited(budget, |ev| match ev {
            ReadEvent::Reset => *stats = SessionStats::default(),
            ReadEvent::Entry(e) => {
                #[cfg(test)]
                {
                    *applied += 1;
                }
                stats.apply(&e);
            }
        });
    }
}

pub struct Monitor {
    claude_dir: PathBuf,
    locator: Locator,
    states: HashMap<String, SessionState>,
    pump_budget: u64,
}

fn dt_ms(t: Option<chrono::DateTime<chrono::Utc>>) -> Option<u64> {
    t.map(|t| t.timestamp_millis().max(0) as u64)
}

/// Cost of all agent nodes (workflow/phase groups carry no cost of their own).
fn agent_cost(nodes: &[AgentNode]) -> f64 {
    nodes
        .iter()
        .map(|n| {
            let own = if n.kind == AgentKind::Agent { n.stats.cost_usd } else { 0.0 };
            own + agent_cost(&n.children)
        })
        .sum()
}

fn running_agents(nodes: &[AgentNode]) -> u32 {
    nodes
        .iter()
        .map(|n| {
            let own = (n.kind == AgentKind::Agent && n.state == AgentState::Running) as u32;
            own + running_agents(&n.children)
        })
        .sum()
}

fn status_rank(s: &SessionStatus) -> u8 {
    match s {
        SessionStatus::Waiting => 0,
        SessionStatus::Busy => 1,
        SessionStatus::Idle => 2,
        SessionStatus::Other(_) => 3,
    }
}

fn cwd_name(cwd: &str) -> String {
    cwd.trim_end_matches(['/', '\\']).rsplit(['/', '\\']).next().unwrap_or("").to_string()
}

fn make_card(
    session_id: &str,
    live: Option<&LiveSession>,
    parent_pid: Option<u32>,
    stats: &SessionStats,
    tree: &AgentTree,
) -> SessionCard {
    let cwd = live
        .map(|l| l.cwd.clone())
        .filter(|c| !c.is_empty())
        .or_else(|| stats.cwd.clone())
        .unwrap_or_default();
    let name = live
        .and_then(|l| l.name.clone())
        .filter(|n| !n.is_empty())
        .or_else(|| stats.title.clone())
        .unwrap_or_else(|| cwd_name(&cwd));
    SessionCard {
        session_id: session_id.to_string(),
        pid: live.map_or(0, |l| l.pid),
        parent_pid,
        name,
        cwd,
        status: live.map_or(SessionStatus::Idle, |l| l.status.clone()),
        waiting_for: live.and_then(|l| l.waiting_for.clone()),
        model: stats.model.clone(),
        context_pct: stats.context_pct(),
        cost_usd: stats.cost_usd + agent_cost(&tree.roots),
        agents_total: tree.total,
        agents_finished: tree.finished,
        last_tool: stats.tool_calls.back().cloned(),
        started_at_ms: live.map(|l| l.started_at_ms).or_else(|| dt_ms(stats.first_at)).unwrap_or(0),
        last_activity_ms: live
            .map_or(0, |l| l.status_updated_at_ms)
            .max(dt_ms(stats.last_at).unwrap_or(0)),
        git_branch: stats.git_branch.clone(),
    }
}

impl Monitor {
    pub fn new(claude_dir: PathBuf) -> Self {
        let locator = Locator::new(claude_dir.join("projects"));
        Self { claude_dir, locator, states: HashMap::new(), pump_budget: PUMP_BUDGET }
    }

    /// Finds the transcript for a live session (misses retried at most every
    /// `LOOKUP_RETRY`), pumps new lines and returns the session's agent tree.
    fn update_live(&mut self, id: &str, now: SystemTime) -> AgentTree {
        let st = self.states.entry(id.to_string()).or_insert_with(SessionState::new);
        if st.reader.is_none() {
            let due = st
                .last_lookup
                .is_none_or(|t| now.duration_since(t).map_or(true, |d| d >= LOOKUP_RETRY));
            if due {
                match self.locator.transcript(id) {
                    Some(p) => st.reader = Some(TranscriptReader::new(p)),
                    None => st.last_lookup = Some(now),
                }
            }
        }
        st.pump(self.pump_budget);
        if st.reader.is_none() {
            return AgentTree::default();
        }
        let tree = match self.locator.subagents_dir(id) {
            Some(dir) => st.agents.build(&dir, true, now),
            None => AgentTree::default(),
        };
        st.tree = tree.clone();
        tree
    }

    /// Cost on `day` of the sessions live at the last refresh (main transcript
    /// plus agents), and their ids. History excludes these ids, so the two sums
    /// never overlap.
    pub fn live_cost_on(&self, day: chrono::NaiveDate) -> (f64, HashSet<String>) {
        fn agents_on(nodes: &[AgentNode], day: chrono::NaiveDate) -> f64 {
            nodes
                .iter()
                .map(|n| {
                    let own = if n.kind == AgentKind::Agent {
                        n.stats.cost_by_day.get(&day).copied().unwrap_or(0.0)
                    } else {
                        0.0
                    };
                    own + agents_on(&n.children, day)
                })
                .sum()
        }
        let mut total = 0.0;
        for st in self.states.values() {
            total += st.stats.cost_by_day.get(&day).copied().unwrap_or(0.0);
            total += agents_on(&st.tree.roots, day);
        }
        (total, self.states.keys().cloned().collect())
    }

    pub fn refresh(&mut self, probe: &dyn ProcessProbe, now: SystemTime) -> Snapshot {
        let live = read_live_sessions(&self.claude_dir.join("sessions"), probe);
        let ids: HashSet<&str> = live.iter().map(|l| l.session_id.as_str()).collect();
        self.states.retain(|k, _| ids.contains(k.as_str()));

        let mut cards = Vec::new();
        let (mut running, mut waiting) = (0u32, 0u32);
        for l in &live {
            let tree = self.update_live(&l.session_id, now);
            running += running_agents(&tree.roots);
            if l.status == SessionStatus::Waiting {
                waiting += 1;
            }
            let stats = &self.states[&l.session_id].stats;
            cards.push(make_card(&l.session_id, Some(l), probe.parent_pid(l.pid), stats, &tree));
        }
        cards.sort_by(|a, b| {
            status_rank(&a.status)
                .cmp(&status_rank(&b.status))
                .then(b.last_activity_ms.cmp(&a.last_activity_ms))
        });
        Snapshot { sessions: cards, running_agents: running, waiting_count: waiting }
    }

    pub fn detail(
        &mut self,
        session_id: &str,
        probe: &dyn ProcessProbe,
        now: SystemTime,
    ) -> Option<SessionDetail> {
        match self.live_detail(session_id, probe, now) {
            Some(d) => Some(d),
            None => self.closed_job(session_id)?.run(session_id, now),
        }
    }

    /// Detail of a live session (cheap: incremental state). `None` if not live.
    pub fn live_detail(
        &mut self,
        session_id: &str,
        probe: &dyn ProcessProbe,
        now: SystemTime,
    ) -> Option<SessionDetail> {
        let l = read_live_sessions(&self.claude_dir.join("sessions"), probe)
            .into_iter()
            .find(|l| l.session_id == session_id)?;
        let tree = self.update_live(session_id, now);
        let stats = &self.states[session_id].stats;
        Some(SessionDetail {
            card: make_card(session_id, Some(&l), probe.parent_pid(l.pid), stats, &tree),
            tokens: stats.tokens,
            tool_call_count: stats.tool_call_count,
            main_tool_calls: stats.tool_calls.iter().cloned().collect(),
            tree,
        })
    }

    /// Locates a closed session's files; the expensive streaming is done by
    /// `ClosedJob::run`, which needs no access to the monitor (no lock held).
    pub fn closed_job(&mut self, session_id: &str) -> Option<ClosedJob> {
        let path = self.locator.transcript(session_id)?;
        let subagents_dir = self.locator.subagents_dir(session_id);
        Some(ClosedJob { path, subagents_dir })
    }
}

/// Everything needed to build a closed session's detail without the monitor.
pub struct ClosedJob {
    path: PathBuf,
    subagents_dir: Option<PathBuf>,
}

/// Identity of a closed transcript's content: (path, length, mtime in ms).
pub type ClosedKey = (PathBuf, u64, u64);

impl ClosedJob {
    /// Cache key for the detail this job would build; `None` if the file is gone.
    pub fn key(&self) -> Option<ClosedKey> {
        let md = std::fs::metadata(&self.path).ok()?;
        let m = md
            .modified()
            .ok()
            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_millis() as u64);
        Some((self.path.clone(), md.len(), m))
    }

    pub fn run(self, session_id: &str, now: SystemTime) -> Option<SessionDetail> {
        let mut reader = TranscriptReader::new(self.path);
        let mut stats = SessionStats::default();
        reader
            .read_each(|ev| match ev {
                ReadEvent::Reset => stats = SessionStats::default(),
                ReadEvent::Entry(e) => stats.apply(&e),
            })
            .ok()?;
        let tree = match self.subagents_dir {
            Some(dir) => AgentCache::default().build(&dir, false, now),
            None => AgentTree::default(),
        };
        Some(SessionDetail {
            card: make_card(session_id, None, None, &stats, &tree),
            tokens: stats.tokens,
            tool_call_count: stats.tool_call_count,
            main_tool_calls: stats.tool_calls.iter().cloned().collect(),
            tree,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::time::UNIX_EPOCH;

    struct FakeProbe;
    impl ProcessProbe for FakeProbe {
        fn start_filetime(&self, pid: u32) -> Option<u64> {
            Some(1000 + pid as u64)
        }
        fn parent_pid(&self, pid: u32) -> Option<u32> {
            Some(pid + 1)
        }
    }

    const NOW_S: u64 = 1_800_000_000;
    fn now() -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(NOW_S)
    }
    fn id(n: u32) -> String {
        format!("00000000-0000-4000-8000-{n:012x}")
    }

    struct Env {
        dir: tempfile::TempDir,
    }
    impl Env {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            fs::create_dir_all(dir.path().join("sessions")).unwrap();
            fs::create_dir_all(dir.path().join("projects").join("proj-a")).unwrap();
            Env { dir }
        }
        fn reg(&self, pid: u32, sid: &str, status: &str, updated: u64, name: Option<&str>) {
            let name = name.map(|n| format!(r#","name":"{n}""#)).unwrap_or_default();
            let j = format!(
                r#"{{"pid":{pid},"sessionId":"{sid}","cwd":"C:\\work\\demo","startedAt":1000,"procStart":"{}","status":"{status}","statusUpdatedAt":{updated}{name}}}"#,
                1000 + pid as u64
            );
            fs::write(self.dir.path().join("sessions").join(format!("{pid}.json")), j).unwrap();
        }
        fn transcript(&self, sid: &str) -> PathBuf {
            self.dir.path().join("projects").join("proj-a").join(format!("{sid}.jsonl"))
        }
        fn append(&self, path: &PathBuf, line: &str) {
            let mut f = fs::OpenOptions::new().create(true).append(true).open(path).unwrap();
            writeln!(f, "{line}").unwrap();
        }
        fn mon(&self) -> Monitor {
            Monitor::new(self.dir.path().to_path_buf())
        }
    }

    fn asst(mid: &str, out: u64, ts: &str) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"{ts}","cwd":"C:\\work\\demo","gitBranch":"main","message":{{"id":"{mid}","model":"claude-sonnet-5-5","usage":{{"input_tokens":10,"output_tokens":{out}}},"content":[]}}}}"#
        )
    }

    #[test]
    fn snapshot_lists_only_live_sessions() {
        let e = Env::new();
        e.reg(1, &id(1), "idle", 10, None);
        // pid 2 file exists but procStart mismatches -> not live
        fs::write(
            e.dir.path().join("sessions/2.json"),
            format!(r#"{{"pid":2,"sessionId":"{}","cwd":"x","procStart":"5","status":"busy"}}"#, id(2)),
        )
        .unwrap();
        e.append(&e.transcript(&id(2)), &asst("m1", 5, "2026-11-02T07:00:00Z"));
        let snap = e.mon().refresh(&FakeProbe, now());
        assert_eq!(snap.sessions.len(), 1);
        assert_eq!(snap.sessions[0].session_id, id(1));
        assert_eq!(snap.sessions[0].parent_pid, Some(2));
    }

    #[test]
    fn sorted_waiting_then_busy_then_idle_then_recent() {
        let e = Env::new();
        e.reg(1, &id(1), "idle", 100, None);
        e.reg(2, &id(2), "busy", 50, None);
        e.reg(3, &id(3), "waiting", 10, None);
        e.reg(4, &id(4), "idle", 200, None);
        e.reg(5, &id(5), "weird", 999, None);
        e.reg(6, &id(6), "busy", 60, None);
        let snap = e.mon().refresh(&FakeProbe, now());
        let order: Vec<_> = snap.sessions.iter().map(|c| c.pid).collect();
        assert_eq!(order, vec![3, 6, 2, 4, 1, 5]);
        assert_eq!(snap.waiting_count, 1);
    }

    #[test]
    fn refresh_is_incremental() {
        let e = Env::new();
        let sid = id(1);
        e.reg(1, &sid, "busy", 10, None);
        let t = e.transcript(&sid);
        e.append(&t, &asst("m1", 5, "2026-11-02T07:00:00Z"));
        let mut m = e.mon();
        m.refresh(&FakeProbe, now());
        let first = m.states[&sid].applied;
        assert!(first > 0);
        m.refresh(&FakeProbe, now());
        assert_eq!(m.states[&sid].applied, first, "nothing new -> nothing re-read");
        e.append(&t, &asst("m2", 7, "2026-11-02T07:00:05Z"));
        m.refresh(&FakeProbe, now());
        let after = m.states[&sid].applied;
        assert_eq!(after - first, 2, "only the new line (assistant + meta) is read");
        assert_eq!(m.states[&sid].stats.tokens.output, 12);
    }

    #[test]
    fn pump_respects_byte_budget() {
        let e = Env::new();
        let sid = id(1);
        e.reg(1, &sid, "busy", 10, None);
        let t = e.transcript(&sid);
        let n = 2000;
        for i in 0..n {
            e.append(&t, &asst(&format!("m{i}"), 1, "2026-11-02T07:00:00Z"));
        }
        let mut m = e.mon();
        m.pump_budget = 64 * 1024;
        let snap = m.refresh(&FakeProbe, now());
        assert_eq!(snap.sessions.len(), 1, "card appears at once");
        let first = m.states[&sid].stats.tokens.output;
        assert!(first > 0 && first < n, "partial stats after one tick: {first}");
        let mut ticks = 1;
        while m.states[&sid].stats.tokens.output < n {
            m.refresh(&FakeProbe, now());
            ticks += 1;
            assert!(ticks < 100);
        }
        assert_eq!(m.states[&sid].stats.tokens.output, n, "every line exactly once");
        assert!(ticks > 2);
    }

    #[test]
    fn live_cost_on_sums_main_and_agents_for_the_day() {
        let e = Env::new();
        let sid = id(1);
        e.reg(1, &sid, "busy", 10, None);
        e.append(&e.transcript(&sid), &asst("m1", 1_000_000, "2026-11-02T12:00:00Z"));
        let sub = e.dir.path().join("projects/proj-a").join(&sid).join("subagents");
        fs::create_dir_all(&sub).unwrap();
        e.append(&sub.join("agent-a1.jsonl"), &asst("s1", 1_000_000, "2026-11-02T12:00:00Z"));
        let mut m = e.mon();
        m.refresh(&FakeProbe, now());
        let day = chrono::DateTime::parse_from_rfc3339("2026-11-02T12:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Local)
            .date_naive();
        let (cost, ids) = m.live_cost_on(day);
        let main = m.states[&sid].stats.cost_usd;
        assert!((cost - 2.0 * main).abs() < 1e-9, "{cost} vs {main}");
        assert_eq!(ids, HashSet::from([sid.clone()]));
        assert_eq!(m.live_cost_on(day.succ_opt().unwrap()).0, 0.0);
    }

    #[test]
    fn closed_job_key_tracks_size() {
        let e = Env::new();
        let sid = id(9);
        let t = e.transcript(&sid);
        e.append(&t, &asst("m1", 5, "2026-11-02T07:00:00Z"));
        let mut m = e.mon();
        let k1 = m.closed_job(&sid).unwrap().key().unwrap();
        assert_eq!(k1, m.closed_job(&sid).unwrap().key().unwrap());
        e.append(&t, &asst("m2", 5, "2026-11-02T07:00:01Z"));
        assert_ne!(k1, m.closed_job(&sid).unwrap().key().unwrap());
    }

    #[test]
    fn ended_session_state_dropped() {
        let e = Env::new();
        let sid = id(1);
        e.reg(1, &sid, "busy", 10, None);
        e.append(&e.transcript(&sid), &asst("m1", 5, "2026-11-02T07:00:00Z"));
        let mut m = e.mon();
        m.refresh(&FakeProbe, now());
        assert!(m.states.contains_key(&sid));
        fs::remove_file(e.dir.path().join("sessions/1.json")).unwrap();
        let snap = m.refresh(&FakeProbe, now());
        assert!(snap.sessions.is_empty());
        assert!(m.states.is_empty());
    }

    #[test]
    fn session_cost_includes_subagents() {
        let e = Env::new();
        let sid = id(1);
        e.reg(1, &sid, "busy", 10, None);
        e.append(&e.transcript(&sid), &asst("m1", 1_000_000, "2026-11-02T07:00:00Z"));
        let sub = e.dir.path().join("projects/proj-a").join(&sid).join("subagents");
        fs::create_dir_all(&sub).unwrap();
        e.append(&sub.join("agent-a1.jsonl"), &asst("s1", 1_000_000, "2026-11-02T07:00:00Z"));
        fs::write(sub.join("agent-a1.meta.json"), r#"{"agentType":"Explore","finished":true}"#).unwrap();
        let mut m = e.mon();
        let snap = m.refresh(&FakeProbe, now());
        let c = &snap.sessions[0];
        let main = m.states[&sid].stats.cost_usd;
        assert!(main > 0.0);
        assert!((c.cost_usd - 2.0 * main).abs() < 1e-9, "{} vs {}", c.cost_usd, main);
        assert_eq!(c.agents_total, 1);
        assert_eq!(c.agents_finished, 1);
    }

    #[test]
    fn name_falls_back_to_cwd_folder() {
        let e = Env::new();
        e.reg(1, &id(1), "idle", 10, None);
        e.reg(2, &id(2), "idle", 20, Some("kayitli"));
        let snap = e.mon().refresh(&FakeProbe, now());
        let by = |p| snap.sessions.iter().find(|c| c.pid == p).unwrap();
        assert_eq!(by(1).name, "demo");
        assert_eq!(by(2).name, "kayitli");
    }

    #[test]
    fn detail_for_closed_session_from_history() {
        let e = Env::new();
        let sid = id(9);
        e.append(&e.transcript(&sid), &asst("m1", 5, "2026-11-02T07:00:00Z"));
        let sub = e.dir.path().join("projects/proj-a").join(&sid).join("subagents");
        fs::create_dir_all(&sub).unwrap();
        // fresh mtime and no finished flag: would be Running if the session were live
        e.append(&sub.join("agent-a1.jsonl"), &asst("s1", 5, "2026-11-02T07:00:00Z"));
        let d = e.mon().detail(&sid, &FakeProbe, now()).unwrap();
        assert_eq!(d.card.pid, 0);
        assert_eq!(d.card.parent_pid, None);
        assert_eq!(d.card.status, SessionStatus::Idle);
        assert_eq!(d.card.name, "demo");
        assert_eq!(d.tokens.output, 5);
        assert_eq!(d.tree.total, 1);
        assert_eq!(d.tree.finished, 1);
        assert!(d.tree.roots.iter().all(|n| n.state == AgentState::Finished));
        assert!(e.mon().detail(&id(77), &FakeProbe, now()).is_none());
    }
}
