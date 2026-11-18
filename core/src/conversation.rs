//! Conversation nodes parsed from transcript lines, and on-demand image reading.
use crate::transcript::{tool_target, TranscriptReader};
use chrono::DateTime;
use serde::Serialize;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

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
    /// A line the system wrote into the conversation (a background task
    /// finished): its summary, empty when it has none.
    Notice { text: String },
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

/// Longest prefix of `t` of at most `max` bytes that ends on a char boundary.
fn prefix(t: &str, max: usize) -> &str {
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

/// A text block the user never typed: empty once its system reminders are
/// gone, or local command output / a caveat / shell output.
fn is_hidden_text(t: &str) -> bool {
    let t = t.trim_start();
    t.is_empty()
        || ["<local-command-stdout>", "<local-command-caveat>", "<bash-stdout>", "<bash-stderr>"]
            .iter()
            .any(|p| t.starts_with(p))
}

/// `t` without its `<system-reminder>…</system-reminder>` blocks (Claude
/// Code adds them to the user's text; an unclosed one runs to the end).
/// Text on both sides of a removed block keeps one run of the whitespace
/// around it.
fn strip_reminders(t: &str) -> String {
    const OPEN: &str = "<system-reminder>";
    const CLOSE: &str = "</system-reminder>";
    if !t.contains(OPEN) {
        return t.to_string();
    }
    let mut out = String::with_capacity(t.len());
    let mut rest = t;
    while let Some(at) = rest.find(OPEN) {
        out.push_str(&rest[..at]);
        rest = match rest[at + OPEN.len()..].find(CLOSE) {
            Some(end) => &rest[at + OPEN.len() + end + CLOSE.len()..],
            None => "",
        };
        let after = rest.trim_start();
        if !out.ends_with(char::is_whitespace) && !out.is_empty() {
            out.push_str(&rest[..rest.len() - after.len()]);
        }
        rest = after;
    }
    out.push_str(rest);
    out.trim_end().to_string()
}

/// `t` without `<pasted_content …>` / `</pasted_content>` wrappers (Claude
/// Code keeps a long paste in them); what they wrap stays.
fn strip_pasted(t: &str) -> String {
    const OPEN: &str = "<pasted_content";
    const CLOSE: &str = "</pasted_content>";
    let mut out = String::with_capacity(t.len());
    let mut rest = t;
    while let Some(at) = rest.find('<') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        if let Some(after) = rest.strip_prefix(CLOSE) {
            rest = after;
        } else if let Some(after) = rest.strip_prefix(OPEN).filter(|a| a.starts_with([' ', '>'])) {
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
    strip_pasted(t)
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

/// An attachment line. Only `queued_command` shows: a prompt typed while
/// Claude was busy (recorded only here) or a background task's notice.
fn attachment_body(offset: u64, v: &Value) -> NodeBody {
    let Some(att) = v.get("attachment").filter(|a| s(a, "type") == Some("queued_command")) else {
        return NodeBody::Other;
    };
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
fn image_content(v: &Value) -> Option<&Vec<Value>> {
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

fn valid_b64(d: &str) -> bool {
    d.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'=' | b'\r' | b'\n'))
}

/// Re-reads the line at `offset` and returns `(media_type, base64)` of the
/// image with the given ordinal. Any mismatch means the file changed: `Gone`.
pub fn read_image(path: &Path, offset: u64, ordinal: u32) -> Result<(String, String), ImageError> {
    read_image_limited(path, offset, ordinal, MAX_LINE)
}

/// [`read_image`] with the longest accepted line passed in (tests).
fn read_image_limited(path: &Path, offset: u64, ordinal: u32, max_line: usize) -> Result<(String, String), ImageError> {
    let mut f = File::open(path).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => ImageError::Gone,
        _ => ImageError::Io,
    })?;
    f.seek(SeekFrom::Start(offset)).map_err(|_| ImageError::Gone)?;
    let mut buf = Vec::new();
    // Up to the limit plus the newline, as the transcript reader accepts it.
    BufReader::new(f.take(max_line as u64 + 1))
        .read_until(b'\n', &mut buf)
        .map_err(|_| ImageError::Io)?;
    let line = buf.strip_suffix(b"\n").unwrap_or(&buf);
    if line.len() > max_line {
        return Err(ImageError::Gone);
    }
    let line = std::str::from_utf8(line).map_err(|_| ImageError::Gone)?;
    let v: Value = serde_json::from_str(line.trim_end()).map_err(|_| ImageError::Gone)?;
    let content = image_content(&v).ok_or(ImageError::Gone)?;
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

/// One tool call of a [`ConvItem::Steps`] group.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Step {
    pub tool: String,
    pub target: String,
    /// Its tool result is on the chain.
    pub done: bool,
    pub is_error: bool,
    pub images: Vec<ImageRef>,
}

/// One displayed entry of the current conversation branch.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum ConvItem {
    User { text: String, truncated: bool, images: Vec<ImageRef>, at_ms: i64 },
    Assistant { text: String, truncated: bool, at_ms: i64 },
    Steps { steps: Vec<Step>, at_ms: i64 },
    Compacted { at_ms: i64 },
    /// A one-line note from the system (a background task finished); an
    /// empty text means it carried no summary.
    Notice { text: String, at_ms: i64 },
}

/// Items `start..start + items.len()` of the `total` items of the branch
/// identified by `epoch`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConvPage {
    pub epoch: u32,
    pub total: u32,
    pub start: u32,
    pub items: Vec<ConvItem>,
}

/// Items per page.
pub const PAGE: u32 = 100;

/// Source of every epoch in the process: a conversation dropped from a cache
/// and opened again never repeats an epoch the UI may still hold.
static EPOCHS: AtomicU32 = AtomicU32::new(0);

fn next_epoch() -> u32 {
    EPOCHS.fetch_add(1, Ordering::Relaxed)
}

/// The current branch of one transcript, kept up to date incrementally.
pub struct Conversation {
    path: PathBuf,
    reader: TranscriptReader,
    nodes: HashMap<String, Node>,
    leaf: Option<String>,
    /// Uuids of the displayed nodes, in file order.
    shown: Vec<String>,
    items: Vec<ConvItem>,
    images: HashSet<(u64, u32)>,
    epoch: u32,
    /// How often the items were rebuilt (tests).
    #[cfg(test)]
    rebuilds: usize,
}

impl Conversation {
    pub fn new(path: PathBuf) -> Self {
        Self {
            reader: TranscriptReader::new(path.clone()),
            path,
            nodes: HashMap::new(),
            leaf: None,
            shown: Vec::new(),
            items: Vec::new(),
            images: HashSet::new(),
            epoch: next_epoch(),
            #[cfg(test)]
            rebuilds: 0,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Reads the lines appended since the last call. A shrunk file clears
    /// everything and starts a new epoch. A new epoch also starts when the
    /// displayed nodes no longer extend the previous ones (rewind, edit,
    /// retry), or when an item before the previous last one changed (a late
    /// tool result), since `since` only resends from the last item. Lines
    /// delivered before an io error are kept.
    pub fn update(&mut self) -> io::Result<()> {
        let mut reset = false;
        let mut fresh = Vec::new();
        let res = self.reader.read_lines_at(
            || reset = true,
            |at, l| fresh.extend(parse_node(at, l)),
        );
        if reset {
            self.nodes.clear();
            self.leaf = None;
            self.shown.clear();
            self.items.clear();
            self.epoch = next_epoch();
        }
        // Hidden lines alone change nothing shown: they never move the leaf.
        let changed = reset || fresh.iter().any(|n| n.body != NodeBody::Other);
        for n in fresh {
            if n.body != NodeBody::Other {
                self.leaf = Some(n.uuid.clone());
            }
            self.nodes.insert(n.uuid.clone(), n);
        }
        if changed {
            self.rebuild();
        }
        res
    }

    fn rebuild(&mut self) {
        #[cfg(test)]
        {
            self.rebuilds += 1;
        }
        let shown = shown_nodes(&self.nodes, current_chain(&self.nodes, self.leaf.as_deref()));
        // Hidden nodes take no part in the prefix check below: one joining
        // the middle (a side line of a shown API message) shows nothing.
        let uuids: Vec<String> = shown
            .iter()
            .filter_map(|l| match l {
                Link::Node(n) if n.body != NodeBody::Other => Some(n.uuid.clone()),
                _ => None,
            })
            .collect();
        let old = std::mem::replace(&mut self.items, group(&shown));
        let keep = old.len().saturating_sub(1);
        if !uuids.starts_with(&self.shown) || self.items.get(..keep) != old.get(..keep) {
            self.epoch = next_epoch();
        }
        self.images.clear();
        for item in &self.items {
            match item {
                ConvItem::User { images, .. } => {
                    for r in images {
                        self.images.insert((r.offset, r.ordinal));
                    }
                }
                ConvItem::Steps { steps, .. } => {
                    for r in steps.iter().flat_map(|s| &s.images) {
                        self.images.insert((r.offset, r.ordinal));
                    }
                }
                _ => {}
            }
        }
        self.shown = uuids;
    }

    fn total(&self) -> u32 {
        self.items.len() as u32
    }

    fn slice(&self, start: u32, end: u32) -> ConvPage {
        ConvPage {
            epoch: self.epoch,
            total: self.total(),
            start,
            items: self.items[start as usize..end as usize].to_vec(),
        }
    }

    /// The (at most) `PAGE` items ending before index `before` (default: the end).
    pub fn page(&self, before: Option<u32>) -> ConvPage {
        let end = before.unwrap_or(self.total()).min(self.total());
        self.slice(end.saturating_sub(PAGE), end)
    }

    /// Items from index `from` on when `epoch` is still current; otherwise
    /// the newest page (the caller then replaces what it has).
    pub fn since(&self, epoch: u32, from: u32) -> ConvPage {
        if epoch != self.epoch {
            return self.page(None);
        }
        self.slice(from.min(self.total()), self.total())
    }

    /// True when the image is shown somewhere on the current branch.
    pub fn has_image(&self, offset: u64, ordinal: u32) -> bool {
        self.images.contains(&(offset, ordinal))
    }
}

enum Link<'a> {
    Node(&'a Node),
    /// The walk crossed to a logical parent at this node, which is not a
    /// compact boundary; a divider goes right before it.
    Jump(&'a Node),
}

/// The chain from `leaf` back to its root, in file order. A missing parent
/// continues at the logical parent (compaction); a cycle ends the walk.
fn current_chain<'a>(nodes: &'a HashMap<String, Node>, leaf: Option<&str>) -> Vec<Link<'a>> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut cur = leaf.and_then(|u| nodes.get(u));
    while let Some(n) = cur {
        if !seen.insert(n.uuid.as_str()) {
            break;
        }
        out.push(Link::Node(n));
        cur = match n.parent.as_deref().and_then(|p| nodes.get(p)) {
            Some(p) => Some(p),
            None => {
                let lp = n
                    .logical_parent
                    .as_deref()
                    .and_then(|p| nodes.get(p))
                    .filter(|p| !seen.contains(p.uuid.as_str()));
                if lp.is_some() && n.body != NodeBody::CompactBoundary {
                    out.push(Link::Jump(n));
                }
                lp
            }
        };
    }
    out.reverse();
    out
}

/// The chain plus its parallel tool calls, in file order. Claude Code chains
/// the tool_use lines of one API message and hangs each result off its own
/// tool_use, so the walk from the leaf misses all but one of them. Added:
/// assistant lines sharing a `message.id` with a chain line, and tool results
/// answering a tool_use of any displayed line. Abandoned branches carry other
/// message ids and stay out.
fn shown_nodes<'a>(nodes: &'a HashMap<String, Node>, chain: Vec<Link<'a>>) -> Vec<Link<'a>> {
    let on_chain = |l: &Link<'a>| match l {
        Link::Node(n) => Some(*n),
        Link::Jump(_) => None,
    };
    let uuids: HashSet<&str> = chain.iter().filter_map(on_chain).map(|n| n.uuid.as_str()).collect();
    let msgs: HashSet<&str> = chain.iter().filter_map(on_chain).filter_map(|n| n.message_id.as_deref()).collect();
    let mut extra: Vec<&Node> = nodes
        .values()
        .filter(|n| !uuids.contains(n.uuid.as_str()))
        .filter(|n| n.message_id.as_deref().is_some_and(|m| msgs.contains(m)))
        .collect();
    let uses: HashSet<&str> = chain
        .iter()
        .filter_map(on_chain)
        .chain(extra.iter().copied())
        .flat_map(|n| match &n.body {
            NodeBody::ToolUses(u) => u.iter().map(|(id, _, _)| id.as_str()).collect(),
            _ => Vec::new(),
        })
        .collect();
    extra.extend(nodes.values().filter(|n| {
        !uuids.contains(n.uuid.as_str())
            && matches!(&n.body, NodeBody::ToolResults(r) if r.iter().any(|(id, _, _)| uses.contains(id.as_str())))
    }));
    let mut out = chain;
    out.extend(extra.into_iter().map(Link::Node));
    // A divider sorts right before the node it belongs to.
    out.sort_by_key(|l| match l {
        Link::Jump(n) => (n.offset, 0),
        Link::Node(n) => (n.offset, 1),
    });
    out
}

/// Groups displayed nodes into items (see [`ConvItem`]).
fn group(chain: &[Link]) -> Vec<ConvItem> {
    let mut items = Vec::new();
    // tool_use id -> (item index, step index)
    let mut steps_at: HashMap<&str, (usize, usize)> = HashMap::new();
    for link in chain {
        let n = match link {
            Link::Jump(n) => {
                items.push(ConvItem::Compacted { at_ms: n.at_ms });
                continue;
            }
            Link::Node(n) => n,
        };
        match &n.body {
            NodeBody::UserText { text, truncated, images } => items.push(ConvItem::User {
                text: text.clone(),
                truncated: *truncated,
                images: images.clone(),
                at_ms: n.at_ms,
            }),
            NodeBody::AssistantText { text, truncated } => match items.last_mut() {
                Some(ConvItem::Assistant { text: t, truncated: tr, .. }) => {
                    let room = MAX_TEXT.saturating_sub(t.len());
                    if room >= 2 + text.len() {
                        t.push_str("\n\n");
                        t.push_str(text);
                    } else {
                        if room > 2 {
                            t.push_str("\n\n");
                            t.push_str(prefix(text, room - 2));
                            // No bare blank line where the cut fell.
                            t.truncate(t.trim_end().len());
                        }
                        *tr = true;
                    }
                    *tr |= *truncated;
                }
                _ => items.push(ConvItem::Assistant { text: text.clone(), truncated: *truncated, at_ms: n.at_ms }),
            },
            NodeBody::ToolUses(uses) => {
                if !matches!(items.last(), Some(ConvItem::Steps { .. })) {
                    items.push(ConvItem::Steps { steps: Vec::new(), at_ms: n.at_ms });
                }
                let i = items.len() - 1;
                let Some(ConvItem::Steps { steps, .. }) = items.last_mut() else { unreachable!() };
                for (id, tool, target) in uses {
                    if !id.is_empty() {
                        steps_at.insert(id, (i, steps.len()));
                    }
                    steps.push(Step {
                        tool: tool.clone(),
                        target: target.clone(),
                        done: false,
                        is_error: false,
                        images: Vec::new(),
                    });
                }
            }
            NodeBody::ToolResults(results) => {
                for (id, is_error, images) in results {
                    let Some(&(i, j)) = steps_at.get(id.as_str()) else { continue };
                    if let ConvItem::Steps { steps, .. } = &mut items[i] {
                        // A result written again for the same call replaces the first.
                        let s = &mut steps[j];
                        s.done = true;
                        s.is_error = *is_error;
                        s.images = images.clone();
                    }
                }
            }
            NodeBody::CompactBoundary => items.push(ConvItem::Compacted { at_ms: n.at_ms }),
            NodeBody::Notice { text } => items.push(ConvItem::Notice { text: text.clone(), at_ms: n.at_ms }),
            NodeBody::Other => {}
        }
    }
    items
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
    fn keeps_offset_and_assistant_message_id() {
        let l = r#"{"type":"assistant","uuid":"x","message":{"id":"msg_1","content":[{"type":"text","text":"t"}]}}"#;
        let n = parse_node(42, l).unwrap();
        assert_eq!((n.offset, n.message_id.as_deref()), (42, Some("msg_1")));
        let n = parse_node(7, r#"{"type":"user","uuid":"y","message":{"id":"msg_2","content":"hi"}}"#).unwrap();
        assert_eq!((n.offset, n.message_id), (7, None));
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

    /// Hand-made transcript written line by line; line `n` gets timestamp
    /// `T0 + n` seconds.
    struct Tx {
        _dir: tempfile::TempDir,
        path: PathBuf,
        off: u64,
        n: i64,
    }

    const T0: i64 = 1_767_225_600_000;

    impl Tx {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("s.jsonl");
            File::create(&path).unwrap();
            Self { _dir: dir, path, off: 0, n: 0 }
        }

        /// Appends a line; returns (byte offset, at_ms).
        fn push(&mut self, mut v: Value) -> (u64, i64) {
            let at = T0 + self.n * 1000;
            v["timestamp"] = Value::from(chrono::DateTime::from_timestamp_millis(at).unwrap().to_rfc3339());
            self.n += 1;
            let line = format!("{v}\n");
            let mut f = std::fs::OpenOptions::new().append(true).open(&self.path).unwrap();
            f.write_all(line.as_bytes()).unwrap();
            let off = self.off;
            self.off += line.len() as u64;
            (off, at)
        }

        fn conv(&self) -> Conversation {
            let mut c = Conversation::new(self.path.clone());
            c.update().unwrap();
            c
        }
    }

    fn img() -> Value {
        serde_json::json!({"type":"image","source":{"type":"base64","media_type":"image/png","data":"AAAA"}})
    }

    fn line(kind: &str, uuid: &str, parent: Option<&str>, content: Value) -> Value {
        serde_json::json!({"type": kind, "uuid": uuid, "parentUuid": parent, "message": {"content": content}})
    }

    fn u(uuid: &str, parent: Option<&str>, text: &str) -> Value {
        line("user", uuid, parent, Value::from(text))
    }

    fn u_img(uuid: &str, parent: Option<&str>, text: &str) -> Value {
        line("user", uuid, parent, serde_json::json!([{"type":"text","text":text}, img()]))
    }

    fn a(uuid: &str, parent: &str, text: &str) -> Value {
        line("assistant", uuid, Some(parent), serde_json::json!([{"type":"text","text":text}]))
    }

    fn think(uuid: &str, parent: &str) -> Value {
        line("assistant", uuid, Some(parent), serde_json::json!([{"type":"thinking","thinking":"hm"}]))
    }

    fn tu(uuid: &str, parent: &str, id: &str, name: &str, input: Value) -> Value {
        line("assistant", uuid, Some(parent), serde_json::json!([{"type":"tool_use","id":id,"name":name,"input":input}]))
    }

    fn bash(uuid: &str, parent: &str, id: &str) -> Value {
        tu(uuid, parent, id, "Bash", serde_json::json!({"command": "ls"}))
    }

    fn tr(uuid: &str, parent: &str, id: &str, err: bool, images: usize) -> Value {
        let inner: Vec<Value> = (0..images).map(|_| img()).collect();
        line("user", uuid, Some(parent), serde_json::json!([{"type":"tool_result","tool_use_id":id,"is_error":err,"content":inner}]))
    }

    /// Tool use line that is part of API message `msg`.
    fn tu_msg(uuid: &str, parent: &str, id: &str, msg: &str) -> Value {
        let mut v = bash(uuid, parent, id);
        v["message"]["id"] = msg.into();
        v
    }

    fn kinds(items: &[ConvItem]) -> Vec<String> {
        items
            .iter()
            .map(|i| match i {
                ConvItem::User { text, .. } => format!("U:{text}"),
                ConvItem::Assistant { text, .. } => format!("A:{text}"),
                ConvItem::Steps { steps, .. } => format!("S{}", steps.len()),
                ConvItem::Compacted { .. } => "C".to_string(),
                ConvItem::Notice { text, .. } => format!("N:{text}"),
            })
            .collect()
    }

    fn all(c: &Conversation) -> Vec<ConvItem> {
        c.page(None).items
    }

    #[test]
    fn groups_text_and_steps() {
        let mut t = Tx::new();
        let (_, at_u) = t.push(u("u1", None, "hi"));
        let (_, at_a) = t.push(a("a1", "u1", "let me look"));
        t.push(think("th", "a1"));
        t.push(a("a2", "th", "more"));
        let (_, at_s) = t.push(bash("b1", "a2", "t1"));
        t.push(tr("r1", "b1", "t1", false, 0));
        t.push(think("th2", "r1"));
        t.push(bash("b2", "th2", "t2"));
        t.push(tr("r2", "b2", "t2", true, 0));
        t.push(tu("rd", "r2", "t3", "Read", serde_json::json!({"file_path": "/x/grid.jpg"})));
        t.push(tr("r3", "rd", "t3", false, 0));
        let (_, at_end) = t.push(a("a3", "r3", "done"));
        let items = all(&t.conv());
        assert_eq!(kinds(&items), ["U:hi", "A:let me look\n\nmore", "S3", "A:done"]);
        let ats: Vec<i64> = items
            .iter()
            .map(|i| match i {
                ConvItem::User { at_ms, .. }
                | ConvItem::Assistant { at_ms, .. }
                | ConvItem::Steps { at_ms, .. }
                | ConvItem::Compacted { at_ms }
                | ConvItem::Notice { at_ms, .. } => *at_ms,
            })
            .collect();
        assert_eq!(ats, [at_u, at_a, at_s, at_end]);
        let ConvItem::Steps { steps, .. } = &items[2] else { panic!() };
        let tools: Vec<_> = steps.iter().map(|s| (s.tool.as_str(), s.target.as_str(), s.done, s.is_error)).collect();
        assert_eq!(tools, [("Bash", "ls", true, false), ("Bash", "ls", true, true), ("Read", "grid.jpg", true, false)]);
    }

    #[test]
    fn user_message_breaks_steps_and_unknown_result_is_ignored() {
        let mut t = Tx::new();
        t.push(u("u1", None, "go"));
        t.push(bash("b1", "u1", "t1"));
        t.push(u("u2", Some("b1"), "stop"));
        t.push(tr("r0", "u2", "nope", true, 1));
        t.push(bash("b2", "r0", "t2"));
        let items = all(&t.conv());
        assert_eq!(kinds(&items), ["U:go", "S1", "U:stop", "S1"]);
        let ConvItem::Steps { steps, .. } = &items[1] else { panic!() };
        assert!(!steps[0].done);
        assert!(steps[0].images.is_empty());
    }

    #[test]
    fn tool_result_marks_step_done_with_image() {
        let mut t = Tx::new();
        t.push(u("u1", None, "show"));
        t.push(tu("rd", "u1", "t1", "Read", serde_json::json!({"file_path": "/x/grid.jpg"})));
        let (off, _) = t.push(tr("r1", "rd", "t1", false, 1));
        let items = all(&t.conv());
        let ConvItem::Steps { steps, .. } = &items[1] else { panic!("{items:?}") };
        assert_eq!(
            steps[0],
            Step {
                tool: "Read".into(),
                target: "grid.jpg".into(),
                done: true,
                is_error: false,
                images: vec![ImageRef { offset: off, ordinal: 0, media_type: "image/png".into() }],
            }
        );
    }

    #[test]
    fn rewind_shows_current_branch_and_bumps_epoch() {
        let mut t = Tx::new();
        t.push(u("u0", None, "start"));
        t.push(a("a0", "u0", "ok"));
        t.push(u("ua", Some("a0"), "A"));
        t.push(a("aa", "ua", "reply"));
        let mut c = t.conv();
        assert_eq!(kinds(&all(&c)), ["U:start", "A:ok", "U:A", "A:reply"]);
        let e0 = c.page(None).epoch;
        t.push(u("ub", Some("a0"), "B"));
        c.update().unwrap();
        assert_eq!(kinds(&all(&c)), ["U:start", "A:ok", "U:B"]);
        assert_ne!(c.page(None).epoch, e0);
        // Extending the branch keeps the epoch.
        let e1 = c.page(None).epoch;
        t.push(a("ab", "ub", "reply B"));
        c.update().unwrap();
        assert_eq!(kinds(&all(&c)), ["U:start", "A:ok", "U:B", "A:reply B"]);
        assert_eq!(c.page(None).epoch, e1);
    }

    #[test]
    fn compaction_keeps_history_with_divider() {
        let mut t = Tx::new();
        t.push(u("u1", None, "before"));
        t.push(a("a1", "u1", "old reply"));
        let mut c = t.conv();
        let e0 = c.page(None).epoch;
        let mut cb = line("system", "cb", None, Value::Null);
        cb["subtype"] = "compact_boundary".into();
        cb["logicalParentUuid"] = "a1".into();
        let (_, at_cb) = t.push(cb);
        let mut sum = u("sum", Some("cb"), "summary text");
        sum["isCompactSummary"] = true.into();
        t.push(sum);
        t.push(u("u2", Some("sum"), "after"));
        c.update().unwrap();
        let items = all(&c);
        assert_eq!(kinds(&items), ["U:before", "A:old reply", "C", "U:after"]);
        assert_eq!(items[2], ConvItem::Compacted { at_ms: at_cb });
        assert_eq!(c.page(None).epoch, e0);
    }

    #[test]
    fn jump_without_boundary_node_gets_divider() {
        let mut t = Tx::new();
        t.push(u("u1", None, "before"));
        let mut n = u("u2", None, "after");
        n["logicalParentUuid"] = "u1".into();
        let (_, at) = t.push(n);
        let items = all(&t.conv());
        assert_eq!(kinds(&items), ["U:before", "C", "U:after"]);
        assert_eq!(items[1], ConvItem::Compacted { at_ms: at });
    }

    #[test]
    fn parent_cycle_terminates() {
        let mut t = Tx::new();
        t.push(u("x", Some("y"), "one"));
        t.push(u("y", Some("x"), "two"));
        assert_eq!(kinds(&all(&t.conv())), ["U:one", "U:two"]);
    }

    #[test]
    fn since_resends_last_item_and_newer() {
        let mut t = Tx::new();
        t.push(u("u1", None, "go"));
        t.push(bash("b1", "u1", "t1"));
        let mut c = t.conv();
        let p = c.page(None);
        assert_eq!(p.total, 2);
        let last = p.total - 1;
        t.push(tr("r1", "b1", "t1", false, 0));
        t.push(bash("b2", "r1", "t2"));
        c.update().unwrap();
        let s = c.since(p.epoch, last);
        assert_eq!(s.start, last);
        assert_eq!(s.total, 2);
        assert_eq!(kinds(&s.items), ["S2"]);
        let s = c.since(p.epoch, 99);
        assert_eq!((s.start, s.items.len()), (2, 0));
    }

    #[test]
    fn since_with_old_epoch_returns_newest_page() {
        let mut t = Tx::new();
        t.push(u("u0", None, "start"));
        t.push(u("ua", Some("u0"), "A"));
        let mut c = t.conv();
        let e0 = c.page(None).epoch;
        t.push(u("ub", Some("u0"), "B"));
        c.update().unwrap();
        let s = c.since(e0, 1);
        assert_ne!(s.epoch, e0);
        assert_eq!(s, c.page(None));
        assert_eq!((s.start, s.total), (0, 2));
        assert_eq!(kinds(&s.items), ["U:start", "U:B"]);
    }

    #[test]
    fn truncated_file_resets_and_bumps_epoch() {
        let mut t = Tx::new();
        t.push(u("u0", None, "old"));
        t.push(u("u1", Some("u0"), "old 2"));
        let mut c = t.conv();
        let e0 = c.page(None).epoch;
        std::fs::write(&t.path, "").unwrap();
        c.update().unwrap();
        assert_eq!(c.page(None).total, 0);
        assert_ne!(c.page(None).epoch, e0);
    }

    #[test]
    fn page_returns_newest_100_of_5000() {
        let mut t = Tx::new();
        let mut text = String::new();
        for i in 0..5000 {
            let parent = (i > 0).then(|| format!("u{}", i - 1));
            text.push_str(&format!("{}\n", u(&format!("u{i}"), parent.as_deref(), &format!("m{i}"))));
        }
        std::fs::write(&t.path, text).unwrap();
        t.off = 0;
        let c = t.conv();
        let p = c.page(None);
        assert_eq!((p.total, p.start, p.items.len()), (5000, 4900, 100));
        assert_eq!(kinds(&p.items[..1]), ["U:m4900"]);
        let p = c.page(Some(4900));
        assert_eq!((p.total, p.start, p.items.len()), (5000, 4800, 100));
        let p = c.page(Some(50));
        assert_eq!((p.start, p.items.len()), (0, 50));
        let p = c.page(Some(9999));
        assert_eq!((p.start, p.items.len()), (4900, 100));
    }

    #[test]
    fn has_image_only_for_current_chain() {
        let mut t = Tx::new();
        t.push(u("u0", None, "start"));
        let (off_a, _) = t.push(u_img("ua", Some("u0"), "A"));
        t.push(tu("rd", "ua", "t1", "Read", serde_json::json!({"file_path": "/x/a.png"})));
        let (off_r, _) = t.push(tr("r1", "rd", "t1", false, 2));
        let mut c = t.conv();
        assert!(c.has_image(off_a, 0));
        assert!(c.has_image(off_r, 0));
        assert!(c.has_image(off_r, 1));
        assert!(!c.has_image(off_r, 2));
        let (off_b, _) = t.push(u_img("ub", Some("u0"), "B"));
        c.update().unwrap();
        assert!(!c.has_image(off_a, 0));
        assert!(!c.has_image(off_r, 0));
        assert!(c.has_image(off_b, 0));
    }

    #[test]
    fn serializes_items_with_kind_tag() {
        let item = ConvItem::Steps {
            steps: vec![Step { tool: "Bash".into(), target: "ls".into(), done: true, is_error: false, images: vec![] }],
            at_ms: 5,
        };
        assert_eq!(
            serde_json::to_value(&item).unwrap(),
            serde_json::json!({"kind":"steps","atMs":5,"steps":[{"tool":"Bash","target":"ls","done":true,"isError":false,"images":[]}]})
        );
        let page = ConvPage { epoch: 1, total: 0, start: 0, items: vec![ConvItem::Compacted { at_ms: 2 }] };
        assert_eq!(
            serde_json::to_value(&page).unwrap(),
            serde_json::json!({"epoch":1,"total":0,"start":0,"items":[{"kind":"compacted","atMs":2}]})
        );
    }

    #[test]
    fn joined_assistant_text_is_capped() {
        let mut t = Tx::new();
        t.push(u("u1", None, "go"));
        t.push(a("a1", "u1", &"x".repeat(40 * 1024)));
        t.push(a("a2", "a1", &"ç".repeat(20 * 1024)));
        let items = all(&t.conv());
        let ConvItem::Assistant { text, truncated, .. } = &items[1] else { panic!() };
        assert!(text.len() <= MAX_TEXT, "{}", text.len());
        assert!(text.len() > 40 * 1024);
        assert!(*truncated);
    }

    #[test]
    fn parallel_tool_calls_on_side_branch_are_shown() {
        // Real shape: the second tool_use of one API message hangs off the
        // first, its result off it; the conversation continues from result 1.
        let mut t = Tx::new();
        t.push(u("u1", None, "go"));
        t.push(tu_msg("t1", "u1", "c1", "msg_a"));
        t.push(tu_msg("t2", "t1", "c2", "msg_a"));
        // Poll after every line, as the live view does.
        let mut c = t.conv();
        let e0 = c.page(None).epoch;
        let mut step = |t: &mut Tx, v: Value| {
            let (off, _) = t.push(v);
            c.update().unwrap();
            assert_eq!(c.page(None).epoch, e0, "a live update must not reload");
            off
        };
        step(&mut t, tr("r1", "t1", "c1", false, 0));
        let off_r2 = step(&mut t, tr("r2", "t2", "c2", false, 1));
        step(&mut t, a("a1", "r1", "both done"));
        let items = all(&c);
        assert_eq!(kinds(&items), ["U:go", "S2", "A:both done"]);
        let ConvItem::Steps { steps, .. } = &items[1] else { panic!() };
        assert!(steps.iter().all(|s| s.done));
        assert!(steps[0].images.is_empty());
        assert_eq!(steps[1].images, vec![ImageRef { offset: off_r2, ordinal: 0, media_type: "image/png".into() }]);
        assert!(c.has_image(off_r2, 0));
    }

    #[test]
    fn abandoned_branch_with_other_message_id_stays_hidden() {
        let mut t = Tx::new();
        t.push(u("u1", None, "go"));
        t.push(tu_msg("t1", "u1", "c1", "msg_old"));
        t.push(tr("r1", "t1", "c1", false, 1));
        // Retry: a new API message from the same user message.
        t.push(tu_msg("t2", "u1", "c2", "msg_new"));
        t.push(tr("r2", "t2", "c2", false, 0));
        let c = t.conv();
        let items = all(&c);
        assert_eq!(kinds(&items), ["U:go", "S1"]);
        let ConvItem::Steps { steps, .. } = &items[1] else { panic!() };
        assert!(steps[0].done && steps[0].images.is_empty());
        assert!(c.images.is_empty());
    }

    #[test]
    fn late_result_on_older_steps_bumps_epoch() {
        let mut t = Tx::new();
        t.push(u("u1", None, "go"));
        t.push(bash("b1", "u1", "t1"));
        t.push(a("a1", "b1", "waiting"));
        let mut c = t.conv();
        let e0 = c.page(None).epoch;
        t.push(tr("r1", "a1", "t1", false, 0));
        c.update().unwrap();
        let e1 = c.page(None).epoch;
        assert_ne!(e1, e0);
        let ConvItem::Steps { steps, .. } = &all(&c)[1] else { panic!() };
        assert!(steps[0].done);
        // A change to the last item alone keeps the epoch.
        t.push(a("a2", "r1", "more"));
        c.update().unwrap();
        assert_eq!(c.page(None).epoch, e1);
        assert_eq!(kinds(&all(&c)), ["U:go", "S1", "A:waiting\n\nmore"]);
    }

    fn text_of(n: Node) -> String {
        match n.body {
            NodeBody::UserText { text, .. } => text,
            other => panic!("not user text: {other:?}"),
        }
    }

    const TASK_NOTE: &str = "<task-notification>\n<task-id>b1</task-id>\n<tool-use-id>toolu_1</tool-use-id>\n<output-file>/tmp/x.output</output-file>\n<status>completed</status>\n<summary>  Background command \"ls\" completed (exit code 0)\n</summary>\n</task-notification>";

    #[test]
    fn task_notification_user_line_becomes_notice() {
        let origin = r#","origin":{"kind":"task-notification","producer":"session-task"},"promptSource":"system","turnOrigin":"task_notification""#;
        let n = parse_node(0, &user(&serde_json::to_string(TASK_NOTE).unwrap(), origin)).unwrap();
        assert_eq!(n.body, NodeBody::Notice { text: "Background command \"ls\" completed (exit code 0)".into() });
        // No summary: the UI shows its own text.
        let bare = "<task-notification>\n<task-id>b1</task-id>\n<status>completed</status>\n</task-notification>";
        let n = parse_node(0, &user(&serde_json::to_string(bare).unwrap(), origin)).unwrap();
        assert_eq!(n.body, NodeBody::Notice { text: String::new() });
        // Older lines carry no origin.
        let n = parse_node(0, &user(&serde_json::to_string(TASK_NOTE).unwrap(), "")).unwrap();
        assert!(matches!(n.body, NodeBody::Notice { .. }), "{:?}", n.body);
    }

    #[test]
    fn system_written_user_lines_are_hidden() {
        let peer = r#","origin":{"kind":"peer","from":"a1"},"promptSource":"system","turnOrigin":"peer""#;
        assert_eq!(parse_node(0, &user(r#""<peer-message>hi</peer-message>""#, peer)).unwrap().body, NodeBody::Other);
        let sys = r#","promptSource":"system""#;
        assert_eq!(parse_node(0, &user(r#""note""#, sys)).unwrap().body, NodeBody::Other);
        let human = r#","origin":{"kind":"human"},"promptSource":"typed","turnOrigin":"human""#;
        assert_eq!(text_of(parse_node(0, &user(r#""selam""#, human)).unwrap()), "selam");
        // Tool results never carry an origin and stay results.
        let r = r#"[{"type":"tool_result","tool_use_id":"t1","content":"ok"}]"#;
        assert!(matches!(parse_node(0, &user(r, sys)).unwrap().body, NodeBody::ToolResults(_)));
    }

    #[test]
    fn bash_mode_lines_and_pasted_content() {
        assert_eq!(text_of(parse_node(0, &user(r#""<bash-input>git status</bash-input>""#, "")).unwrap()), "! git status");
        for out in [
            r#""<bash-stdout>On branch main</bash-stdout><bash-stderr></bash-stderr>""#,
            r#""<bash-stderr>boom</bash-stderr>""#,
        ] {
            assert_eq!(parse_node(0, &user(out, "")).unwrap().body, NodeBody::Other, "{out}");
        }
        let pasted = r#""headers:\n\n<pasted_content id=\"d1f4\">\nA | B\n</pasted_content>\nthanks""#;
        assert_eq!(text_of(parse_node(0, &user(pasted, "")).unwrap()), "headers:\n\n\nA | B\n\nthanks");
        let two = r#""<pasted_content id=\"a\">x</pasted_content> and <pasted_content>y</pasted_content> <pasted_contentz>""#;
        assert_eq!(text_of(parse_node(0, &user(two, "")).unwrap()), "x and y <pasted_contentz>");
    }

    fn queued(prompt: Value, mode: &str, origin: Value) -> String {
        serde_json::json!({
            "type": "attachment", "uuid": "q", "parentUuid": "p", "timestamp": "2026-01-01T00:00:00Z",
            "attachment": {"type": "queued_command", "prompt": prompt, "commandMode": mode, "origin": origin, "humanTurn": true},
        })
        .to_string()
    }

    #[test]
    fn queued_prompt_becomes_user_text() {
        let human = serde_json::json!({"kind": "human"});
        let n = parse_node(0, &queued("typed while busy".into(), "prompt", human.clone())).unwrap();
        assert_eq!((n.uuid.as_str(), n.parent.as_deref()), ("q", Some("p")));
        assert_eq!(n.at_ms, 1_767_225_600_000);
        assert_eq!(text_of(n), "typed while busy");
        // Origin missing: still the user's.
        assert_eq!(text_of(parse_node(0, &queued("old".into(), "prompt", Value::Null)).unwrap()), "old");
        let blocks = serde_json::json!([{"type":"text","text":"see"}, img()]);
        let n = parse_node(9, &queued(blocks, "prompt", human.clone())).unwrap();
        assert_eq!(
            n.body,
            NodeBody::UserText {
                text: "see".into(),
                truncated: false,
                images: vec![ImageRef { offset: 9, ordinal: 0, media_type: "image/png".into() }],
            }
        );
        let note = queued(TASK_NOTE.into(), "task-notification", serde_json::json!({"kind":"task-notification"}));
        assert_eq!(
            parse_node(0, &note).unwrap().body,
            NodeBody::Notice { text: "Background command \"ls\" completed (exit code 0)".into() }
        );
        let peer = queued("hi".into(), "prompt", serde_json::json!({"kind":"peer"}));
        assert_eq!(parse_node(0, &peer).unwrap().body, NodeBody::Other);
        let bash = queued("ls".into(), "bash", human);
        assert_eq!(parse_node(0, &bash).unwrap().body, NodeBody::Other);
    }

    #[test]
    fn read_image_finds_queued_prompt_image() {
        let mut t = Tx::new();
        t.push(u("u1", None, "go"));
        let prompt = serde_json::json!([{"type":"text","text":"a"}, {"type":"image","source":{"type":"base64","media_type":"image/jpeg","data":"/9j/"}}, img()]);
        let (off, _) = t.push(serde_json::from_str(&queued(prompt, "prompt", serde_json::json!({"kind":"human"}))).unwrap());
        let mut c = Conversation::new(t.path.clone());
        c.update().unwrap();
        assert!(c.has_image(off, 0) && c.has_image(off, 1));
        assert_eq!(read_image(&t.path, off, 0), Ok(("image/jpeg".into(), "/9j/".into())));
        assert_eq!(read_image(&t.path, off, 1), Ok(("image/png".into(), "AAAA".into())));
        assert_eq!(read_image(&t.path, off, 2), Err(ImageError::Gone));
    }

    #[test]
    fn notice_and_queued_prompt_take_their_place_in_the_conversation() {
        let mut t = Tx::new();
        t.push(u("u1", None, "go"));
        t.push(a("a1", "u1", "working"));
        t.push(bash("b1", "a1", "t1"));
        t.push(tr("r1", "b1", "t1", false, 0));
        let mut note = u("n1", Some("r1"), TASK_NOTE);
        note["origin"] = serde_json::json!({"kind":"task-notification"});
        note["promptSource"] = "system".into();
        t.push(note);
        t.push(bash("b2", "n1", "t2"));
        let mut q: Value = serde_json::from_str(&queued("also this".into(), "prompt", serde_json::json!({"kind":"human"}))).unwrap();
        q["uuid"] = "q1".into();
        q["parentUuid"] = "b2".into();
        t.push(q);
        t.push(a("a2", "q1", "ok"));
        let items = all(&t.conv());
        assert_eq!(
            kinds(&items),
            ["U:go", "A:working", "S1", "N:Background command \"ls\" completed (exit code 0)", "S1", "U:also this", "A:ok"]
        );
    }

    #[test]
    fn serializes_notice() {
        let item = ConvItem::Notice { text: "done".into(), at_ms: 3 };
        assert_eq!(serde_json::to_value(&item).unwrap(), serde_json::json!({"kind":"notice","text":"done","atMs":3}));
    }

    #[test]
    fn repeated_tool_result_replaces_the_step_images() {
        let mut t = Tx::new();
        t.push(u("u1", None, "show"));
        t.push(tu("rd", "u1", "t1", "Read", serde_json::json!({"file_path": "/x/a.png"})));
        t.push(tr("r1", "rd", "t1", false, 1));
        let (off2, _) = t.push(tr("r2", "r1", "t1", false, 1));
        let c = t.conv();
        let ConvItem::Steps { steps, .. } = &all(&c)[1] else { panic!() };
        assert_eq!(steps[0].images, vec![ImageRef { offset: off2, ordinal: 0, media_type: "image/png".into() }]);
    }

    #[test]
    fn command_tags_mid_sentence_stay_text() {
        let text_of = |c: &str| {
            let NodeBody::UserText { text, .. } = parse_node(0, &user(c, "")).unwrap().body else { panic!() };
            text
        };
        let mid = r#""how do I write <command-name>/model</command-name> in a hook?""#;
        assert_eq!(text_of(mid), "how do I write <command-name>/model</command-name> in a hook?");
        let lead = r#""  \n<command-name>/clear</command-name>""#;
        assert_eq!(text_of(lead), "/clear");
    }

    #[test]
    fn system_reminders_inside_human_text_are_stripped() {
        let text_of = |c: &str| {
            let NodeBody::UserText { text, .. } = parse_node(0, &user(c, "")).unwrap().body else { panic!() };
            text
        };
        let inline = r#""fix this<system-reminder>\nsecret note\n</system-reminder> please""#;
        assert_eq!(text_of(inline), "fix this please");
        let blocks = r#"[{"type":"text","text":"<system-reminder>x</system-reminder>"},{"type":"text","text":"real"}]"#;
        assert_eq!(text_of(blocks), "real");
        let leading = r#""<system-reminder>x</system-reminder>\nhello""#;
        assert_eq!(text_of(leading), "hello");
        let lines = r#""line one<system-reminder>x</system-reminder>\nline two\n\n<system-reminder>y</system-reminder>\n\nend""#;
        assert_eq!(text_of(lines), "line one\nline two\n\nend");
        let unclosed = r#""hi <system-reminder>never closed""#;
        assert_eq!(text_of(unclosed), "hi");
        // Only reminders: hidden as before.
        let only = r#""<system-reminder>x</system-reminder>""#;
        assert_eq!(parse_node(0, &user(only, "")).unwrap().body, NodeBody::Other);
    }

    #[test]
    fn cut_text_has_no_trailing_blank_line() {
        let first = "x".repeat(MAX_TEXT - 2);
        let c = serde_json::json!([{"type":"text","text":first}, {"type":"text","text":"y".repeat(10)}]);
        let NodeBody::UserText { text, truncated, .. } = parse_node(0, &user(&c.to_string(), "")).unwrap().body else {
            panic!()
        };
        assert!(truncated);
        assert_eq!(text, first);
        let mut t = Tx::new();
        t.push(u("u1", None, "go"));
        t.push(a("a1", "u1", &"x".repeat(MAX_TEXT - 3)));
        t.push(a("a2", "a1", "çç"));
        let ConvItem::Assistant { text, truncated, .. } = &all(&t.conv())[1] else { panic!() };
        assert!(*truncated);
        assert_eq!(text.len(), MAX_TEXT - 3);
    }

    #[test]
    fn read_image_of_a_missing_file_is_gone() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(read_image(&dir.path().join("nope.jsonl"), 0, 0), Err(ImageError::Gone));
    }

    #[test]
    fn read_image_line_limit_matches_the_transcript_reader() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("s.jsonl");
        let line = r#"{"message":{"content":[{"type":"image","source":{"type":"base64","media_type":"image/png","data":"AAAA"}}]}}"#;
        std::fs::write(&p, format!("{line}\n")).unwrap();
        // A line of exactly the limit, plus its newline, is read.
        assert_eq!(read_image_limited(&p, 0, 0, line.len()), Ok(("image/png".into(), "AAAA".into())));
        // One byte over the limit is refused.
        assert_eq!(read_image_limited(&p, 0, 0, line.len() - 1), Err(ImageError::Gone));
        // Without a final newline the limit is the same.
        std::fs::write(&p, line).unwrap();
        assert_eq!(read_image_limited(&p, 0, 0, line.len()), Ok(("image/png".into(), "AAAA".into())));
        assert_eq!(read_image_limited(&p, 0, 0, line.len() - 1), Err(ImageError::Gone));
    }

    #[test]
    fn nested_tool_result_images_count_in_one_order() {
        let png = |d: &str| serde_json::json!({"type":"image","source":{"type":"base64","media_type":"image/png","data":d}});
        let content = serde_json::json!([
            {"type":"tool_result","tool_use_id":"a","content":[png("A0"), {"type":"tool_result","content":[png("A1")]}]},
            png("TOP"),
            {"type":"tool_result","tool_use_id":"b","content":[{"type":"text","text":"t"}, png("B0")]},
        ]);
        let mut t = Tx::new();
        let (off, _) = t.push(line("user", "r", None, content));
        let NodeBody::ToolResults(r) = parse_node(off, &std::fs::read_to_string(&t.path).unwrap()).unwrap().body else {
            panic!()
        };
        let ords: Vec<(&str, Vec<u32>)> = r.iter().map(|(id, _, im)| (id.as_str(), im.iter().map(|i| i.ordinal).collect())).collect();
        assert_eq!(ords, [("a", vec![0, 1]), ("b", vec![3])]);
        for (ord, data) in [(0, "A0"), (1, "A1"), (2, "TOP"), (3, "B0")] {
            assert_eq!(read_image(&t.path, off, ord), Ok(("image/png".into(), data.into())), "{ord}");
        }
    }

    #[test]
    fn hidden_lines_alone_do_not_rebuild() {
        let mut t = Tx::new();
        t.push(u("u1", None, "go"));
        let mut c = t.conv();
        let before = c.rebuilds;
        t.push(think("th", "u1"));
        c.update().unwrap();
        assert_eq!(c.rebuilds, before);
        t.push(a("a1", "th", "ok"));
        c.update().unwrap();
        assert_eq!(c.rebuilds, before + 1);
        assert_eq!(kinds(&all(&c)), ["U:go", "A:ok"]);
    }

    #[test]
    fn hidden_nodes_do_not_take_part_in_the_epoch_check() {
        let mut t = Tx::new();
        t.push(u("u1", None, "go"));
        t.push(tu_msg("t1", "u1", "c1", "msg_a"));
        // A hidden line of the next API message, written on a side branch.
        let mut th = think("thb", "u1");
        th["message"]["id"] = "msg_b".into();
        t.push(th);
        t.push(tr("r1", "t1", "c1", false, 0));
        let mut c = t.conv();
        let e0 = c.page(None).epoch;
        let mut a2 = a("a2", "r1", "done");
        a2["message"]["id"] = "msg_b".into();
        t.push(a2);
        c.update().unwrap();
        assert_eq!(kinds(&all(&c)), ["U:go", "S1", "A:done"]);
        assert_eq!(c.page(None).epoch, e0, "only a new item was added");
    }

    #[test]
    fn epochs_never_repeat_across_instances() {
        let mut t = Tx::new();
        t.push(u("u1", None, "go"));
        t.push(u("u2", Some("u1"), "x"));
        let mut c = t.conv();
        let first = c.page(None).epoch;
        t.push(u("u3", Some("u1"), "y"));
        c.update().unwrap();
        let bumped = c.page(None).epoch;
        assert_ne!(bumped, first);
        // A cache entry dropped and created again must not reuse an epoch the
        // UI may still hold.
        drop(c);
        let again = t.conv().page(None).epoch;
        assert!(again != first && again != bumped, "{first} {bumped} {again}");
    }
}
