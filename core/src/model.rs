use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Longest display string (session name, title, branch, model, agent type,
/// description...) kept from Claude Code's files, in characters. Applied at
/// ingestion so an oversized value is never stored or sent to the UI.
pub const MAX_DISPLAY: usize = 256;

/// `s` cut to [`MAX_DISPLAY`] characters.
pub fn cap_display(s: &str) -> String {
    s.chars().take(MAX_DISPLAY).collect()
}

/// A path is never cut (a shortened path names a different folder): one over
/// [`MAX_DISPLAY`] characters is dropped.
pub fn cap_path(s: &str) -> Option<String> {
    (s.chars().nth(MAX_DISPLAY).is_none()).then(|| s.to_string())
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
    /// Total cache-write tokens (5-minute + 1-hour).
    pub cache_creation: u64,
    /// Part of `cache_creation` written with the 1-hour TTL.
    pub cache_creation_1h: u64,
}

impl Usage {
    /// All four token kinds: input + output + cache read + cache write.
    /// (`cache_creation_1h` is part of `cache_creation`, not added again.)
    pub fn total(&self) -> u64 {
        self.input
            .saturating_add(self.output)
            .saturating_add(self.cache_read)
            .saturating_add(self.cache_creation)
    }

    /// Adds `o` into `self` (saturating).
    pub fn add(&mut self, o: &Usage) {
        self.input = self.input.saturating_add(o.input);
        self.output = self.output.saturating_add(o.output);
        self.cache_read = self.cache_read.saturating_add(o.cache_read);
        self.cache_creation = self.cache_creation.saturating_add(o.cache_creation);
        self.cache_creation_1h = self.cache_creation_1h.saturating_add(o.cache_creation_1h);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolState {
    Running,
    Ok,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub target: String,
    pub at: DateTime<Utc>,
    pub state: ToolState,
    /// Full path of the file an editing tool (see [`EDIT_TOOLS`]) changes.
    /// Kept for the changed-files list; not sent to the UI.
    #[serde(skip)]
    pub edit_path: Option<String>,
}

/// Tools that change the file named in their input.
pub const EDIT_TOOLS: [&str; 4] = ["Edit", "Write", "MultiEdit", "NotebookEdit"];

/// A file a session changed: successful editing tool calls on one path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFile {
    pub path: String,
    /// Successful editing tool calls on the file.
    pub edits: u32,
    pub last_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_serializes_camel_case() {
        let v = serde_json::to_value(Usage::default()).unwrap();
        let mut keys: Vec<&str> = v.as_object().unwrap().keys().map(|k| k.as_str()).collect();
        keys.sort();
        assert_eq!(keys, ["cacheCreation", "cacheCreation1h", "cacheRead", "input", "output"]);
    }
}
