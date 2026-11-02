//! Subagent tree built from `<session>/subagents/` (agent-*.jsonl + meta.json).
use crate::model::ToolState;
use crate::stats::SessionStats;
use crate::transcript::TranscriptReader;
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// An agent whose transcript changed within this window counts as running.
pub const RUNNING_WINDOW: Duration = Duration::from_secs(2 * 60);

/// Label of the phase group for workflow agents without a `workflowPhase`.
const NO_PHASE: &str = "Aşamasız";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentState {
    Running,
    Finished,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AgentKind {
    Agent,
    Workflow { id: String },
    Phase { name: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentNode {
    pub id: String,
    pub kind: AgentKind,
    pub agent_type: Option<String>,
    pub description: Option<String>,
    pub state: AgentState,
    pub stats: SessionStats,
    pub children: Vec<AgentNode>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTree {
    pub roots: Vec<AgentNode>,
    pub total: u32,
    pub finished: u32,
}

#[derive(Clone)]
struct Meta {
    agent_type: Option<String>,
    description: Option<String>,
    parent: Option<String>,
    phase: Option<String>,
    finished: bool,
}

fn read_meta(path: &Path) -> Meta {
    let v: Value = fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or(Value::Null);
    let s = |k: &str| v.get(k).and_then(Value::as_str).map(String::from);
    Meta {
        agent_type: s("agentType"),
        description: s("description"),
        parent: s("parentAgentId"),
        phase: s("workflowPhase"),
        finished: v.get("finished").and_then(Value::as_bool).unwrap_or(false),
    }
}

/// Every `agent-<id>.jsonl` directly in `dir`, as (id, path), sorted by id.
fn agent_files(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(rd) = fs::read_dir(dir) else { return Vec::new() };
    let mut out: Vec<(String, PathBuf)> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let id = name.strip_prefix("agent-")?.strip_suffix(".jsonl")?;
            if id.is_empty() || !e.path().is_file() {
                return None;
            }
            Some((id.to_string(), e.path()))
        })
        .collect();
    out.sort();
    out
}

struct Loaded {
    node: AgentNode,
    parent: Option<String>,
    phase: Option<String>,
}


type Time = Option<chrono::DateTime<chrono::Utc>>;

/// Earliest first; no time last.
fn cmp_time(a: Time, b: Time) -> std::cmp::Ordering {
    match (a, b) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

fn sort_nodes(v: &mut [AgentNode]) {
    v.sort_by(|a, b| cmp_time(a.stats.first_at, b.stats.first_at).then_with(|| a.id.cmp(&b.id)));
}

fn first_of<'a>(it: impl Iterator<Item = &'a AgentNode>) -> Time {
    it.filter_map(|n| n.stats.first_at).min()
}

fn any_running<'a>(mut it: impl Iterator<Item = &'a AgentNode>) -> AgentState {
    if it.any(|c| c.state == AgentState::Running) {
        AgentState::Running
    } else {
        AgentState::Finished
    }
}

fn attach(id: &str, nodes: &mut HashMap<String, AgentNode>, kids: &HashMap<String, Vec<String>>) -> Option<AgentNode> {
    let mut n = nodes.remove(id)?;
    for c in kids.get(id).into_iter().flatten() {
        if let Some(child) = attach(c, nodes, kids) {
            n.children.push(child);
        }
    }
    sort_nodes(&mut n.children);
    Some(n)
}

fn count(n: &AgentNode, total: &mut u32, finished: &mut u32) {
    if n.kind == AgentKind::Agent {
        *total += 1;
        if n.state == AgentState::Finished {
            *finished += 1;
        }
    }
    for c in &n.children {
        count(c, total, finished);
    }
}


struct Cached {
    reader: TranscriptReader,
    stats: SessionStats,
    meta: Meta,
    meta_sig: Option<(SystemTime, u64)>,
}

/// Keeps per-agent readers and stats between polls so each build only reads
/// bytes appended since the previous one.
#[derive(Default)]
pub struct AgentCache {
    agents: HashMap<PathBuf, Cached>,
    seen: HashSet<PathBuf>,
}

impl AgentCache {
    fn load(&mut self, id: &str, jsonl: &Path, session_live: bool, now: SystemTime) -> Loaded {
        self.seen.insert(jsonl.to_path_buf());
        let meta_path = jsonl.with_extension("meta.json");
        let sig = fs::metadata(&meta_path).ok().and_then(|m| Some((m.modified().ok()?, m.len())));
        let c = self.agents.entry(jsonl.to_path_buf()).or_insert_with(|| Cached {
            reader: TranscriptReader::new(jsonl.to_path_buf()),
            stats: SessionStats::default(),
            meta: read_meta(&meta_path),
            meta_sig: sig,
        });
        if c.meta_sig != sig {
            c.meta = read_meta(&meta_path);
            c.meta_sig = sig;
        }
        if let Ok(out) = c.reader.read_new() {
            if out.reset {
                c.stats = SessionStats::default();
            }
            for e in &out.entries {
                c.stats.apply(e);
            }
        }
        let meta = c.meta.clone();
        let stats = c.stats.clone();
        let state = if meta.finished || !session_live {
            AgentState::Finished
        } else {
            let recent = fs::metadata(jsonl)
                .and_then(|m| m.modified())
                .map(|mt| now.duration_since(mt).map_or(true, |d| d <= RUNNING_WINDOW))
                .unwrap_or(false);
            let open_tool = stats.tool_calls.back().map_or(false, |c| c.state == ToolState::Running);
            if recent || open_tool {
                AgentState::Running
            } else {
                AgentState::Finished
            }
        };
        Loaded {
            node: AgentNode {
                id: id.to_string(),
                kind: AgentKind::Agent,
                agent_type: meta.agent_type,
                description: meta.description,
                state,
                stats,
                children: Vec::new(),
            },
            parent: meta.parent,
            phase: meta.phase,
        }
    }

    fn workflow_node(&mut self, dir: &Path, session_live: bool, now: SystemTime) -> Option<AgentNode> {
        let wf_id = dir.file_name()?.to_string_lossy().into_owned();
        let mut phases: Vec<(String, Vec<AgentNode>)> = Vec::new();
        for (id, p) in agent_files(dir) {
            let l = self.load(&id, &p, session_live, now);
            let name = l.phase.unwrap_or_else(|| NO_PHASE.to_string());
            match phases.iter_mut().find(|(n, _)| *n == name) {
                Some((_, v)) => v.push(l.node),
                None => phases.push((name, vec![l.node])),
            }
        }
        if phases.is_empty() {
            return None;
        }
        let mut phase_nodes: Vec<AgentNode> = phases
            .into_iter()
            .map(|(name, mut agents)| {
                sort_nodes(&mut agents);
                let mut stats = SessionStats::default();
                stats.first_at = first_of(agents.iter());
                AgentNode {
                    id: format!("{wf_id}/{name}"),
                    state: any_running(agents.iter()),
                    kind: AgentKind::Phase { name },
                    agent_type: None,
                    description: None,
                    stats,
                    children: agents,
                }
            })
            .collect();
        sort_nodes(&mut phase_nodes);
        let mut stats = SessionStats::default();
        stats.first_at = first_of(phase_nodes.iter());
        Some(AgentNode {
            id: wf_id.clone(),
            kind: AgentKind::Workflow { id: wf_id },
            agent_type: None,
            description: None,
            state: any_running(phase_nodes.iter()),
            stats,
            children: phase_nodes,
        })
    }

    pub fn build(&mut self, subagents_dir: &Path, session_live: bool, now: SystemTime) -> AgentTree {
        self.seen.clear();
        let loaded: Vec<Loaded> = agent_files(subagents_dir)
            .iter()
            .map(|(id, p)| self.load(id, p, session_live, now))
            .collect();
        // Known parents only; self-parent counts as none.
        let parent_of: HashMap<String, String> = loaded
            .iter()
            .filter_map(|l| {
                let p = l.parent.as_ref()?;
                (p != &l.node.id && loaded.iter().any(|o| &o.node.id == p)).then(|| (l.node.id.clone(), p.clone()))
            })
            .collect();
        // An agent whose parent chain leads back to itself becomes a root.
        let in_cycle = |start: &str| {
            let mut cur = start;
            for _ in 0..=parent_of.len() {
                match parent_of.get(cur) {
                    Some(p) if p == start => return true,
                    Some(p) => cur = p,
                    None => return false,
                }
            }
            false // leads into a loop that does not include `start`
        };
        let mut kids: HashMap<String, Vec<String>> = HashMap::new();
        let mut root_ids = Vec::new();
        for l in &loaded {
            match parent_of.get(&l.node.id) {
                Some(p) if !in_cycle(&l.node.id) => kids.entry(p.clone()).or_default().push(l.node.id.clone()),
                _ => root_ids.push(l.node.id.clone()),
            }
        }
        let mut nodes: HashMap<String, AgentNode> = loaded.into_iter().map(|l| (l.node.id.clone(), l.node)).collect();
        let mut roots: Vec<AgentNode> = root_ids.iter().filter_map(|id| attach(id, &mut nodes, &kids)).collect();
        sort_nodes(&mut roots);

        // Workflow groups come after plain agents.
        let mut wf_dirs: Vec<PathBuf> = fs::read_dir(subagents_dir.join("workflows"))
            .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect())
            .unwrap_or_default();
        wf_dirs.sort();
        let mut workflows: Vec<AgentNode> =
            wf_dirs.iter().filter_map(|d| self.workflow_node(d, session_live, now)).collect();
        sort_nodes(&mut workflows);
        roots.extend(workflows);

        let (mut total, mut finished) = (0, 0);
        for r in &roots {
            count(r, &mut total, &mut finished);
        }
        let tree = AgentTree { roots, total, finished };
        let seen = &self.seen;
        self.agents.retain(|p, _| seen.contains(p));
        tree
    }
}

pub fn build_tree(subagents_dir: &Path, session_live: bool, now: SystemTime) -> AgentTree {
    AgentCache::default().build(subagents_dir, session_live, now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    const NOW_S: u64 = 1_800_000_000;
    fn now() -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(NOW_S)
    }
    fn ago(secs: u64) -> SystemTime {
        now() - Duration::from_secs(secs)
    }

    fn call(id: &str, t: &str) -> String {
        format!(r#"{{"type":"assistant","timestamp":"{t}","message":{{"id":"m_{id}","model":"claude-sonnet-5-5","content":[{{"type":"tool_use","id":"{id}","name":"Bash","input":{{"command":"ls"}}}}]}}}}"#)
    }
    fn result(id: &str, t: &str) -> String {
        format!(r#"{{"type":"user","timestamp":"{t}","message":{{"role":"user","content":[{{"type":"tool_result","tool_use_id":"{id}","content":"x"}}]}}}}"#)
    }

    /// Writes agent-<id>.jsonl (+ meta when given) and sets the jsonl mtime.
    fn agent(dir: &Path, id: &str, meta: Option<&str>, lines: &[String], age: u64) {
        fs::create_dir_all(dir).unwrap();
        let p = dir.join(format!("agent-{id}.jsonl"));
        let mut f = File::create(&p).unwrap();
        for l in lines {
            writeln!(f, "{l}").unwrap();
        }
        f.set_modified(ago(age)).unwrap();
        drop(f);
        if let Some(m) = meta {
            fs::write(dir.join(format!("agent-{id}.meta.json")), m).unwrap();
        }
    }

    fn fixture() -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        let d = t.path();
        agent(d, "a1", Some(r#"{"agentType":"Explore","description":"one","spawnDepth":1,"finished":true}"#),
            &[call("t1", "2026-11-02T07:00:00Z"), result("t1", "2026-11-02T07:00:01Z")], 30);
        agent(d, "a2", Some(r#"{"agentType":"general-purpose","description":"two","spawnDepth":1,"model":"inherit"}"#),
            &[call("t2", "2026-11-02T07:01:00Z"), result("t2", "2026-11-02T07:01:01Z")], 30);
        agent(d, "a3", Some(r#"{"agentType":"Explore","spawnDepth":2,"parentAgentId":"a2"}"#),
            &[call("t3", "2026-11-02T07:02:00Z")], 30);
        let wf = d.join("workflows").join("wf_x");
        agent(&wf, "w1", Some(r#"{"agentType":"reviewer","workflowPhase":"Review"}"#),
            &[call("t4", "2026-11-02T07:03:00Z"), result("t4", "2026-11-02T07:03:01Z")], 30);
        agent(&wf, "w2", Some(r#"{"agentType":"verifier","workflowPhase":"Verify"}"#),
            &[call("t5", "2026-11-02T07:04:00Z"), result("t5", "2026-11-02T07:04:01Z")], 30);
        t
    }

    fn find<'a>(ns: &'a [AgentNode], id: &str) -> &'a AgentNode {
        ns.iter().find(|n| n.id == id).unwrap_or_else(|| panic!("no node {id}"))
    }

    #[test]
    fn depth2_agent_nests_under_parent() {
        let t = fixture();
        let tree = build_tree(t.path(), true, now());
        let a2 = find(&tree.roots, "a2");
        assert_eq!(a2.children.len(), 1);
        assert_eq!(a2.children[0].id, "a3");
        assert!(tree.roots.iter().all(|r| r.id != "a3"));
    }

    #[test]
    fn workflow_agents_grouped_by_phase() {
        let t = fixture();
        let tree = build_tree(t.path(), true, now());
        let wf = tree.roots.last().unwrap();
        assert_eq!(wf.kind, AgentKind::Workflow { id: "wf_x".into() });
        let names: Vec<_> = wf.children.iter().map(|p| p.kind.clone()).collect();
        assert_eq!(names, vec![AgentKind::Phase { name: "Review".into() }, AgentKind::Phase { name: "Verify".into() }]);
        assert_eq!(wf.children[0].children[0].id, "w1");
        assert_eq!(wf.children[1].children[0].id, "w2");
    }

    #[test]
    fn counts_include_workflow_agents() {
        let t = fixture();
        let tree = build_tree(t.path(), true, now());
        assert_eq!(tree.total, 5);
    }

    #[test]
    fn finished_true_is_finished() {
        let t = fixture();
        let tree = build_tree(t.path(), true, now());
        assert_eq!(find(&tree.roots, "a1").state, AgentState::Finished);
    }

    #[test]
    fn missing_finished_recent_mtime_is_running() {
        let t = tempfile::tempdir().unwrap();
        agent(t.path(), "r", Some("{}"), &[call("t", "2026-11-02T07:00:00Z"), result("t", "2026-11-02T07:00:01Z")], 30);
        let tree = build_tree(t.path(), true, now());
        assert_eq!(tree.roots[0].state, AgentState::Running);
    }

    #[test]
    fn missing_finished_stale_and_resolved_is_finished() {
        let t = tempfile::tempdir().unwrap();
        agent(t.path(), "r", Some("{}"), &[call("t", "2026-11-02T07:00:00Z"), result("t", "2026-11-02T07:00:01Z")], 300);
        let tree = build_tree(t.path(), true, now());
        assert_eq!(tree.roots[0].state, AgentState::Finished);
    }

    #[test]
    fn missing_finished_stale_but_open_tool_is_running() {
        let t = tempfile::tempdir().unwrap();
        agent(t.path(), "r", Some("{}"), &[call("t", "2026-11-02T07:00:00Z")], 300);
        let tree = build_tree(t.path(), true, now());
        assert_eq!(tree.roots[0].state, AgentState::Running);
    }

    #[test]
    fn closed_session_all_finished() {
        let t = fixture();
        let tree = build_tree(t.path(), false, now());
        assert_eq!(tree.finished, tree.total);
        assert_eq!(tree.total, 5);
    }

    #[test]
    fn model_comes_from_agent_transcript() {
        let t = fixture();
        let tree = build_tree(t.path(), true, now());
        assert_eq!(find(&tree.roots, "a2").stats.model.as_deref(), Some("claude-sonnet-5-5"));
    }

    #[test]
    fn bad_or_missing_meta_keeps_agent_and_orphan_meta_is_ignored() {
        let t = tempfile::tempdir().unwrap();
        agent(t.path(), "g", Some("{not json"), &[call("t", "2026-11-02T07:00:00Z")], 30);
        agent(t.path(), "n", None, &[], 30);
        fs::write(t.path().join("agent-orphan.meta.json"), "{}").unwrap();
        let tree = build_tree(t.path(), true, now());
        assert_eq!(tree.total, 2);
        assert_eq!(tree.roots[0].id, "g"); // has a time; "n" has none -> last
        assert_eq!(tree.roots[1].id, "n");
        assert!(tree.roots[0].agent_type.is_none());
    }

    #[test]
    fn parent_cycle_and_unknown_parent_do_not_lose_agents() {
        let t = tempfile::tempdir().unwrap();
        agent(t.path(), "x", Some(r#"{"parentAgentId":"y"}"#), &[], 30);
        agent(t.path(), "y", Some(r#"{"parentAgentId":"x"}"#), &[], 30);
        agent(t.path(), "z", Some(r#"{"parentAgentId":"gone"}"#), &[], 30);
        let tree = build_tree(t.path(), true, now());
        assert_eq!(tree.total, 3);
        assert_eq!(tree.roots.len(), 3);
    }

    #[test]
    fn missing_dir_gives_empty_tree() {
        let tree = build_tree(Path::new("Z:\\no\\such\\dir"), true, now());
        assert_eq!(tree.total, 0);
        assert!(tree.roots.is_empty());
    }

    fn append(dir: &Path, id: &str, line: &str) {
        let mut f = fs::OpenOptions::new().append(true).open(dir.join(format!("agent-{id}.jsonl"))).unwrap();
        writeln!(f, "{line}").unwrap();
    }

    #[test]
    fn cache_reads_only_appended_lines() {
        let t = fixture();
        let mut cache = AgentCache::default();
        let before = cache.build(t.path(), true, now());
        let n_a2 = find(&before.roots, "a2").stats.tool_call_count;
        let n_a1 = find(&before.roots, "a1").stats.tool_call_count;
        append(t.path(), "a2", &call("t9", "2026-11-02T07:05:00Z"));
        let after = cache.build(t.path(), true, now());
        assert_eq!(find(&after.roots, "a2").stats.tool_call_count, n_a2 + 1);
        assert_eq!(find(&after.roots, "a1").stats.tool_call_count, n_a1);
        // an unchanged third build must not double anything
        let again = cache.build(t.path(), true, now());
        assert_eq!(find(&again.roots, "a2").stats.tool_call_count, n_a2 + 1);
    }

    #[test]
    fn cache_evicts_removed_agents() {
        let t = fixture();
        let mut cache = AgentCache::default();
        assert_eq!(cache.build(t.path(), true, now()).total, 5);
        assert_eq!(cache.agents.len(), 5);
        fs::remove_file(t.path().join("agent-a1.jsonl")).unwrap();
        let tree = cache.build(t.path(), true, now());
        assert_eq!(tree.total, 4);
        assert_eq!(cache.agents.len(), 4);
    }

    #[test]
    fn cache_rebuilds_after_shrink() {
        let t = tempfile::tempdir().unwrap();
        let two = [call("t1", "2026-11-02T07:00:00Z"), result("t1", "2026-11-02T07:00:01Z"), call("t2", "2026-11-02T07:00:02Z")];
        agent(t.path(), "r", Some("{}"), &two, 30);
        let mut cache = AgentCache::default();
        assert_eq!(cache.build(t.path(), true, now()).roots[0].stats.tool_call_count, 2);
        agent(t.path(), "r", Some("{}"), &[call("t7", "2026-11-02T07:00:00Z")], 30);
        let tree = cache.build(t.path(), true, now());
        assert_eq!(tree.roots[0].stats.tool_call_count, 1);
    }

    #[test]
    fn serializes_camel_case_tags() {
        let t = fixture();
        let tree = build_tree(t.path(), true, now());
        let v = serde_json::to_value(&tree).unwrap();
        let wf = v["roots"].as_array().unwrap().last().unwrap();
        assert_eq!(wf["kind"]["kind"], "workflow");
        assert_eq!(wf["kind"]["id"], "wf_x");
        assert_eq!(v["roots"][0]["state"], "finished");
        assert!(v["roots"][0].get("agentType").is_some());
        assert_eq!(v["total"], 5);
    }
}
