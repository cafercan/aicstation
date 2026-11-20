//! One transcript line into a [`Node`]: what it shows (user text, Claude's
//! text, tool calls and results, notices) or `Other` when it is hidden.
use crate::transcript::tool_target;
use chrono::DateTime;
use serde::Serialize;
use serde_json::Value;

pub(super) const MAX_TEXT: usize = 64 * 1024;
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
    /// Claude's thinking that was kept with its text (many thinking blocks
    /// are written without it; those stay `Other`).
    Thinking { text: String, truncated: bool },
    /// (id, tool, target)
    ToolUses(Vec<(String, String, String)>),
    /// (tool_use_id, is_error, images)
    ToolResults(Vec<(String, bool, Vec<ImageRef>)>),
    CompactBoundary,
    /// A line the system wrote into the conversation (a background task
    /// finished): its summary, empty when it has none.
    Notice { text: String },
    /// Claude Code's note at the end of a turn: how long the turn took.
    TurnDone { duration_ms: u64 },
    /// A file or folder the user named with `@` in a message; Claude Code
    /// attaches it on the line after the message.
    Mentioned { path: String, dir: bool },
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
    /// Byte offset of the line; orders nodes in file order.
    pub offset: u64,
    /// `message.id` of an assistant line. Claude Code writes each content
    /// block of one API message on its own line, all with this id.
    pub message_id: Option<String>,
}

pub(super) fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn flag(v: &Value, k: &str) -> bool {
    v.get(k).and_then(Value::as_bool).unwrap_or(false)
}

/// Longest prefix of `t` of at most `max` bytes that ends on a char boundary.
pub(super) fn prefix(t: &str, max: usize) -> &str {
    if t.len() <= max {
        return t;
    }
    let mut end = max;
    while !t.is_char_boundary(end) {
        end -= 1;
    }
    &t[..end]
}

/// `t` cut to `MAX_TEXT` bytes, and whether it was cut. A cut text never
/// ends in the blank line that separated two joined blocks.
fn cap_text(t: &str) -> (String, bool) {
    let p = prefix(t, MAX_TEXT);
    if p.len() < t.len() {
        (p.trim_end().to_string(), true)
    } else {
        (p.to_string(), false)
    }
}

/// Supported media type and base64 payload of an image block, if usable.
pub(super) fn image_parts(block: &Value) -> Option<(&str, &str)> {
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
pub(super) fn walk_images<'a>(content: &'a [Value], ord: &mut u32, f: &mut impl FnMut(u32, &'a Value)) {
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

/// A text block the user never typed: empty once its system reminders are
/// gone, or local command output / a caveat / shell output.
fn is_hidden_text(t: &str) -> bool {
    let t = t.trim_start();
    t.is_empty()
        || ["<local-command-stdout>", "<local-command-caveat>", "<bash-stdout>", "<bash-stderr>"]
            .iter()
            .any(|p| t.starts_with(p))
}

/// `t` without its closed `<system-reminder>…</system-reminder>` blocks
/// (Claude Code adds them to the user's text and always closes them; an
/// unclosed tag is the user's own text and stays). Text on both sides of a
/// removed block keeps one run of the whitespace around it.
fn strip_reminders(t: &str) -> String {
    const OPEN: &str = "<system-reminder>";
    const CLOSE: &str = "</system-reminder>";
    if !t.contains(OPEN) {
        return t.to_string();
    }
    let mut out = String::with_capacity(t.len());
    let mut rest = t;
    while let Some(at) = rest.find(OPEN) {
        let Some(end) = rest[at + OPEN.len()..].find(CLOSE) else { break };
        out.push_str(&rest[..at]);
        rest = &rest[at + OPEN.len() + end + CLOSE.len()..];
        let after = rest.trim_start();
        if !out.ends_with(char::is_whitespace) && !out.is_empty() {
            out.push_str(&rest[..rest.len() - after.len()]);
        }
        rest = after;
    }
    out.push_str(rest);
    out.trim_end().to_string()
}

/// `t` without `<pasted_content …>` / `</pasted_content …>` wrappers (Claude
/// Code keeps a long paste in them, the closing tag also with the paste's
/// id); what they wrap stays.
fn strip_pasted(t: &str) -> String {
    const OPEN: &str = "<pasted_content";
    const CLOSE: &str = "</pasted_content";
    let mut out = String::with_capacity(t.len());
    let mut rest = t;
    while let Some(at) = rest.find('<') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let tag = rest
            .strip_prefix(CLOSE)
            .or_else(|| rest.strip_prefix(OPEN))
            .filter(|a| a.starts_with([' ', '>']));
        if let Some(after) = tag {
            match after.find('>') {
                Some(end) => rest = &after[end + 1..],
                None => {
                    out.push_str(rest);
                    rest = "";
                }
            }
        } else {
            out.push('<');
            rest = &rest[1..];
        }
    }
    out.push_str(rest);
    out
}

/// One text block of a user message as shown: a slash command as typed, a
/// `!` shell command as `! cmd`, a paste without its wrapper. A block is a
/// slash command only when it starts with Claude Code's command tags, so a
/// message that mentions them stays text.
fn shown_user_text(t: &str) -> String {
    let lead = t.trim_start();
    let command = lead.starts_with("<command-name>") || lead.starts_with("<command-message>");
    if let Some(name) = tag(t, "command-name").filter(|_| command) {
        let args = tag(t, "command-args").unwrap_or("").trim();
        return if args.is_empty() { name.trim().to_string() } else { format!("{} {}", name.trim(), args) };
    }
    if t.trim_start().starts_with("<bash-input>") {
        if let Some(cmd) = tag(t, "bash-input") {
            return format!("! {}", cmd.trim());
        }
    }
    // Blank lines left around a paste (its tags sit on lines of their own) go.
    strip_pasted(t).trim_matches(['\n', '\r']).to_string()
}

/// Body of a system-written prompt: a background task's notice (its summary)
/// or, for anything else, `Other`.
fn notice_body(content: &Value) -> NodeBody {
    let text = match content {
        Value::String(t) => t.as_str(),
        Value::Array(a) => a.iter().filter(|b| s(b, "type") == Some("text")).find_map(|b| s(b, "text")).unwrap_or(""),
        _ => "",
    };
    if !text.trim_start().starts_with("<task-notification>") {
        return NodeBody::Other;
    }
    let summary = tag(text, "summary").map_or("", str::trim);
    NodeBody::Notice { text: prefix(summary, MAX_TEXT).to_string() }
}

/// Who wrote a prompt: `origin.kind` (absent on older lines) and whether
/// `promptSource` is `system`.
fn written_by_system(origin_kind: Option<&str>, prompt_source: Option<&str>) -> bool {
    origin_kind.is_some_and(|k| k != "human") || prompt_source == Some("system")
}

fn user_body(offset: u64, content: &Value) -> NodeBody {
    // Borrowed, not cloned: the blocks may carry megabytes of base64.
    let single;
    let blocks: &[Value] = match content {
        Value::String(t) => {
            single = [serde_json::json!({"type": "text", "text": t})];
            &single
        }
        Value::Array(a) => a,
        _ => return NodeBody::Other,
    };
    if blocks.iter().any(|b| s(b, "type") == Some("tool_result")) {
        let mut ord = 0;
        let mut out = Vec::new();
        for b in blocks {
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
    let images = refs_of(offset, &mut ord, blocks);
    let texts: Vec<&str> = blocks
        .iter()
        .filter(|b| s(b, "type") == Some("text"))
        .filter_map(|b| s(b, "text"))
        .collect();
    if images.is_empty() && texts.len() == 1 && texts[0].trim_start().starts_with("<task-notification>") {
        // Older lines of a background task's notification carry no origin.
        return notice_body(content);
    }
    let shown: Vec<String> = texts.iter().map(|t| strip_reminders(t)).filter(|t| !is_hidden_text(t)).collect();
    if images.is_empty() && !texts.is_empty() && shown.is_empty() {
        return NodeBody::Other;
    }
    let joined = shown.iter().map(|t| shown_user_text(t)).collect::<Vec<_>>().join("\n\n");
    if joined.is_empty() && images.is_empty() {
        return NodeBody::Other;
    }
    let (text, truncated) = cap_text(&joined);
    NodeBody::UserText { text, truncated, images }
}

/// A user line: the user's message, a tool result, or a system-written
/// prompt (a background task's notice; anything else is hidden).
fn user_line_body(offset: u64, v: &Value, content: &Value) -> NodeBody {
    let kind = v.get("origin").and_then(|o| s(o, "kind"));
    if kind == Some("task-notification") {
        return notice_body(content);
    }
    let body = user_body(offset, content);
    if written_by_system(kind, s(v, "promptSource")) && !matches!(body, NodeBody::ToolResults(_)) {
        return NodeBody::Other;
    }
    body
}

/// Longest path shown for a file named with `@`.
const MAX_MENTION: usize = 1024;

/// An attachment line. Shown: `queued_command` (a prompt typed while Claude
/// was busy, recorded only here, or a background task's notice) and a file
/// or folder named with `@`.
fn attachment_body(offset: u64, v: &Value) -> NodeBody {
    let Some(att) = v.get("attachment") else { return NodeBody::Other };
    let (key, dir) = match s(att, "type") {
        Some("queued_command") => ("", false),
        Some("file" | "already_read_file" | "pdf_reference") => ("filename", false),
        Some("directory") => ("path", true),
        _ => return NodeBody::Other,
    };
    if !key.is_empty() {
        return match s(att, "displayPath").or_else(|| s(att, key)).filter(|p| !p.is_empty()) {
            Some(p) => NodeBody::Mentioned { path: prefix(p, MAX_MENTION).to_string(), dir },
            None => NodeBody::Other,
        };
    }
    let Some(prompt) = att.get("prompt") else { return NodeBody::Other };
    let kind = att.get("origin").and_then(|o| s(o, "kind"));
    let mode = s(att, "commandMode");
    if kind == Some("task-notification") || mode == Some("task-notification") {
        return notice_body(prompt);
    }
    if mode != Some("prompt") || written_by_system(kind, None) {
        return NodeBody::Other;
    }
    match user_body(offset, prompt) {
        b @ (NodeBody::UserText { .. } | NodeBody::Notice { .. }) => b,
        _ => NodeBody::Other,
    }
}

/// The content array a line's image ordinals count in: `message.content`,
/// or a queued prompt's `attachment.prompt`.
pub(super) fn image_content(v: &Value) -> Option<&Vec<Value>> {
    let content = match s(v, "type") {
        Some("attachment") => v.get("attachment").and_then(|a| a.get("prompt")),
        _ => v.get("message").and_then(|m| m.get("content")),
    };
    content.and_then(Value::as_array)
}

/// An assistant line. Claude Code writes one content block per assistant
/// line; a mixed text + tool_use line (never seen in real transcripts) keeps
/// its text.
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
            let input = b.get("input").unwrap_or(&Value::Null);
            (s(b, "id").unwrap_or("").to_string(), name.to_string(), tool_target(name, input))
        })
        .collect();
    let texts: Vec<&str> = blocks
        .iter()
        .filter(|b| s(b, "type") == Some("text"))
        .filter_map(|b| s(b, "text"))
        .filter(|t| !t.is_empty())
        .collect();
    if !texts.is_empty() {
        let (text, truncated) = cap_text(&texts.join("\n\n"));
        return NodeBody::AssistantText { text, truncated };
    }
    if !uses.is_empty() {
        return NodeBody::ToolUses(uses);
    }
    let thoughts: Vec<&str> = blocks
        .iter()
        .filter(|b| s(b, "type") == Some("thinking"))
        .filter_map(|b| s(b, "thinking"))
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .collect();
    if !thoughts.is_empty() {
        let (text, truncated) = cap_text(&thoughts.join("\n\n"));
        return NodeBody::Thinking { text, truncated };
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
        (Some("user"), _) => content.map_or(NodeBody::Other, |c| user_line_body(offset, &v, c)),
        (Some("attachment"), _) => attachment_body(offset, &v),
        (Some("assistant"), _) => content.map_or(NodeBody::Other, assistant_body),
        (Some("system"), _) if s(&v, "subtype") == Some("compact_boundary") => NodeBody::CompactBoundary,
        (Some("system"), _) if s(&v, "subtype") == Some("turn_duration") => v
            .get("durationMs")
            .and_then(Value::as_u64)
            .map_or(NodeBody::Other, |duration_ms| NodeBody::TurnDone { duration_ms }),
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
        offset,
        message_id: match s(&v, "type") {
            Some("assistant") => v.get("message").and_then(|m| s(m, "id")).map(str::to_string),
            _ => None,
        },
    })
}
