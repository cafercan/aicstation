//! Incremental per-session statistics built from transcript entries.
use crate::model::{ChangedFile, ToolCall, ToolState, Usage};
use crate::pricing;
use crate::transcript::{Entry, QueueOp, Skill};
use chrono::{DateTime, Local, NaiveDate, Utc};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

/// Maximum number of tool calls kept in memory.
pub const MAX_TOOL_CALLS: usize = 50;
/// Assistant message ids remembered for usage dedupe. Lines sharing an id are
/// adjacent in a transcript, so a recent window is enough.
pub const MAX_SEEN_IDS: usize = 1024;
/// Changed files remembered per transcript; past it the least recently
/// changed one is forgotten.
pub const MAX_CHANGED_FILES: usize = 500;
/// Distinct top-level cwds kept, in first-seen order, to find the resume folder.
pub const MAX_CWDS: usize = 32;
/// Running background tasks and queued prompts kept, each; past it the oldest goes.
pub const MAX_LIVE_ITEMS: usize = 64;

#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStats {
    pub title: Option<String>,
    /// Last recorded top-level cwd (display).
    pub cwd: Option<String>,
    /// First recorded top-level cwd.
    pub first_cwd: Option<String>,
    /// Distinct top-level cwds in first-seen order (at most [`MAX_CWDS`]);
    /// see [`SessionStats::resume_cwd`].
    #[serde(skip)]
    pub cwds: Vec<String>,
    pub git_branch: Option<String>,
    pub model: Option<String>,
    pub requested_model: Option<String>,
    pub tokens: Usage,
    pub last_usage: Option<Usage>,
    pub cost_usd: f64,
    pub cost_by_day: BTreeMap<NaiveDate, f64>,
    /// Tokens (all four kinds) per local day; counted for every model, priced or not.
    pub tokens_by_day: BTreeMap<NaiveDate, u64>,
    pub tool_calls: VecDeque<ToolCall>,
    pub tool_call_count: u64,
    pub first_at: Option<DateTime<Utc>>,
    pub last_at: Option<DateTime<Utc>>,
    /// Files changed by successful editing tool calls, by full path. Sent to
    /// the UI only as the merged list of a session detail.
    #[serde(skip)]
    pub changed_files: HashMap<String, ChangedFile>,
    /// Permission mode last written ("auto", "plan", …).
    pub permission_mode: Option<String>,
    /// Output tokens of the current turn (since the last prompt).
    pub turn_output_tokens: u64,
    /// Background tasks started and not seen finished: (id, start), in start order.
    #[serde(skip)]
    background: VecDeque<(String, DateTime<Utc>)>,
    /// Queued prompts and notices: (content, enqueue time), oldest first.
    #[serde(skip)]
    queue: VecDeque<(Option<String>, DateTime<Utc>)>,
    /// The session was seen idle at this time: nothing started or queued up
    /// to it can still be running or waiting.
    #[serde(skip)]
    settled_before: Option<DateTime<Utc>>,
    /// Skills listed for the session: name → description.
    #[serde(skip)]
    skills: BTreeMap<String, String>,
    #[serde(skip)]
    seen_ids: HashSet<String>,
    /// Insertion order of `seen_ids`; the oldest id is forgotten first.
    #[serde(skip)]
    seen_order: VecDeque<String>,
}

impl SessionStats {
    /// The folder `claude --resume` must run in: the first recorded cwd whose
    /// Claude Code project-folder name is `project_dir` (the folder the
    /// transcript sits in). Usually the first cwd; a session that moved into a
    /// worktree (`claude -w`) is stored under the worktree's folder, so its
    /// first cwd (the main repository) does not match. Falls back to the first
    /// cwd, which the resume check then refuses with a clear error.
    pub fn resume_cwd(&self, project_dir: &str) -> Option<String> {
        self.cwds
            .iter()
            .find(|c| crate::locate::cwd_matches_project_dir(c, project_dir))
            .cloned()
            .or_else(|| self.first_cwd.clone())
    }

    /// Background tasks started and not seen finished.
    pub fn background_shells(&self) -> u32 {
        self.background.len() as u32
    }

    /// Skills the session can run, by name.
    pub fn skills(&self) -> Vec<Skill> {
        self.skills.iter().map(|(n, d)| Skill { name: n.clone(), description: d.clone() }).collect()
    }

    /// Queued prompts of the user (queued task notices are not counted).
    pub fn queued_messages(&self) -> u32 {
        let notice = |c: &Option<String>| c.as_deref().is_some_and(|c| c.trim_start().starts_with("<task-notification>"));
        self.queue.iter().filter(|(c, _)| !notice(c)).count() as u32
    }

    /// The session was seen idle at `since`: no background task or queued
    /// prompt from up to then is still there, also one whose line is read later.
    pub fn settle_idle(&mut self, since: DateTime<Utc>) {
        self.background.retain(|(_, at)| *at > since);
        self.queue.retain(|(_, at)| *at > since);
        if self.settled_before.is_none_or(|s| since > s) {
            self.settled_before = Some(since);
        }
    }

    fn settled(&self, at: DateTime<Utc>) -> bool {
        self.settled_before.is_some_and(|s| at <= s)
    }

    fn apply_queue(&mut self, op: QueueOp, content: &Option<String>, at: DateTime<Utc>) {
        match op {
            QueueOp::Enqueue => {
                if self.settled(at) {
                    return;
                }
                self.queue.push_back((content.clone(), at));
                if self.queue.len() > MAX_LIVE_ITEMS {
                    self.queue.pop_front();
                }
            }
            QueueOp::Dequeue => {
                self.queue.pop_front();
            }
            QueueOp::Remove => {
                if let Some(i) = self.queue.iter().position(|(c, _)| c == content) {
                    self.queue.remove(i);
                }
            }
            QueueOp::PopAll => self.queue.clear(),
        }
    }

    /// True the first time `id` is seen within the recent window.
    fn first_sight(&mut self, id: &str) -> bool {
        if self.seen_ids.contains(id) {
            return false;
        }
        self.seen_ids.insert(id.to_string());
        self.seen_order.push_back(id.to_string());
        while self.seen_order.len() > MAX_SEEN_IDS {
            if let Some(old) = self.seen_order.pop_front() {
                self.seen_ids.remove(&old);
            }
        }
        true
    }

    fn record_change(&mut self, path: &str, at: DateTime<Utc>) {
        if let Some(f) = self.changed_files.get_mut(path) {
            f.edits = f.edits.saturating_add(1);
            f.last_at = f.last_at.max(at);
            return;
        }
        if self.changed_files.len() >= MAX_CHANGED_FILES {
            let oldest = self.changed_files.values().min_by_key(|f| f.last_at).map(|f| f.path.clone());
            if let Some(p) = oldest {
                self.changed_files.remove(&p);
            }
        }
        self.changed_files.insert(path.to_string(), ChangedFile { path: path.to_string(), edits: 1, last_at: at });
    }

    fn touch(&mut self, at: DateTime<Utc>) {
        if self.first_at.is_none_or(|f| at < f) {
            self.first_at = Some(at);
        }
        if self.last_at.is_none_or(|l| at > l) {
            self.last_at = Some(at);
        }
    }

    pub fn apply(&mut self, e: &Entry) {
        match e {
            Entry::Assistant { message_id, model, requested_model, usage, tool_uses, at } => {
                self.touch(*at);
                if model.is_some() {
                    self.model = model.clone();
                }
                if requested_model.is_some() {
                    self.requested_model = requested_model.clone();
                }
                // Empty ids are never a dedupe key: such lines are always counted.
                let first_time = message_id.is_empty() || self.first_sight(message_id);
                if let Some(u) = usage {
                    self.last_usage = Some(*u);
                    if first_time {
                        self.tokens.input += u.input;
                        self.tokens.output += u.output;
                        self.tokens.cache_read += u.cache_read;
                        self.tokens.cache_creation += u.cache_creation;
                        self.tokens.cache_creation_1h += u.cache_creation_1h;
                        self.turn_output_tokens += u.output;
                        let day = at.with_timezone(&Local).date_naive();
                        let day_tokens = self.tokens_by_day.entry(day).or_insert(0);
                        *day_tokens = day_tokens.saturating_add(u.total());
                        let m = model.as_deref().or(requested_model.as_deref());
                        if let Some(c) = m.and_then(|m| pricing::cost_usd(m, u)) {
                            self.cost_usd += c;
                            *self.cost_by_day.entry(day).or_insert(0.0) += c;
                        }
                    }
                }
                // Each content block is its own line sharing one message id,
                // so tool calls are deduped by tool id, not message id.
                for t in tool_uses {
                    if self.tool_calls.iter().any(|c| c.id == t.id) {
                        continue;
                    }
                    self.tool_call_count += 1;
                    self.tool_calls.push_back(t.clone());
                    while self.tool_calls.len() > MAX_TOOL_CALLS {
                        self.tool_calls.pop_front();
                    }
                }
            }
            Entry::ToolResult { tool_use_id, is_error, at } => {
                self.touch(*at);
                let mut changed = None;
                if let Some(c) = self.tool_calls.iter_mut().find(|c| &c.id == tool_use_id) {
                    // Only the first result of a call counts as a change.
                    if c.state == ToolState::Running && !is_error {
                        changed = c.edit_path.clone();
                    }
                    c.state = if *is_error { ToolState::Error } else { ToolState::Ok };
                }
                if let Some(p) = changed {
                    self.record_change(&p, *at);
                }
            }
            Entry::Title(t) => self.title = Some(t.clone()),
            Entry::PermissionMode(m) => self.permission_mode = Some(m.clone()),
            Entry::BackgroundStarted { id, at } => {
                if !self.settled(*at) && !self.background.iter().any(|(i, _)| i == id) {
                    self.background.push_back((id.clone(), *at));
                    if self.background.len() > MAX_LIVE_ITEMS {
                        self.background.pop_front();
                    }
                }
            }
            Entry::TaskFinished(id) => self.background.retain(|(i, _)| i != id),
            Entry::UserPrompt { .. } => self.turn_output_tokens = 0,
            Entry::Queue { op, content, at } => self.apply_queue(*op, content, *at),
            Entry::SkillListing { initial, skills } => {
                if *initial {
                    self.skills.clear();
                }
                for s in skills {
                    self.skills.insert(s.name.clone(), s.description.clone());
                }
            }
            Entry::Meta { cwd, git_branch, at } => {
                if let Some(c) = cwd {
                    if self.first_cwd.is_none() {
                        self.first_cwd = Some(c.clone());
                    }
                    if self.cwds.len() < MAX_CWDS && !self.cwds.contains(c) {
                        self.cwds.push(c.clone());
                    }
                    self.cwd = Some(c.clone());
                }
                if git_branch.is_some() {
                    self.git_branch = git_branch.clone();
                }
                if let Some(at) = at {
                    self.touch(*at);
                }
            }
        }
    }

    /// (ok, error) counts over the kept recent tool calls; running calls are ignored.
    pub fn tool_outcomes(&self) -> (u32, u32) {
        self.tool_calls.iter().fold((0, 0), |(ok, err), c| match c.state {
            ToolState::Ok => (ok + 1, err),
            ToolState::Error => (ok, err + 1),
            ToolState::Running => (ok, err),
        })
    }

    /// (input + cache_read + cache_creation) of the last usage / context window * 100,
    /// clamped to 0..=100. None when the window is unknown.
    pub fn context_pct(&self) -> Option<f32> {
        let u = self.last_usage?;
        let name = self.requested_model.as_deref().or(self.model.as_deref())?;
        let win = pricing::context_window(name)?;
        let used = u.input + u.cache_read + u.cache_creation;
        Some((used as f64 / win as f64 * 100.0).min(100.0) as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn at(h: u32, m: u32) -> DateTime<Utc> {
        Local.with_ymd_and_hms(2026, 6, 10, h, m, 0).unwrap().to_utc()
    }
    fn usage(i: u64, o: u64, cr: u64, cc: u64) -> Usage {
        Usage { input: i, output: o, cache_read: cr, cache_creation: cc, cache_creation_1h: 0 }
    }
    fn asst(id: &str, model: &str, req: Option<&str>, u: Usage, tools: Vec<ToolCall>, t: DateTime<Utc>) -> Entry {
        Entry::Assistant {
            message_id: id.into(),
            model: Some(model.into()),
            requested_model: req.map(Into::into),
            usage: Some(u),
            tool_uses: tools,
            at: t,
        }
    }
    fn tool(id: &str) -> ToolCall {
        ToolCall { id: id.into(), name: "Bash".into(), target: "x".into(), at: at(10, 0), state: ToolState::Running, edit_path: None }
    }
    fn edit(id: &str, path: &str) -> ToolCall {
        ToolCall { name: "Edit".into(), edit_path: Some(path.into()), ..tool(id) }
    }
    fn result(id: &str, is_error: bool, t: DateTime<Utc>) -> Entry {
        Entry::ToolResult { tool_use_id: id.into(), is_error, at: t }
    }

    #[test]
    fn changed_files_count_successful_edits_once() {
        let mut s = SessionStats::default();
        let calls = vec![edit("A", "/w/a.rs"), edit("B", "/w/a.rs"), edit("C", "/w/b.rs"), tool("D")];
        s.apply(&asst("m", "claude-opus-5-5", None, usage(1, 1, 0, 0), calls, at(10, 0)));
        assert!(s.changed_files.is_empty(), "nothing changed before the results");
        s.apply(&result("A", false, at(10, 1)));
        s.apply(&result("A", false, at(10, 9))); // a repeated result is not a second edit
        s.apply(&result("B", false, at(10, 2)));
        s.apply(&result("C", true, at(10, 3))); // a failed edit changed nothing
        s.apply(&result("D", false, at(10, 4))); // Bash: not an editing tool
        assert_eq!(s.changed_files.len(), 1);
        let a = &s.changed_files["/w/a.rs"];
        assert_eq!((a.edits, a.last_at), (2, at(10, 2)));
    }

    #[test]
    fn changed_files_are_bounded() {
        let mut s = SessionStats::default();
        for i in 0..(MAX_CHANGED_FILES + 5) {
            let id = format!("t{i}");
            let t = at(10, 0) + chrono::Duration::seconds(i as i64);
            s.apply(&asst(&format!("m{i}"), "claude-opus-5-5", None, usage(1, 1, 0, 0), vec![edit(&id, &format!("/w/{i}"))], t));
            s.apply(&result(&id, false, t));
        }
        assert_eq!(s.changed_files.len(), MAX_CHANGED_FILES);
        assert!(!s.changed_files.contains_key("/w/0"), "the oldest is forgotten first");
        assert!(s.changed_files.contains_key(&format!("/w/{}", MAX_CHANGED_FILES + 4)));
    }

    #[test]
    fn usage_is_deduplicated_by_message_id() {
        let mut s = SessionStats::default();
        let u = usage(10, 20, 30, 40);
        for _ in 0..3 {
            s.apply(&asst("m1", "claude-opus-5-5", None, u, vec![], at(10, 0)));
        }
        assert_eq!(s.tokens, u);
        // empty ids are not a dedupe key
        s.apply(&asst("", "claude-opus-5-5", None, u, vec![], at(10, 1)));
        s.apply(&asst("", "claude-opus-5-5", None, u, vec![], at(10, 2)));
        assert_eq!(s.tokens.input, 30);
    }

    #[test]
    fn seen_ids_are_bounded_to_a_recent_window() {
        let mut s = SessionStats::default();
        let u = usage(1, 0, 0, 0);
        for i in 0..(MAX_SEEN_IDS * 3) {
            s.apply(&asst(&format!("m{i}"), "claude-opus-5-5", None, u, vec![], at(10, 0)));
        }
        assert!(s.seen_ids.len() <= MAX_SEEN_IDS);
        assert_eq!(s.tokens.input, (MAX_SEEN_IDS * 3) as u64);
        // A repeat of a recent id is still deduplicated.
        let last = format!("m{}", MAX_SEEN_IDS * 3 - 1);
        s.apply(&asst(&last, "claude-opus-5-5", None, u, vec![], at(10, 0)));
        assert_eq!(s.tokens.input, (MAX_SEEN_IDS * 3) as u64);
    }

    #[test]
    fn first_cwd_is_kept_while_cwd_follows_the_last_line() {
        let mut s = SessionStats::default();
        let meta = |c: &str| Entry::Meta { cwd: Some(c.into()), git_branch: None, at: None };
        s.apply(&Entry::Meta { cwd: None, git_branch: Some("main".into()), at: None });
        assert_eq!(s.first_cwd, None);
        s.apply(&meta(r"C:\demo\proj"));
        s.apply(&meta(r"C:\demo\proj\other"));
        assert_eq!(s.first_cwd.as_deref(), Some(r"C:\demo\proj"));
        assert_eq!(s.cwd.as_deref(), Some(r"C:\demo\proj\other"));
    }

    #[test]
    fn resume_cwd_is_the_first_cwd_that_matches_the_transcript_folder() {
        // `claude -w modul-testi` started in the repository, then moved into the
        // worktree; Claude Code stores the transcript under the worktree's folder.
        let mut s = SessionStats::default();
        let meta = |c: &str| Entry::Meta { cwd: Some(c.into()), git_branch: None, at: None };
        s.apply(&meta(r"C:\Users\u\IAR\GDP"));
        s.apply(&meta("/c/Users/u/IAR/GDP/.claude/worktrees/modul-testi"));
        s.apply(&meta(r"C:\Users\u\IAR\GDP\.claude\worktrees\modul-testi"));
        s.apply(&meta(r"C:\Users\u\IAR\GDP\.claude\worktrees\modul-testi\Docs"));
        let wt = "C--Users-u-IAR-GDP--claude-worktrees-modul-testi";
        assert_eq!(s.resume_cwd(wt).as_deref(), Some(r"C:\Users\u\IAR\GDP\.claude\worktrees\modul-testi"));
        // A plain session keeps its first cwd.
        assert_eq!(s.resume_cwd("C--Users-u-IAR-GDP").as_deref(), Some(r"C:\Users\u\IAR\GDP"));
        // No match: the first cwd, which the resume check refuses.
        assert_eq!(s.resume_cwd("C--elsewhere").as_deref(), Some(r"C:\Users\u\IAR\GDP"));
    }

    #[test]
    fn cwds_are_distinct_and_capped() {
        let mut s = SessionStats::default();
        for i in 0..(MAX_CWDS + 10) {
            s.apply(&Entry::Meta { cwd: Some(format!(r"C:\d\{i}")), git_branch: None, at: None });
            s.apply(&Entry::Meta { cwd: Some(r"C:\d\0".into()), git_branch: None, at: None });
        }
        assert_eq!(s.cwds.len(), MAX_CWDS);
        assert_eq!(s.cwds[0], r"C:\d\0");
    }

    #[test]
    fn context_pct_uses_requested_model_1m() {
        let mut s = SessionStats::default();
        s.apply(&asst("m", "claude-opus-5-5", Some("claude-opus-5-5[1m]"), usage(100_000, 5, 50_000, 50_000), vec![], at(10, 0)));
        assert_eq!(s.context_pct(), Some(20.0));
    }

    #[test]
    fn context_pct_uses_family_window_without_suffix() {
        let mut s = SessionStats::default();
        s.apply(&asst("m", "claude-opus-5-5", None, usage(7_000, 5, 400_000, 100_000), vec![], at(10, 0)));
        let pct = s.context_pct().unwrap();
        assert!((pct - 50.7).abs() < 0.01, "{pct}");
    }

    #[test]
    fn context_pct_is_clamped_to_100() {
        let mut s = SessionStats::default();
        s.apply(&asst("m", "claude-haiku-4-5", None, usage(250_000, 0, 0, 0), vec![], at(10, 0)));
        assert_eq!(s.context_pct(), Some(100.0));
    }

    #[test]
    fn tool_outcomes_count_finished_calls_only() {
        let mut s = SessionStats::default();
        s.apply(&asst("m", "claude-opus-5-5", None, usage(1, 1, 0, 0), vec![tool("A"), tool("B"), tool("C"), tool("D")], at(10, 0)));
        assert_eq!(s.tool_outcomes(), (0, 0));
        s.apply(&Entry::ToolResult { tool_use_id: "A".into(), is_error: false, at: at(10, 1) });
        s.apply(&Entry::ToolResult { tool_use_id: "B".into(), is_error: false, at: at(10, 1) });
        s.apply(&Entry::ToolResult { tool_use_id: "C".into(), is_error: true, at: at(10, 1) });
        assert_eq!(s.tool_outcomes(), (2, 1));
    }

    #[test]
    fn context_pct_unknown_model_is_none() {
        let mut s = SessionStats::default();
        assert_eq!(s.context_pct(), None);
        s.apply(&asst("m", "mystery", None, usage(1, 1, 1, 1), vec![], at(10, 0)));
        assert_eq!(s.context_pct(), None);
    }

    #[test]
    fn tool_result_updates_matching_call() {
        let mut s = SessionStats::default();
        s.apply(&asst("m", "claude-opus-5-5", None, usage(1, 1, 0, 0), vec![tool("A")], at(10, 0)));
        assert_eq!(s.tool_calls[0].state, ToolState::Running);
        s.apply(&Entry::ToolResult { tool_use_id: "A".into(), is_error: true, at: at(10, 1) });
        assert_eq!(s.tool_calls[0].state, ToolState::Error);
    }

    #[test]
    fn tool_call_history_is_capped_at_50() {
        let mut s = SessionStats::default();
        for i in 1..=120 {
            s.apply(&asst(&format!("m{i}"), "claude-opus-5-5", None, usage(1, 1, 0, 0), vec![tool(&format!("t{i}"))], at(10, 0)));
        }
        assert_eq!(s.tool_calls.len(), 50);
        assert_eq!(s.tool_calls.front().unwrap().id, "t71");
        assert_eq!(s.tool_call_count, 120);
    }

    #[test]
    fn cost_split_by_local_day() {
        let mut s = SessionStats::default();
        let u = usage(1_000_000, 0, 0, 0);
        s.apply(&asst("a", "claude-opus-5-5", None, u, vec![], at(23, 50)));
        let next = Local.with_ymd_and_hms(2026, 6, 11, 0, 10, 0).unwrap().to_utc();
        s.apply(&asst("b", "claude-opus-5-5", None, u, vec![], next));
        assert_eq!(s.cost_by_day.len(), 2);
        assert!((s.cost_usd - 8.0).abs() < 1e-9);
    }

    #[test]
    fn tokens_by_day_counts_all_kinds_even_for_unpriced_models() {
        let mut s = SessionStats::default();
        s.apply(&asst("a", "mystery-model", None, usage(100, 20, 3000, 400), vec![], at(23, 50)));
        // Same message id again: deduped, like the totals.
        s.apply(&asst("a", "mystery-model", None, usage(100, 20, 3000, 400), vec![], at(23, 50)));
        let next = Local.with_ymd_and_hms(2026, 6, 11, 0, 10, 0).unwrap().to_utc();
        s.apply(&asst("b", "mystery-model", None, usage(1, 1, 1, 1), vec![], next));
        assert_eq!(s.cost_usd, 0.0, "unknown model has no cost");
        assert_eq!(s.tokens_by_day.len(), 2);
        assert_eq!(s.tokens_by_day.values().sum::<u64>(), 3520 + 4);
        assert_eq!(s.tokens.total(), 3524);
    }

    fn ts(s: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 11, 19, 7, 0, s).unwrap()
    }
    fn out(id: &str, o: u64, t: DateTime<Utc>) -> Entry {
        asst(id, "claude-opus-5-5", None, usage(0, o, 0, 0), vec![], t)
    }
    fn queue(op: QueueOp, c: Option<&str>, s: u32) -> Entry {
        Entry::Queue { op, content: c.map(Into::into), at: ts(s) }
    }

    #[test]
    fn background_tasks_start_and_finish() {
        let mut st = SessionStats::default();
        st.apply(&Entry::BackgroundStarted { id: "a".into(), at: ts(1) });
        st.apply(&Entry::BackgroundStarted { id: "b".into(), at: ts(2) });
        st.apply(&Entry::TaskFinished("a".into()));
        st.apply(&Entry::TaskFinished("zz".into()));
        assert_eq!(st.background_shells(), 1);
    }

    #[test]
    fn background_tasks_are_capped() {
        let mut st = SessionStats::default();
        for i in 0..70 {
            st.apply(&Entry::BackgroundStarted { id: format!("t{i}"), at: ts(1) });
        }
        assert_eq!(st.background_shells(), 64);
    }

    #[test]
    fn settle_idle_drops_older_starts_also_read_later() {
        let mut st = SessionStats::default();
        st.apply(&Entry::BackgroundStarted { id: "old".into(), at: ts(1) });
        st.settle_idle(ts(5));
        assert_eq!(st.background_shells(), 0);
        st.apply(&Entry::BackgroundStarted { id: "late".into(), at: ts(4) });
        assert_eq!(st.background_shells(), 0, "written before idle, read after");
        st.apply(&Entry::BackgroundStarted { id: "new".into(), at: ts(9) });
        assert_eq!(st.background_shells(), 1);
    }

    #[test]
    fn queue_counts_messages_not_notifications() {
        let mut st = SessionStats::default();
        st.apply(&queue(QueueOp::Enqueue, Some("<task-notification>x</task-notification>"), 1));
        st.apply(&queue(QueueOp::Enqueue, Some("hi"), 2));
        st.apply(&queue(QueueOp::Enqueue, Some("again"), 3));
        assert_eq!(st.queued_messages(), 2);
        st.apply(&queue(QueueOp::Dequeue, None, 4)); // takes the notification (oldest)
        assert_eq!(st.queued_messages(), 2);
        st.apply(&queue(QueueOp::Remove, Some("hi"), 5));
        assert_eq!(st.queued_messages(), 1);
        st.apply(&queue(QueueOp::PopAll, Some("again"), 6));
        assert_eq!(st.queued_messages(), 0);
    }

    #[test]
    fn queue_remove_unknown_is_ignored() {
        let mut st = SessionStats::default();
        st.apply(&queue(QueueOp::Remove, Some("x"), 1));
        st.apply(&queue(QueueOp::Dequeue, None, 2));
        assert_eq!(st.queued_messages(), 0);
    }

    #[test]
    fn settle_idle_empties_the_queue() {
        let mut st = SessionStats::default();
        st.apply(&queue(QueueOp::Enqueue, Some("hi"), 1));
        st.settle_idle(ts(3));
        st.apply(&queue(QueueOp::Enqueue, Some("late"), 2));
        assert_eq!(st.queued_messages(), 0);
    }

    #[test]
    fn turn_tokens_reset_by_a_prompt_only() {
        let mut st = SessionStats::default();
        st.apply(&out("m1", 100, ts(1)));
        st.apply(&Entry::UserPrompt { at: ts(2) });
        st.apply(&out("m2", 30, ts(3)));
        st.apply(&result("t", false, ts(4)));
        st.apply(&Entry::TaskFinished("b".into()));
        st.apply(&out("m3", 12, ts(5)));
        st.apply(&out("m3", 12, ts(5))); // same id: once
        assert_eq!(st.turn_output_tokens, 42);
    }

    #[test]
    fn skill_listings_join_and_initial_starts_over() {
        let sk = |n: &str| Skill { name: n.into(), description: format!("{n} does it") };
        let mut st = SessionStats::default();
        st.apply(&Entry::SkillListing { initial: true, skills: vec![sk("b"), sk("a")] });
        st.apply(&Entry::SkillListing { initial: false, skills: vec![sk("c")] });
        let names = |st: &SessionStats| st.skills().into_iter().map(|s| s.name).collect::<Vec<_>>();
        assert_eq!(names(&st), ["a", "b", "c"]);
        st.apply(&Entry::SkillListing { initial: true, skills: vec![sk("z")] });
        assert_eq!(names(&st), ["z"]);
    }

    #[test]
    fn permission_mode_is_the_last_seen() {
        let mut st = SessionStats::default();
        st.apply(&Entry::PermissionMode("auto".into()));
        st.apply(&Entry::PermissionMode("plan".into()));
        assert_eq!(st.permission_mode.as_deref(), Some("plan"));
    }
}
