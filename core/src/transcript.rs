//! JSONL transcript line parsing and an incremental, read-only reader.
use crate::model::{cap_display, cap_path, ToolCall, ToolState, Usage};
use chrono::{DateTime, Utc};
use serde_json::Value;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::ops::ControlFlow;
use std::path::PathBuf;

/// Read buffer size used by [`TranscriptReader`].
pub const READ_BUF: usize = 64 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Assistant {
        message_id: String,
        model: Option<String>,
        requested_model: Option<String>,
        usage: Option<Usage>,
        tool_uses: Vec<ToolCall>,
        at: DateTime<Utc>,
    },
    ToolResult {
        tool_use_id: String,
        is_error: bool,
        at: DateTime<Utc>,
    },
    Title(String),
    Meta {
        cwd: Option<String>,
        git_branch: Option<String>,
        at: Option<DateTime<Utc>>,
    },
}

fn str_of<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
    v.get(key).and_then(Value::as_str)
}

fn parse_time(v: &Value) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(str_of(v, "timestamp")?)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

fn truncate(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

fn file_name(p: &str) -> String {
    p.rsplit(['\\', '/']).next().unwrap_or(p).to_string()
}

fn tool_target(name: &str, input: &Value) -> String {
    let get = |k: &str| input.get(k).and_then(Value::as_str);
    match name {
        "Read" | "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => {
            if let Some(p) = get("file_path").or_else(|| get("notebook_path")) {
                return file_name(p);
            }
        }
        "Bash" => {
            if let Some(c) = get("command") {
                return truncate(c, 60);
            }
        }
        "Grep" => {
            if let Some(p) = get("pattern") {
                return truncate(p, 60);
            }
        }
        _ => {}
    }
    input
        .as_object()
        .and_then(|o| o.values().find_map(Value::as_str))
        .map(|s| truncate(s, 60))
        .unwrap_or_default()
}

fn usage_of(m: &Value) -> Option<Usage> {
    let u = m.get("usage")?.as_object()?;
    let n = |k: &str| u.get(k).and_then(Value::as_u64).unwrap_or(0);
    Some(Usage {
        input: n("input_tokens"),
        output: n("output_tokens"),
        cache_read: n("cache_read_input_tokens"),
        cache_creation: n("cache_creation_input_tokens"),
        cache_creation_1h: u
            .get("cache_creation")
            .and_then(|c| c.get("ephemeral_1h_input_tokens"))
            .and_then(Value::as_u64)
            .unwrap_or(0),
    })
}

fn tool_uses_of(msg: &Value, at: DateTime<Utc>) -> Vec<ToolCall> {
    let Some(blocks) = msg.get("content").and_then(Value::as_array) else {
        return Vec::new();
    };
    blocks
        .iter()
        .filter(|b| str_of(b, "type") == Some("tool_use"))
        .filter_map(|b| {
            let name = str_of(b, "name")?;
            Some(ToolCall {
                id: str_of(b, "id")?.to_string(),
                name: name.to_string(),
                target: tool_target(name, b.get("input").unwrap_or(&Value::Null)),
                at,
                state: ToolState::Running,
            })
        })
        .collect()
}

/// Parse one JSONL line. Broken or unknown lines yield an empty vec.
pub fn parse_line(line: &str) -> Vec<Entry> {
    let line = line.trim();
    if line.is_empty() {
        return Vec::new();
    }
    let Ok(v) = serde_json::from_str::<Value>(line) else {
        return Vec::new();
    };
    let at = parse_time(&v);
    let kind = str_of(&v, "type");
    let mut out = Vec::new();

    match kind {
        Some("assistant") => {
            if let (Some(msg), Some(at)) = (v.get("message"), at) {
                out.push(Entry::Assistant {
                    message_id: str_of(msg, "id").unwrap_or_default().to_string(),
                    model: str_of(msg, "model").map(cap_display),
                    requested_model: str_of(&v, "requestedModel").map(cap_display),
                    usage: usage_of(msg),
                    tool_uses: tool_uses_of(msg, at),
                    at,
                });
            }
        }
        Some("user") => {
            // `content` may be a plain string; then there are no tool results.
            let blocks = v
                .get("message")
                .and_then(|m| m.get("content"))
                .and_then(Value::as_array);
            if let (Some(blocks), Some(at)) = (blocks, at) {
                for b in blocks {
                    if str_of(b, "type") != Some("tool_result") {
                        continue;
                    }
                    if let Some(id) = str_of(b, "tool_use_id") {
                        out.push(Entry::ToolResult {
                            tool_use_id: id.to_string(),
                            is_error: b.get("is_error").and_then(Value::as_bool).unwrap_or(false),
                            at,
                        });
                    }
                }
            }
        }
        Some("custom-title") => {
            if let Some(t) = str_of(&v, "customTitle") {
                out.push(Entry::Title(cap_display(t)));
            }
        }
        Some("agent-name") => {
            if let Some(t) = str_of(&v, "agentName") {
                out.push(Entry::Title(cap_display(t)));
            }
        }
        _ => {}
    }

    if matches!(kind, Some("assistant" | "user" | "system" | "attachment")) {
        let cwd = str_of(&v, "cwd").and_then(cap_path);
        let git_branch = str_of(&v, "gitBranch").map(cap_display);
        if cwd.is_some() || git_branch.is_some() {
            out.push(Entry::Meta { cwd, git_branch, at });
        }
    }
    out
}

#[derive(Debug, Default)]
pub struct ReadOutcome {
    pub entries: Vec<Entry>,
    /// True when the file shrank and was re-read from the start.
    pub reset: bool,
}

/// Event delivered by [`TranscriptReader::read_each`].
#[derive(Debug)]
pub enum ReadEvent {
    /// The file shrank and is re-read from the start; clear derived state.
    Reset,
    Entry(Entry),
}

pub struct TranscriptReader {
    path: PathBuf,
    offset: u64,
    carry: Vec<u8>,
    /// Inside a line longer than `max_line`: discard bytes until its newline.
    skipping: bool,
    max_line: usize,
}

/// Lines longer than this are dropped (memory guard against corrupt files).
pub const MAX_LINE: usize = 16 * 1024 * 1024;

impl TranscriptReader {
    pub fn new(path: PathBuf) -> Self {
        Self { path, offset: 0, carry: Vec::new(), skipping: false, max_line: MAX_LINE }
    }

    /// Streams bytes appended since the last call, line by line, without collecting
    /// them. An unterminated trailing line stays in `carry` until its newline arrives.
    /// If the file shrank, `f(ReadEvent::Reset)` is called once BEFORE any entry so the
    /// caller can clear its state; then `f(ReadEvent::Entry(..))` per parsed entry.
    /// On an io error, entries already delivered stay delivered; `offset`/`carry`
    /// only ever cover bytes already handed over, so the next call neither loses nor
    /// repeats lines.
    pub fn read_each(&mut self, f: impl FnMut(ReadEvent)) -> io::Result<()> {
        self.read_each_limited(u64::MAX, f).map(|_| ())
    }

    /// Like [`read_each`](Self::read_each) but stops once about `max_bytes` have been
    /// read in this call (the partial line stays in `carry`, so nothing is lost or
    /// repeated). Returns true when the file holds more unread bytes.
    pub fn read_each_limited(&mut self, max_bytes: u64, f: impl FnMut(ReadEvent)) -> io::Result<bool> {
        // Both callbacks deliver through `f`.
        let f = std::cell::RefCell::new(f);
        let (more, _) = self.scan(
            max_bytes,
            || (f.borrow_mut())(ReadEvent::Reset),
            |s| {
                for e in parse_line(s) {
                    (f.borrow_mut())(ReadEvent::Entry(e));
                }
                ControlFlow::Continue(())
            },
        )?;
        Ok(more)
    }

    /// Streams the raw text of every complete line (UTF-8, at most `MAX_LINE`
    /// bytes) from the current offset to the end, without parsing it. `f` may
    /// return `ControlFlow::Break` to stop early; the reader is then spent
    /// (its position is somewhere inside the last read chunk) and must not be
    /// read again. Returns true when `f` stopped the read.
    pub fn read_lines(&mut self, f: impl FnMut(&str) -> ControlFlow<()>) -> io::Result<bool> {
        self.scan(u64::MAX, || {}, f).map(|(_, stopped)| stopped)
    }

    /// Shared line splitter of [`read_each_limited`](Self::read_each_limited)
    /// and [`read_lines`](Self::read_lines). Returns (more unread bytes, stopped).
    fn scan(
        &mut self,
        max_bytes: u64,
        mut on_reset: impl FnMut(),
        mut on_line: impl FnMut(&str) -> ControlFlow<()>,
    ) -> io::Result<(bool, bool)> {
        let mut file = File::open(&self.path)?;
        let len = file.metadata()?.len();
        if len < self.offset {
            self.offset = 0;
            self.carry.clear();
            self.skipping = false;
            on_reset();
        }
        file.seek(SeekFrom::Start(self.offset))?;
        let mut buf = vec![0u8; READ_BUF];
        let mut read = 0u64;
        while read < max_bytes {
            let n = file.read(&mut buf)?;
            if n == 0 {
                break;
            }
            read += n as u64;
            self.offset += n as u64;
            let mut chunk = &buf[..n];
            if self.skipping {
                match chunk.iter().position(|&b| b == b'\n') {
                    None => continue,
                    Some(i) => {
                        self.skipping = false;
                        chunk = &chunk[i + 1..];
                    }
                }
            }
            // Only the newly read bytes can hold a new line end.
            let start = self.carry.len();
            self.carry.extend_from_slice(chunk);
            let Some(pos) = self.carry[start..].iter().rposition(|&b| b == b'\n') else {
                if self.carry.len() > self.max_line {
                    self.carry = Vec::new();
                    self.skipping = true;
                }
                continue;
            };
            let rest = self.carry.split_off(start + pos + 1);
            // `carry` ends with the newline found above; the empty piece after it is not a line.
            for line in self.carry[..self.carry.len() - 1].split(|&b| b == b'\n') {
                if line.len() > self.max_line {
                    continue;
                }
                if let Ok(s) = std::str::from_utf8(line) {
                    if on_line(s).is_break() {
                        return Ok((self.offset < len, true));
                    }
                }
            }
            self.carry = rest;
            if self.carry.len() > self.max_line {
                self.carry = Vec::new();
                self.skipping = true;
            }
        }
        Ok((self.offset < len, false))
    }

    /// Collecting variant of [`read_each`](Self::read_each); only for small reads/tests.
    pub fn read_new(&mut self) -> io::Result<ReadOutcome> {
        let mut out = ReadOutcome::default();
        self.read_each(|ev| match ev {
            ReadEvent::Reset => out.reset = true,
            ReadEvent::Entry(e) => out.entries.push(e),
        })?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn read_lines_streams_raw_lines_and_stops_on_break() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("t.jsonl");
        std::fs::write(&p, "bir\n\niki\nüç\nyarım").unwrap();
        let mut seen = Vec::new();
        let stopped = TranscriptReader::new(p.clone())
            .read_lines(|l| {
                seen.push(l.to_string());
                ControlFlow::Continue(())
            })
            .unwrap();
        assert!(!stopped);
        // The unterminated tail is not a line yet.
        assert_eq!(seen, ["bir", "", "iki", "üç"]);
        let mut n = 0;
        let stopped = TranscriptReader::new(p)
            .read_lines(|_| {
                n += 1;
                if n == 2 { ControlFlow::Break(()) } else { ControlFlow::Continue(()) }
            })
            .unwrap();
        assert!(stopped);
        assert_eq!(n, 2);
    }

    const ASSISTANT: &str = r#"{"type":"assistant","timestamp":"2026-11-02T07:12:52.197Z","requestedModel":"claude-opus-5-5[1m]","message":{"id":"msg_1","model":"claude-opus-5-5","content":[{"type":"tool_use","id":"toolu_A","name":"Read","input":{"file_path":"C:\\demo\\adc.c"}}],"usage":{"input_tokens":10,"output_tokens":5,"cache_read_input_tokens":100,"cache_creation_input_tokens":20}}}"#;

    #[test]
    fn parses_assistant_with_usage_and_tool_use() {
        let e = parse_line(ASSISTANT);
        assert_eq!(e.len(), 1);
        let Entry::Assistant { message_id, model, requested_model, usage, tool_uses, .. } = &e[0] else {
            panic!("not assistant")
        };
        assert_eq!(message_id, "msg_1");
        assert_eq!(model.as_deref(), Some("claude-opus-5-5"));
        assert_eq!(requested_model.as_deref(), Some("claude-opus-5-5[1m]"));
        assert_eq!(
            *usage,
            Some(Usage { input: 10, output: 5, cache_read: 100, cache_creation: 20, cache_creation_1h: 0 })
        );
        assert_eq!(tool_uses.len(), 1);
        assert_eq!(tool_uses[0].name, "Read");
        assert_eq!(tool_uses[0].id, "toolu_A");
        assert_eq!(tool_uses[0].state, ToolState::Running);
    }

    #[test]
    fn multiple_tool_uses_and_meta() {
        let l = r#"{"type":"assistant","timestamp":"2026-11-02T07:12:52Z","cwd":"C:\\demo","gitBranch":"main","message":{"id":"m","content":[{"type":"tool_use","id":"a","name":"Bash","input":{"command":"ls"}},{"type":"text","text":"x"},{"type":"tool_use","id":"b","name":"Grep","input":{"pattern":"foo"}}]}}"#;
        let e = parse_line(l);
        assert_eq!(e.len(), 2);
        let Entry::Assistant { tool_uses, usage, .. } = &e[0] else { panic!() };
        assert_eq!(tool_uses.len(), 2);
        assert!(usage.is_none());
        assert!(matches!(&e[1], Entry::Meta { cwd: Some(c), git_branch: Some(g), .. } if c == "C:\\demo" && g == "main"));
    }

    #[test]
    fn parses_tool_result_error_flag() {
        let l = r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_A","is_error":true,"content":"x"}]},"timestamp":"2026-11-02T07:12:53.000Z"}"#;
        let e = parse_line(l);
        assert!(matches!(&e[0], Entry::ToolResult { tool_use_id, is_error: true, .. } if tool_use_id == "toolu_A"));
        let s = r#"{"type":"user","message":{"role":"user","content":"hi"},"timestamp":"2026-11-02T07:12:53.000Z"}"#;
        assert!(parse_line(s).is_empty());
    }

    #[test]
    fn title_from_custom_title() {
        let e = parse_line(r#"{"type":"custom-title","customTitle":"Pusula","sessionId":"s"}"#);
        assert_eq!(e, vec![Entry::Title("Pusula".into())]);
    }

    #[test]
    fn display_strings_are_capped_at_ingestion() {
        use crate::model::MAX_DISPLAY;
        let long = "t".repeat(5000);
        let e = parse_line(&format!(r#"{{"type":"custom-title","customTitle":"{long}"}}"#));
        assert_eq!(e, vec![Entry::Title("t".repeat(MAX_DISPLAY))]);
        let e = parse_line(&format!(r#"{{"type":"agent-name","agentName":"{long}"}}"#));
        assert_eq!(e, vec![Entry::Title("t".repeat(MAX_DISPLAY))]);
        let l = format!(
            r#"{{"type":"assistant","timestamp":"2026-11-02T07:12:52Z","gitBranch":"{long}","requestedModel":"{long}","message":{{"id":"m","model":"{long}","content":[]}}}}"#
        );
        let e = parse_line(&l);
        let Entry::Assistant { model, requested_model, .. } = &e[0] else { panic!() };
        assert_eq!(model.as_ref().unwrap().chars().count(), MAX_DISPLAY);
        assert_eq!(requested_model.as_ref().unwrap().chars().count(), MAX_DISPLAY);
        let Entry::Meta { git_branch, .. } = &e[1] else { panic!() };
        assert_eq!(git_branch.as_ref().unwrap().chars().count(), MAX_DISPLAY);
    }

    #[test]
    fn overlong_cwd_is_dropped_not_truncated() {
        // A truncated path is a different (possibly existing) folder.
        let long = format!(r"C:\\{}", "d".repeat(400));
        let l = format!(r#"{{"type":"user","timestamp":"2026-11-02T07:12:52Z","cwd":"{long}","gitBranch":"main","message":{{"content":"x"}}}}"#);
        let e = parse_line(&l);
        assert!(matches!(&e[0], Entry::Meta { cwd: None, git_branch: Some(g), .. } if g == "main"));
        let ok = r#"{"type":"user","timestamp":"2026-11-02T07:12:52Z","cwd":"C:\\demo","message":{"content":"x"}}"#;
        assert!(matches!(&parse_line(ok)[0], Entry::Meta { cwd: Some(c), .. } if c == r"C:\demo"));
    }

    fn msg_line(i: usize) -> String {
        format!(
            r#"{{"type":"assistant","timestamp":"2026-11-02T07:12:52Z","message":{{"id":"m{i}","content":[{{"type":"tool_use","id":"t{i}","name":"Bash","input":{{"command":"echo {i}"}}}}]}}}}"#
        )
    }

    fn ids(evs: &[Entry]) -> Vec<String> {
        evs.iter()
            .filter_map(|e| match e {
                Entry::Assistant { message_id, .. } => Some(message_id.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn read_each_limited_stops_early_without_losing_or_repeating_lines() {
        let (_d, p) = tmp();
        let n = 3000;
        let body: String = (0..n).map(|i| msg_line(i) + "\n").collect();
        std::fs::write(&p, &body).unwrap();
        let total = body.len() as u64;
        let mut r = TranscriptReader::new(p);
        let mut got = Vec::new();
        let mut calls = 0;
        loop {
            calls += 1;
            let before = r.offset;
            let more = r
                .read_each_limited(READ_BUF as u64, |ev| {
                    if let ReadEvent::Entry(e) = ev {
                        got.push(e)
                    }
                })
                .unwrap();
            assert!(r.offset - before <= 2 * READ_BUF as u64, "a call reads about max_bytes");
            if !more {
                break;
            }
            assert!(calls < 1000);
        }
        assert!(calls > 2, "budget split the read into several calls ({calls})");
        assert_eq!(r.offset, total);
        let expect: Vec<String> = (0..n).map(|i| format!("m{i}")).collect();
        assert_eq!(ids(&got), expect);
    }

    #[test]
    fn overlong_line_is_dropped_and_reading_continues() {
        let (_d, p) = tmp();
        let mut r = TranscriptReader::new(p.clone());
        r.max_line = 1000;
        let huge = format!(r#"{{"type":"assistant","x":"{}"}}"#, "a".repeat(200_000));
        // also a long-but-allowed line just under the cap boundary
        std::fs::write(&p, format!("{}\n{huge}\n{}\n", msg_line(1), msg_line(2))).unwrap();
        let o = r.read_new().unwrap();
        assert_eq!(ids(&o.entries), ["m1", "m2"]);
        assert!(r.carry.len() <= 1000 + READ_BUF, "carry stays bounded");
        // a huge unterminated tail is dropped too, then the next line is read
        let mut f = std::fs::OpenOptions::new().append(true).open(&p).unwrap();
        write!(f, "{}", "b".repeat(300_000)).unwrap();
        assert!(r.read_new().unwrap().entries.is_empty());
        assert!(r.carry.len() <= 1000 + READ_BUF);
        writeln!(f, "zz\n{}", msg_line(3)).unwrap();
        assert_eq!(ids(&r.read_new().unwrap().entries), ["m3"]);
    }

    #[test]
    fn unknown_and_broken_lines_yield_nothing() {
        assert!(parse_line("{not json").is_empty());
        assert!(parse_line(r#"{"type":"api-request-blob","x":1}"#).is_empty());
        assert!(parse_line("").is_empty());
    }

    #[test]
    fn tool_target_is_short() {
        let t = |n: &str, i: Value| tool_target(n, &i);
        assert_eq!(t("Read", serde_json::json!({"file_path":"C:\\x\\adc.c"})), "adc.c");
        assert_eq!(t("Read", serde_json::json!({"file_path":"/x/y/adc.c"})), "adc.c");
        let long = "a".repeat(100);
        assert_eq!(t("Bash", serde_json::json!({"command": long})).chars().count(), 60);
        assert_eq!(t("Grep", serde_json::json!({"pattern":"foo.*"})), "foo.*");
        assert_eq!(t("Other", serde_json::json!({"n":1,"q":"zzz"})), "zzz");
        assert_eq!(t("Other", serde_json::json!({})), "");
    }

    fn tmp() -> (tempfile::TempDir, PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("t.jsonl");
        (d, p)
    }

    #[test]
    fn partial_line_is_kept_for_next_read() {
        let (_d, p) = tmp();
        let half = &ASSISTANT[..30];
        std::fs::write(&p, format!("{ASSISTANT}\n{half}")).unwrap();
        let mut r = TranscriptReader::new(p.clone());
        let o = r.read_new().unwrap();
        assert_eq!(o.entries.len(), 1);
        assert!(!o.reset);
        let mut f = std::fs::OpenOptions::new().append(true).open(&p).unwrap();
        writeln!(f, "{}", &ASSISTANT[30..]).unwrap();
        let o = r.read_new().unwrap();
        assert_eq!(o.entries.len(), 1);
        assert!(r.read_new().unwrap().entries.is_empty());
    }

    #[test]
    fn shrunk_file_resets_offset() {
        let (_d, p) = tmp();
        std::fs::write(&p, format!("{ASSISTANT}\n{ASSISTANT}\n")).unwrap();
        let mut r = TranscriptReader::new(p.clone());
        assert_eq!(r.read_new().unwrap().entries.len(), 2);
        std::fs::write(&p, format!("{ASSISTANT}\n")).unwrap();
        let o = r.read_new().unwrap();
        assert!(o.reset);
        assert_eq!(o.entries.len(), 1);
    }

    #[test]
    fn reads_in_chunks_without_loading_file() {
        let (_d, p) = tmp();
        assert_eq!(READ_BUF, 64 * 1024);
        let mut f = std::io::BufWriter::new(File::create(&p).unwrap());
        let per_line = ASSISTANT.len() + 1;
        let n = 5 * 1024 * 1024 / per_line + 1;
        for _ in 0..n {
            writeln!(f, "{ASSISTANT}").unwrap();
        }
        f.flush().unwrap();
        drop(f);
        assert!(std::fs::metadata(&p).unwrap().len() >= 5 * 1024 * 1024);
        let mut r = TranscriptReader::new(p);
        let o = r.read_new().unwrap();
        assert_eq!(o.entries.len(), n);
    }

    #[test]
    fn fixture_parses() {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/transcript_basic.jsonl");
        let o = TranscriptReader::new(p).read_new().unwrap();
        // user meta, assistant + meta, tool_result, title
        assert_eq!(o.entries.len(), 5);
    }

    #[test]
    fn cache_creation_1h_split_is_parsed() {
        let line = r#"{"type":"assistant","timestamp":"2026-11-02T07:12:52.197Z","message":{"id":"m","model":"claude-opus-5-5","content":[],"usage":{"input_tokens":1,"output_tokens":2,"cache_read_input_tokens":3,"cache_creation_input_tokens":30,"cache_creation":{"ephemeral_5m_input_tokens":10,"ephemeral_1h_input_tokens":20}}}}"#;
        let e = parse_line(line).into_iter().next().expect("entry");
        match e {
            Entry::Assistant { usage, .. } => {
                let u = usage.unwrap();
                assert_eq!((u.cache_creation, u.cache_creation_1h), (30, 20));
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn read_each_streams_without_collecting() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("big.jsonl");
        let mut f = std::fs::File::create(&p).unwrap();
        let mut lines = 0;
        while f.metadata().unwrap().len() < 1_200_000 {
            writeln!(f, "{ASSISTANT}").unwrap();
            lines += 1;
        }
        drop(f);
        let mut n = 0;
        let mut r = TranscriptReader::new(p.clone());
        r.read_each(|ev| {
            if let ReadEvent::Entry(_) = ev {
                n += 1;
            }
        })
        .unwrap();
        assert!(n >= lines);
        assert_eq!(n, TranscriptReader::new(p).read_new().unwrap().entries.len());
    }

    #[test]
    fn read_each_reset_reported_before_entries() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("s.jsonl");
        std::fs::write(&p, format!("{ASSISTANT}
{ASSISTANT}
")).unwrap();
        let mut r = TranscriptReader::new(p.clone());
        let mut evs = Vec::new();
        r.read_each(|e| evs.push(matches!(e, ReadEvent::Reset))).unwrap();
        assert!(evs.iter().all(|reset| !reset), "no reset on first read");
        std::fs::write(&p, format!("{ASSISTANT}
")).unwrap();
        evs.clear();
        r.read_each(|e| evs.push(matches!(e, ReadEvent::Reset))).unwrap();
        assert!(evs.len() >= 2);
        assert!(evs[0], "reset comes first");
        assert_eq!(evs.iter().filter(|x| **x).count(), 1);
    }
}
