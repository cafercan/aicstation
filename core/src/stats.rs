//! Incremental per-session statistics built from transcript entries.
use crate::model::{ToolCall, ToolState, Usage};
use crate::pricing;
use crate::transcript::Entry;
use chrono::{DateTime, Local, NaiveDate, Utc};
use std::collections::{BTreeMap, HashSet, VecDeque};

/// Maximum number of tool calls kept in memory.
pub const MAX_TOOL_CALLS: usize = 50;

#[derive(Debug, Clone, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionStats {
    pub title: Option<String>,
    pub cwd: Option<String>,
    pub git_branch: Option<String>,
    pub model: Option<String>,
    pub requested_model: Option<String>,
    pub tokens: Usage,
    pub last_usage: Option<Usage>,
    pub cost_usd: f64,
    pub cost_by_day: BTreeMap<NaiveDate, f64>,
    pub tool_calls: VecDeque<ToolCall>,
    pub tool_call_count: u64,
    pub first_at: Option<DateTime<Utc>>,
    pub last_at: Option<DateTime<Utc>>,
    #[serde(skip)]
    seen_ids: HashSet<String>,
}

impl SessionStats {
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
                let first_time = message_id.is_empty() || self.seen_ids.insert(message_id.clone());
                if let Some(u) = usage {
                    self.last_usage = Some(*u);
                    if first_time {
                        self.tokens.input += u.input;
                        self.tokens.output += u.output;
                        self.tokens.cache_read += u.cache_read;
                        self.tokens.cache_creation += u.cache_creation;
                        self.tokens.cache_creation_1h += u.cache_creation_1h;
                        let m = model.as_deref().or(requested_model.as_deref());
                        if let Some(c) = m.and_then(|m| pricing::cost_usd(m, u)) {
                            self.cost_usd += c;
                            let day = at.with_timezone(&Local).date_naive();
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
                if let Some(c) = self.tool_calls.iter_mut().find(|c| &c.id == tool_use_id) {
                    c.state = if *is_error { ToolState::Error } else { ToolState::Ok };
                }
            }
            Entry::Title(t) => self.title = Some(t.clone()),
            Entry::Meta { cwd, git_branch, at } => {
                if cwd.is_some() {
                    self.cwd = cwd.clone();
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

    /// (input + cache_read + cache_creation) of the last usage / context window * 100.
    pub fn context_pct(&self) -> Option<f32> {
        let u = self.last_usage?;
        let name = self.requested_model.as_deref().or(self.model.as_deref())?;
        let win = pricing::context_window(name)?;
        let used = u.input + u.cache_read + u.cache_creation;
        Some((used as f64 / win as f64 * 100.0) as f32)
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
        ToolCall { id: id.into(), name: "Bash".into(), target: "x".into(), at: at(10, 0), state: ToolState::Running }
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
    fn context_pct_uses_requested_model_1m() {
        let mut s = SessionStats::default();
        s.apply(&asst("m", "claude-opus-5-5", Some("claude-opus-5-5[1m]"), usage(100_000, 5, 50_000, 50_000), vec![], at(10, 0)));
        assert_eq!(s.context_pct(), Some(20.0));
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
}
