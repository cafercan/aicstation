//! Conversation nodes parsed from transcript lines, and on-demand image reading.
use crate::transcript::tool_target;
use chrono::DateTime;
use serde::Serialize;
use serde_json::Value;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

const MAX_TEXT: usize = 64 * 1024;
const MAX_LINE: usize = crate::transcript::MAX_LINE;
const IMAGE_TYPES: [&str; 4] = ["image/png", "image/jpeg", "image/gif", "image/webp"];

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageRef {
    pub offset: u64,
    pub ordinal: u32,
    pub media_type: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum NodeBody {
    UserText { text: String, truncated: bool, images: Vec<ImageRef> },
    AssistantText { text: String, truncated: bool },
    /// (id, tool, target)
    ToolUses(Vec<(String, String, String)>),
    /// (tool_use_id, is_error, images)
    ToolResults(Vec<(String, bool, Vec<ImageRef>)>),
    CompactBoundary,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub uuid: String,
    pub parent: Option<String>,
    pub logical_parent: Option<String>,
    pub body: NodeBody,
    pub at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageError {
    Gone,
    Io,
}

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn flag(v: &Value, k: &str) -> bool {
    v.get(k).and_then(Value::as_bool).unwrap_or(false)
}

fn cap_text(t: &str) -> (String, bool) {
    if t.len() <= MAX_TEXT {
        return (t.to_string(), false);
    }
    let mut end = MAX_TEXT;
    while !t.is_char_boundary(end) {
        end -= 1;
    }
    (t[..end].to_string(), true)
}

/// Supported media type and base64 payload of an image block, if usable.
fn image_parts(block: &Value) -> Option<(&str, &str)> {
    let src = block.get("source")?;
    if s(src, "type")? != "base64" {
        return None;
    }
    let mt = s(src, "media_type")?;
    if !IMAGE_TYPES.contains(&mt) {
        return None;
    }
    Some((mt, s(src, "data")?))
}

/// Visits every image block of a content array in ordinal order (top-level
/// blocks, descending depth-first into `tool_result` content arrays).
fn walk_images<'a>(content: &'a [Value], ord: &mut u32, f: &mut impl FnMut(u32, &'a Value)) {
    for b in content {
        match s(b, "type") {
            Some("image") => {
                f(*ord, b);
                *ord += 1;
            }
            Some("tool_result") => {
                if let Some(inner) = b.get("content").and_then(Value::as_array) {
                    walk_images(inner, ord, f);
                }
            }
            _ => {}
        }
    }
}

fn refs_of(offset: u64, ord: &mut u32, content: &[Value]) -> Vec<ImageRef> {
    let mut out = Vec::new();
    walk_images(content, ord, &mut |n, b| {
        if let Some((mt, _)) = image_parts(b) {
            out.push(ImageRef { offset, ordinal: n, media_type: mt.to_string() });
        }
    });
    out
}

fn tag<'a>(t: &'a str, name: &str) -> Option<&'a str> {
    let open = format!("<{name}>");
    let start = t.find(&open)? + open.len();
    let end = t[start..].find(&format!("</{name}>"))? + start;
    Some(&t[start..end])
}

fn is_hidden_text(t: &str) -> bool {
    let t = t.trim_start();
    ["<local-command-stdout>", "<local-command-caveat>", "<system-reminder>"]
        .iter()
        .any(|p| t.starts_with(p))
}

fn user_body(offset: u64, content: &Value) -> NodeBody {
    let blocks: Vec<Value> = match content {
        Value::String(t) => vec![serde_json::json!({"type": "text", "text": t})],
        Value::Array(a) => a.clone(),
        _ => return NodeBody::Other,
    };
    if blocks.iter().any(|b| s(b, "type") == Some("tool_result")) {
        let mut ord = 0;
        let mut out = Vec::new();
        for b in &blocks {
            if s(b, "type") == Some("tool_result") {
                let id = s(b, "tool_use_id").unwrap_or("").to_string();
                let err = flag(b, "is_error");
                let imgs = match b.get("content").and_then(Value::as_array) {
                    Some(inner) => refs_of(offset, &mut ord, inner),
                    None => Vec::new(),
                };
                out.push((id, err, imgs));
            } else {
                walk_images(std::slice::from_ref(b), &mut ord, &mut |_, _| {});
            }
        }
        return NodeBody::ToolResults(out);
    }
    let mut ord = 0;
    let images = refs_of(offset, &mut ord, &blocks);
    let texts: Vec<&str> = blocks
        .iter()
        .filter(|b| s(b, "type") == Some("text"))
        .filter_map(|b| s(b, "text"))
        .collect();
    if images.is_empty() && !texts.is_empty() && texts.iter().all(|t| is_hidden_text(t)) {
        return NodeBody::Other;
    }
    let joined = texts
        .iter()
        .map(|t| match tag(t, "command-name") {
            Some(name) => {
                let args = tag(t, "command-args").unwrap_or("").trim();
                if args.is_empty() { name.trim().to_string() } else { format!("{} {}", name.trim(), args) }
            }
            None => t.to_string(),
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    if joined.is_empty() && images.is_empty() {
        return NodeBody::Other;
    }
    let (text, truncated) = cap_text(&joined);
    NodeBody::UserText { text, truncated, images }
}

fn assistant_body(content: &Value) -> NodeBody {
    let Some(blocks) = content.as_array() else {
        return match content.as_str() {
            Some(t) if !t.is_empty() => {
                let (text, truncated) = cap_text(t);
                NodeBody::AssistantText { text, truncated }
            }
            _ => NodeBody::Other,
        };
    };
    let uses: Vec<(String, String, String)> = blocks
        .iter()
        .filter(|b| s(b, "type") == Some("tool_use"))
        .map(|b| {
            let name = s(b, "name").unwrap_or("");
            let input = b.get("input").cloned().unwrap_or(Value::Null);
            (s(b, "id").unwrap_or("").to_string(), name.to_string(), tool_target(name, &input))
        })
        .collect();
    let texts: Vec<&str> = blocks
        .iter()
        .filter(|b| s(b, "type") == Some("text"))
        .filter_map(|b| s(b, "text"))
        .filter(|t| !t.is_empty())
        .collect();
    // Claude Code writes one content block per assistant line; a mixed
    // text + tool_use line (never seen in real transcripts) keeps its text.
    if !texts.is_empty() {
        let (text, truncated) = cap_text(&texts.join("\n\n"));
        return NodeBody::AssistantText { text, truncated };
    }
    if !uses.is_empty() {
        return NodeBody::ToolUses(uses);
    }
    NodeBody::Other
}

/// Parses one transcript line into a node. `None` only for unparsable JSON,
/// a missing `uuid`, or a sidechain line; hidden lines become `Other`.
pub fn parse_node(offset: u64, line: &str) -> Option<Node> {
    let v: Value = serde_json::from_str(line).ok()?;
    let uuid = s(&v, "uuid")?.to_string();
    if flag(&v, "isSidechain") {
        return None;
    }
    let hidden = flag(&v, "isMeta") || flag(&v, "isCompactSummary") || flag(&v, "isVisibleInTranscriptOnly");
    let content = v.get("message").and_then(|m| m.get("content"));
    let body = match (s(&v, "type"), hidden) {
        (_, true) => NodeBody::Other,
        (Some("user"), _) => content.map_or(NodeBody::Other, |c| user_body(offset, c)),
        (Some("assistant"), _) => content.map_or(NodeBody::Other, assistant_body),
        (Some("system"), _) if s(&v, "subtype") == Some("compact_boundary") => NodeBody::CompactBoundary,
        _ => NodeBody::Other,
    };
    Some(Node {
        uuid,
        parent: s(&v, "parentUuid").map(str::to_string),
        logical_parent: s(&v, "logicalParentUuid").map(str::to_string),
        body,
        at_ms: s(&v, "timestamp")
            .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
            .map_or(0, |t| t.timestamp_millis()),
    })
}

fn valid_b64(d: &str) -> bool {
    d.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'=' | b'\r' | b'\n'))
}

/// Re-reads the line at `offset` and returns `(media_type, base64)` of the
/// image with the given ordinal. Any mismatch means the file changed: `Gone`.
pub fn read_image(path: &Path, offset: u64, ordinal: u32) -> Result<(String, String), ImageError> {
    let mut f = File::open(path).map_err(|_| ImageError::Io)?;
    f.seek(SeekFrom::Start(offset)).map_err(|_| ImageError::Gone)?;
    let mut buf = Vec::new();
    BufReader::new(f.take(MAX_LINE as u64 + 1))
        .read_until(b'\n', &mut buf)
        .map_err(|_| ImageError::Io)?;
    if buf.len() > MAX_LINE {
        return Err(ImageError::Gone);
    }
    let line = std::str::from_utf8(&buf).map_err(|_| ImageError::Gone)?;
    let v: Value = serde_json::from_str(line.trim_end()).map_err(|_| ImageError::Gone)?;
    let content = v
        .get("message")
        .and_then(|m| m.get("content"))
        .and_then(Value::as_array)
        .ok_or(ImageError::Gone)?;
    let mut found = None;
    walk_images(content, &mut 0, &mut |n, b| {
        if n == ordinal {
            found = image_parts(b);
        }
    });
    let (mt, data) = found.ok_or(ImageError::Gone)?;
    if !valid_b64(data) {
        return Err(ImageError::Gone);
    }
    Ok((mt.to_string(), data.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/conversation_basic.jsonl");

    fn line_offsets() -> Vec<(u64, String)> {
        let text = std::fs::read_to_string(FIXTURE).unwrap();
        let mut off = 0u64;
        text.split_inclusive('\n')
            .map(|l| {
                let r = (off, l.trim_end().to_string());
                off += l.len() as u64;
                r
            })
            .collect()
    }

    fn user(content: &str, extra: &str) -> String {
        format!(r#"{{"type":"user","uuid":"x","parentUuid":"p"{extra},"message":{{"content":{content}}}}}"#)
    }

    #[test]
    fn parses_user_text_with_pasted_image() {
        let l = line_offsets();
        let n = parse_node(l[0].0, &l[0].1).unwrap();
        assert_eq!(n.uuid, "u1");
        assert_eq!(n.parent, None);
        assert_eq!(n.at_ms, 1_767_225_600_000);
        assert_eq!(
            n.body,
            NodeBody::UserText {
                text: "look at this".into(),
                truncated: false,
                images: vec![ImageRef { offset: l[0].0, ordinal: 0, media_type: "image/png".into() }],
            }
        );
    }

    #[test]
    fn parses_tool_result_images_in_order() {
        let l = line_offsets();
        let n = parse_node(l[2].0, &l[2].1).unwrap();
        let NodeBody::ToolResults(r) = n.body else { panic!("not tool results") };
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].0, "t1");
        assert!(!r[0].1);
        let ords: Vec<_> = r[0].2.iter().map(|i| (i.ordinal, i.media_type.as_str())).collect();
        assert_eq!(ords, vec![(0, "image/png"), (1, "image/jpeg")]);
        let a = parse_node(l[1].0, &l[1].1).unwrap();
        assert_eq!(a.body, NodeBody::ToolUses(vec![("t1".into(), "Read".into(), "y.png".into())]));
    }

    #[test]
    fn hides_meta_and_compact_summary() {
        for extra in [r#","isMeta":true"#, r#","isCompactSummary":true"#, r#","isVisibleInTranscriptOnly":true"#] {
            let n = parse_node(0, &user(r#""hi""#, extra)).unwrap();
            assert_eq!(n.body, NodeBody::Other, "{extra}");
            assert_eq!(n.uuid, "x");
            assert_eq!(n.parent.as_deref(), Some("p"));
        }
        let att = r#"{"type":"attachment","uuid":"x","parentUuid":"p"}"#;
        let n = parse_node(0, att).unwrap();
        assert_eq!(n.body, NodeBody::Other);
        assert_eq!(n.parent.as_deref(), Some("p"));
        let sys = r#"{"type":"system","subtype":"other","uuid":"x","parentUuid":null}"#;
        assert_eq!(parse_node(0, sys).unwrap().body, NodeBody::Other);
        let cb = r#"{"type":"system","subtype":"compact_boundary","uuid":"x","parentUuid":null,"logicalParentUuid":"l"}"#;
        let n = parse_node(0, cb).unwrap();
        assert_eq!(n.body, NodeBody::CompactBoundary);
        assert_eq!(n.logical_parent.as_deref(), Some("l"));
        let th = r#"{"type":"assistant","uuid":"x","message":{"content":[{"type":"thinking","thinking":"hm"}]}}"#;
        assert_eq!(parse_node(0, th).unwrap().body, NodeBody::Other);
        let out = user(r#""<local-command-stdout>ok</local-command-stdout>""#, "");
        assert_eq!(parse_node(0, &out).unwrap().body, NodeBody::Other);
    }

    #[test]
    fn skips_sidechain() {
        assert_eq!(parse_node(0, &user(r#""hi""#, r#","isSidechain":true"#)), None);
        assert_eq!(parse_node(0, "not json"), None);
        assert_eq!(parse_node(0, r#"{"type":"user"}"#), None);
    }

    #[test]
    fn command_wrapper_becomes_command_text() {
        let c = r#""<command-name>/model</command-name><command-args>opus</command-args>""#;
        let n = parse_node(0, &user(c, "")).unwrap();
        let NodeBody::UserText { text, .. } = n.body else { panic!() };
        assert_eq!(text, "/model opus");
    }

    #[test]
    fn command_wrapper_found_anywhere_in_text() {
        let text_of = |c: &str| {
            let NodeBody::UserText { text, .. } = parse_node(0, &user(c, "")).unwrap().body else { panic!() };
            text
        };
        let a = r#""<command-message>model</command-message><command-name>/model</command-name><command-args>opus</command-args>""#;
        assert_eq!(text_of(a), "/model opus");
        let b = r#""<command-name>/model</command-name>\n            <command-message>model</command-message>\n            <command-args></command-args>""#;
        assert_eq!(text_of(b), "/model");
    }

    #[test]
    fn ignores_unsupported_images() {
        let c = r#"[{"type":"image","source":{"type":"base64","media_type":"image/svg+xml","data":"AAAA"}},{"type":"image","source":{"type":"url","url":"http://x"}},{"type":"text","text":"t"}]"#;
        let n = parse_node(0, &user(c, "")).unwrap();
        let NodeBody::UserText { images, .. } = n.body else { panic!() };
        assert!(images.is_empty());
    }

    #[test]
    fn caps_text_at_64k() {
        let c = format!("\"{}\"", "ç".repeat(70_000));
        let n = parse_node(0, &user(&c, "")).unwrap();
        let NodeBody::UserText { text, truncated, .. } = n.body else { panic!() };
        assert!(text.len() <= 65536);
        assert!(truncated);
    }

    #[test]
    fn read_image_returns_block() {
        let l = line_offsets();
        let p = Path::new(FIXTURE);
        assert_eq!(read_image(p, l[0].0, 0), Ok(("image/png".into(), "iVBORw0KGgo=".into())));
        assert_eq!(read_image(p, l[2].0, 1), Ok(("image/jpeg".into(), "/9j/4AAQ".into())));
    }

    #[test]
    fn read_image_rejects_moved_line() {
        let l = line_offsets();
        let p = Path::new(FIXTURE);
        assert_eq!(read_image(p, l[0].0 + 5, 0), Err(ImageError::Gone));
        assert_eq!(read_image(p, l[0].0, 5), Err(ImageError::Gone));
        assert_eq!(read_image(p, 1 << 40, 0), Err(ImageError::Gone));
        let dir = tempfile::tempdir().unwrap();
        let bad = dir.path().join("bad.jsonl");
        let mut f = File::create(&bad).unwrap();
        writeln!(f, r#"{{"message":{{"content":[{{"type":"image","source":{{"type":"base64","media_type":"image/png","data":"AA<>"}}}}]}}}}"#).unwrap();
        assert_eq!(read_image(&bad, 0, 0), Err(ImageError::Gone));
    }
}
