//! Tests of the conversation module (hand-made transcripts only).
use super::image::read_image_limited;
use super::parse::MAX_TEXT;
use super::*;
use serde_json::Value;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

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
    let th = r#"{"type":"assistant","uuid":"x","message":{"content":[{"type":"thinking","thinking":" "}]}}"#;
    assert_eq!(parse_node(0, th).unwrap().body, NodeBody::Other, "thinking without text");
    let th = r#"{"type":"assistant","uuid":"x","message":{"content":[{"type":"thinking","thinking":"hm, the grid"}]}}"#;
    assert_eq!(parse_node(0, th).unwrap().body, NodeBody::Thinking { text: "hm, the grid".into(), truncated: false });
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
    line("assistant", uuid, Some(parent), serde_json::json!([{"type":"thinking","thinking":""}]))
}

fn think_text(uuid: &str, parent: &str, text: &str) -> Value {
    line("assistant", uuid, Some(parent), serde_json::json!([{"type":"thinking","thinking":text}]))
}

#[test]
fn thinking_with_text_is_an_item_and_runs_of_it_merge() {
    let mut t = Tx::new();
    t.push(u("u1", None, "hi"));
    t.push(think_text("th1", "u1", "first idea"));
    t.push(think("th0", "th1"));
    t.push(think_text("th2", "th0", "second idea"));
    t.push(a("a1", "th2", "answer"));
    t.push(think_text("th3", "a1", "after"));
    t.push(bash("b1", "th3", "t1"));
    let items = all(&t.conv());
    assert_eq!(kinds(&items), ["U:hi", "T:first idea\n\nsecond idea", "A:answer", "T:after", "S1"]);
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
            ConvItem::Thinking { text, .. } => format!("T:{text}"),
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
            | ConvItem::Thinking { at_ms, .. }
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
    // As Claude Code writes a paste sent from the message box: the closing tag
    // repeats the id, and blank lines surround the paste.
    let sent = r#""\n\n<pasted_content id=\"229b\">\na\nline1\n</pasted_content id=\"229b\">\n""#;
    assert_eq!(text_of(parse_node(0, &user(sent, "")).unwrap()), "a\nline1");
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
    // Claude Code always closes its reminders: an unclosed tag is the user's text.
    let unclosed = r#""hi <system-reminder>never closed""#;
    assert_eq!(text_of(unclosed), "hi <system-reminder>never closed");
    let mixed = r#""a<system-reminder>x</system-reminder> b <system-reminder>open""#;
    assert_eq!(text_of(mixed), "a b <system-reminder>open");
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
fn hidden_parent_arriving_after_its_child_rebuilds() {
    // The chain now reaches further back.
    let mut t = Tx::new();
    t.push(u("u1", None, "go"));
    t.push(a("a1", "th", "ok"));
    let mut c = t.conv();
    assert_eq!(kinds(&all(&c)), ["A:ok"]);
    t.push(think("th", "u1"));
    c.update().unwrap();
    assert_eq!(kinds(&all(&c)), ["U:go", "A:ok"]);
}

#[test]
fn shown_line_written_again_as_hidden_rebuilds() {
    let mut t = Tx::new();
    t.push(u("u1", None, "go"));
    t.push(a("a1", "u1", "ok"));
    let mut c = t.conv();
    let mut meta = u("u2", Some("a1"), "again");
    t.push(meta.clone());
    c.update().unwrap();
    assert_eq!(kinds(&all(&c)), ["U:go", "A:ok", "U:again"]);
    meta["isMeta"] = true.into();
    t.push(meta);
    c.update().unwrap();
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
