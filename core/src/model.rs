use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
