//! Content search over closed and live transcripts ("İçerikte ara"). Files are
//! streamed line by line (never loaded whole); only the text blocks of user and
//! assistant messages are matched, case- and accent-insensitively (Turkish
//! İ/ı fold to i). The search stops at the hit limit or when `cancelled`
//! turns true (a newer search started).
use crate::transcript::TranscriptReader;
use chrono::DateTime;
use serde::Serialize;
use serde_json::Value;
use std::ops::ControlFlow;
use std::path::PathBuf;

pub const MIN_QUERY_CHARS: usize = 2;
pub const MAX_QUERY_CHARS: usize = 200;
/// Hits returned at most per search.
pub const MAX_HITS: usize = 50;
/// Hits taken at most from one session, so one long session cannot fill the list.
pub const MAX_HITS_PER_SESSION: usize = 3;
/// Characters of a snippet (before + match + after).
pub const SNIPPET_CHARS: usize = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    User,
    Assistant,
}

/// A snippet split around the match so the UI can highlight it without
/// computing offsets (JS string indices are UTF-16).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    pub before: String,
    pub hit: String,
    pub after: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub session_id: String,
    /// Message timestamp (ms since the epoch), 0 when the line has none.
    pub at_ms: u64,
    pub role: Role,
    pub snippet: Snippet,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchOutcome {
    pub hits: Vec<SearchHit>,
    /// The hit limit was reached; more matches may exist.
    pub truncated: bool,
    /// A newer search cancelled this one; `hits` is partial.
    pub cancelled: bool,
}

/// One char to its search form: lowercase without common Latin accents.
/// Always exactly one char, so folded positions equal original positions.
pub fn fold_char(c: char) -> char {
    if c.is_ascii() {
        return c.to_ascii_lowercase();
    }
    let l = match c {
        'İ' | 'ı' => 'i',
        _ => c.to_lowercase().next().unwrap_or(c),
    };
    match l {
        'à'..='å' | 'ā' | 'ă' | 'ą' => 'a',
        'ç' | 'ć' | 'č' => 'c',
        'ď' => 'd',
        'è'..='ë' | 'ē' | 'ę' | 'ě' | 'ə' => 'e',
        'ğ' => 'g',
        'ì'..='ï' => 'i',
        'ł' => 'l',
        'ñ' | 'ń' | 'ň' => 'n',
        'ò'..='ö' | 'ø' | 'ő' => 'o',
        'ř' => 'r',
        'ś' | 'š' | 'ş' => 's',
        'ť' => 't',
        'ù'..='ü' | 'ū' | 'ů' | 'ű' => 'u',
        'ý' | 'ÿ' => 'y',
        'ź' | 'ż' | 'ž' => 'z',
        'ё' => 'е',
        _ => l,
    }
}

pub fn fold(s: &str) -> String {
    s.chars().map(fold_char).collect()
}

/// Trimmed query of 2..=200 chars without control characters, or None.
pub fn validate_query(q: &str) -> Option<String> {
    let q = q.trim();
    let n = q.chars().count();
    if !(MIN_QUERY_CHARS..=MAX_QUERY_CHARS).contains(&n) || q.chars().any(char::is_control) {
        return None;
    }
    Some(q.to_string())
}

/// Role, timestamp and text of a user/assistant message line: its `text`
/// blocks (or a plain string content) joined by newlines. Tool calls, tool
/// results, thinking and meta lines yield None.
pub fn message_text(line: &str) -> Option<(Role, u64, String)> {
    let v: Value = serde_json::from_str(line.trim()).ok()?;
    let role = match v.get("type").and_then(Value::as_str)? {
        "user" => Role::User,
        "assistant" => Role::Assistant,
        _ => return None,
    };
    if v.get("isMeta").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    let content = v.get("message")?.get("content")?;
    let text = match content {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => {
            let parts: Vec<&str> = blocks
                .iter()
                .filter(|b| b.get("type").and_then(Value::as_str) == Some("text"))
                .filter_map(|b| b.get("text").and_then(Value::as_str))
                .collect();
            parts.join("\n")
        }
        _ => return None,
    };
    if text.is_empty() {
        return None;
    }
    let at = v
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
        .map_or(0, |t| t.timestamp_millis().max(0) as u64);
    Some((role, at, text))
}

/// Char index of the first occurrence of `needle` in `hay` (both folded).
fn find(hay: &[char], needle: &[char]) -> Option<usize> {
    if needle.is_empty() || needle.len() > hay.len() {
        return None;
    }
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Snippet of at most [`SNIPPET_CHARS`] chars around `text[pos..pos + len]`
/// (char indices), the match centred where possible. Whitespace becomes a
/// single space each (positions do not move).
pub fn snippet(text: &[char], pos: usize, len: usize) -> Snippet {
    let flat = |s: &[char]| -> String { s.iter().map(|&c| if c.is_whitespace() { ' ' } else { c }).collect() };
    let pos = pos.min(text.len());
    let len = len.min(text.len() - pos).min(SNIPPET_CHARS);
    let room = SNIPPET_CHARS - len;
    let mut start = pos.saturating_sub(room / 2);
    let mut end = (pos + len + (room - (pos - start))).min(text.len());
    // Short tail: give the unused room back to the head.
    let used = end - start;
    if used < SNIPPET_CHARS {
        start = start.saturating_sub(SNIPPET_CHARS - used);
        end = (start + SNIPPET_CHARS).min(text.len());
    }
    Snippet {
        before: flat(&text[start..pos]),
        hit: flat(&text[pos..pos + len]),
        after: flat(&text[pos + len..end]),
    }
}

/// Searches `files` (session id, transcript path) in the given order and
/// returns at most `max_hits` hits, [`MAX_HITS_PER_SESSION`] per session.
/// `cancelled` is polled per line. `query` must already be validated.
pub fn search_transcripts(
    files: &[(String, PathBuf)],
    query: &str,
    max_hits: usize,
    cancelled: &dyn Fn() -> bool,
) -> SearchOutcome {
    let needle_s = fold(query);
    let needle: Vec<char> = needle_s.chars().collect();
    // JSON escapes `"` and `\`; for other queries a match in the decoded text
    // is also a match in the folded raw line, which is much cheaper to test
    // than parsing every line.
    let prefilter = !query.contains(['"', '\\']);
    let mut out = SearchOutcome::default();
    let mut folded_line = String::new();
    for (id, path) in files {
        if cancelled() {
            out.cancelled = true;
            return out;
        }
        let mut in_session = 0;
        let mut stop = false;
        let _ = TranscriptReader::new(path.clone()).read_lines(|line| {
            if cancelled() {
                out.cancelled = true;
                stop = true;
                return ControlFlow::Break(());
            }
            if prefilter {
                folded_line.clear();
                folded_line.extend(line.chars().map(fold_char));
                if !folded_line.contains(&needle_s) {
                    return ControlFlow::Continue(());
                }
            }
            let Some((role, at_ms, text)) = message_text(line) else { return ControlFlow::Continue(()) };
            let chars: Vec<char> = text.chars().collect();
            let hay: Vec<char> = chars.iter().map(|&c| fold_char(c)).collect();
            let Some(pos) = find(&hay, &needle) else { return ControlFlow::Continue(()) };
            if out.hits.len() >= max_hits {
                out.truncated = true;
                stop = true;
                return ControlFlow::Break(());
            }
            out.hits.push(SearchHit {
                session_id: id.clone(),
                at_ms,
                role,
                snippet: snippet(&chars, pos, needle.len()),
            });
            in_session += 1;
            if in_session >= MAX_HITS_PER_SESSION {
                return ControlFlow::Break(());
            }
            ControlFlow::Continue(())
        });
        if stop {
            return out;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::fs;

    fn user(text: &str, ts: &str) -> String {
        serde_json::json!({"type":"user","timestamp":ts,"message":{"role":"user","content":text}}).to_string()
    }
    fn asst(text: &str, ts: &str) -> String {
        serde_json::json!({"type":"assistant","timestamp":ts,"message":{"id":"m","content":[
            {"type":"thinking","thinking":"gizli düşünce"},
            {"type":"text","text":text},
            {"type":"tool_use","id":"t","name":"Bash","input":{"command":"echo kalibrasyon"}}
        ]}})
        .to_string()
    }
    fn tool_result(text: &str) -> String {
        serde_json::json!({"type":"user","timestamp":"2026-11-02T10:00:00Z","message":{"content":[
            {"type":"tool_result","tool_use_id":"t","content":text}
        ]}})
        .to_string()
    }

    fn write(dir: &std::path::Path, id: &str, lines: &[String]) -> (String, PathBuf) {
        let p = dir.join(format!("{id}.jsonl"));
        fs::write(&p, lines.join("\n") + "\n").unwrap();
        (id.to_string(), p)
    }

    #[test]
    fn fold_is_case_and_accent_insensitive_including_turkish_i() {
        assert_eq!(fold("İSTANBUL Işık ĞÜŞÖÇ"), "istanbul isik gusoc");
        assert_eq!(fold("Café Ə"), "cafe e");
        for c in ['İ', 'ı', 'Ü', 'ß', '中', 'Ё'] {
            assert_eq!(fold(&c.to_string()).chars().count(), 1, "{c}");
        }
    }

    #[test]
    fn query_length_is_2_to_200_chars_after_trim() {
        assert_eq!(validate_query("  ab "), Some("ab".into()));
        assert_eq!(validate_query("a"), None);
        assert_eq!(validate_query("   "), None);
        assert_eq!(validate_query(&"ş".repeat(200)), Some("ş".repeat(200)));
        assert_eq!(validate_query(&"ş".repeat(201)), None);
        assert_eq!(validate_query("a\u{0}b"), None);
    }

    #[test]
    fn message_text_takes_only_user_and_assistant_text() {
        let (r, at, t) = message_text(&user("Merhaba", "2026-11-02T10:00:00Z")).unwrap();
        assert_eq!((r, t.as_str()), (Role::User, "Merhaba"));
        assert_eq!(at, 1_793_613_600_000);
        let (r, _, t) = message_text(&asst("Yanıt", "2026-11-02T10:00:00Z")).unwrap();
        assert_eq!((r, t.as_str()), (Role::Assistant, "Yanıt"));
        assert_eq!(message_text(&tool_result("kalibrasyon")), None);
        assert_eq!(message_text(r#"{"type":"summary","summary":"x"}"#), None);
        assert_eq!(message_text("{bozuk"), None);
        let meta = r#"{"type":"user","isMeta":true,"message":{"content":"<command>"}}"#;
        assert_eq!(message_text(meta), None);
    }

    #[test]
    fn snippet_is_at_most_120_chars_and_contains_the_match() {
        let text: Vec<char> = format!("{}HEDEF{}", "a".repeat(300), "b".repeat(300)).chars().collect();
        let s = snippet(&text, 300, 5);
        assert_eq!(s.hit, "HEDEF");
        let total = s.before.chars().count() + s.hit.chars().count() + s.after.chars().count();
        assert_eq!(total, SNIPPET_CHARS);
        assert!(s.before.chars().count().abs_diff(s.after.chars().count()) <= 1);

        // Near the start: no negative offset, the window fills to the right.
        let s = snippet(&text, 0, 3);
        assert_eq!(s.before, "");
        assert_eq!(s.after.chars().count(), SNIPPET_CHARS - 3);
        // Near the end: the window fills to the left.
        let s = snippet(&text, text.len() - 2, 2);
        assert_eq!(s.after, "");
        assert_eq!(s.before.chars().count(), SNIPPET_CHARS - 2);
        // Short text: everything, newlines flattened.
        let short: Vec<char> = "satır bir\nsatır iki".chars().collect();
        let s = snippet(&short, 6, 3);
        assert_eq!((s.before.as_str(), s.hit.as_str(), s.after.as_str()), ("satır ", "bir", " satır iki"));
        // A match longer than the snippet is cut to it.
        let long: Vec<char> = "x".repeat(200).chars().collect();
        let s = snippet(&long, 0, 200);
        assert_eq!(s.hit.chars().count(), SNIPPET_CHARS);
    }

    #[test]
    fn finds_matches_in_text_not_in_tools_or_thinking() {
        let tmp = tempfile::tempdir().unwrap();
        let a = write(tmp.path(), "a", &[
            user("ADC KALİBRASYONU nasıl yapılır?", "2026-11-02T10:00:00Z"),
            tool_result("kalibrasyon log"),
            asst("Önce referansı ölç.", "2026-11-02T10:01:00Z"),
        ]);
        let b = write(tmp.path(), "b", &[asst("Kalibrasyon tamam.", "2026-11-02T11:00:00Z")]);
        let out = search_transcripts(&[a, b], "kalibrasyon", MAX_HITS, &|| false);
        assert!(!out.truncated && !out.cancelled);
        let ids: Vec<&str> = out.hits.iter().map(|h| h.session_id.as_str()).collect();
        assert_eq!(ids, ["a", "b"]);
        assert_eq!(out.hits[0].role, Role::User);
        assert_eq!(out.hits[0].snippet.hit, "KALİBRASYONU".chars().take(11).collect::<String>());
        assert_eq!(search_transcripts(&[], "gizli", MAX_HITS, &|| false).hits.len(), 0);
        let (_, p) = write(tmp.path(), "c", &[asst("x", "2026-11-02T11:00:00Z")]);
        assert!(search_transcripts(&[("c".into(), p), ], "gizli düşünce", MAX_HITS, &|| false).hits.is_empty());
    }

    #[test]
    fn query_with_quote_or_backslash_skips_the_prefilter_but_still_matches() {
        let tmp = tempfile::tempdir().unwrap();
        let a = write(tmp.path(), "a", &[user(r#"yol "C:\temp" burada"#, "2026-11-02T10:00:00Z")]);
        let out = search_transcripts(&[a], r#""C:\temp""#, MAX_HITS, &|| false);
        assert_eq!(out.hits.len(), 1);
    }

    #[test]
    fn stops_at_the_limit_and_caps_hits_per_session() {
        let tmp = tempfile::tempdir().unwrap();
        let lines: Vec<String> = (0..10).map(|i| user(&format!("eşleşme {i}"), "2026-11-02T10:00:00Z")).collect();
        let files: Vec<_> = (0..5).map(|i| write(tmp.path(), &format!("s{i}"), &lines)).collect();
        let out = search_transcripts(&files, "esles", 50, &|| false);
        assert_eq!(out.hits.len(), 15);
        assert!(!out.truncated);
        for i in 0..5 {
            assert_eq!(out.hits.iter().filter(|h| h.session_id == format!("s{i}")).count(), MAX_HITS_PER_SESSION);
        }
        let out = search_transcripts(&files, "eşleşme", 4, &|| false);
        assert_eq!(out.hits.len(), 4);
        assert!(out.truncated);
    }

    #[test]
    fn cancellation_stops_between_lines_and_files() {
        let tmp = tempfile::tempdir().unwrap();
        let lines: Vec<String> = (0..10).map(|i| user(&format!("satır {i}"), "2026-11-02T10:00:00Z")).collect();
        let files: Vec<_> = (0..3).map(|i| write(tmp.path(), &format!("s{i}"), &lines)).collect();
        // Cancelled up front: nothing is read.
        let out = search_transcripts(&files, "satır", 50, &|| true);
        assert!(out.cancelled && out.hits.is_empty());
        // Cancelled after a few polls: partial result, flagged.
        let polls = Cell::new(0);
        let out = search_transcripts(&files, "satır", 50, &|| {
            polls.set(polls.get() + 1);
            polls.get() > 4
        });
        assert!(out.cancelled);
        assert!(out.hits.len() < 3 * MAX_HITS_PER_SESSION);
        assert!(polls.get() <= 6, "stopped promptly, polled {}", polls.get());
    }

    #[test]
    fn unreadable_file_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let a = write(tmp.path(), "a", &[user("bulunacak", "2026-11-02T10:00:00Z")]);
        let missing = ("x".to_string(), tmp.path().join("yok.jsonl"));
        let out = search_transcripts(&[missing, a], "bulunacak", 50, &|| false);
        assert_eq!(out.hits.len(), 1);
    }
}
