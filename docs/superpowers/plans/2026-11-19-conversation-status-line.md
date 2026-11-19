# Conversation Tab Status Line Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A one-line live status above the Conversation tab's message box: Claude's spinner and footer mode line from the app terminal's screen when present, otherwise state/elapsed/turn tokens, mode, background shells, queued messages and running agents built from the registry and transcript.

**Architecture:** core parses new transcript line kinds into `Entry` variants, `SessionStats` keeps mode, running background tasks, the prompt queue and turn output tokens, and the monitor clears shells and queue for good when a live session is seen idle; `SessionCard` carries the results. The UI reads the app terminal's xterm buffer through pure helpers (`statusLine.ts`) and renders `ConvStatus`, falling back to the card data.

**Tech Stack:** Rust (core `station-core`, app `aicstation`), React 19 + TypeScript, xterm.js, vitest.

**Spec:** `docs/superpowers/specs/2026-11-19-conversation-status-line-design.md`

## Global Constraints

- Live sessions only (`card.pid !== 0`); a closed session shows no line.
- Parts are joined with ` · `; a part with nothing to say is left out.
- Spinner line: optional spaces, one non-letter glyph (`✽ ✶ ✻ ✢ ·` or ASCII `*`), a space, a word, `…`, then ` (`. A line without `…` (`✻ Crunched for 3s · done 10:40`) is not a spinner.
- Footer mode line: trimmed text starting with `⏵⏵` or `⏸`.
- Key hints are dropped from both screen lines: a ` · `-part that is `shift+tab to cycle`, `esc to interrupt`, `↓ to manage`, or is wrapped in parentheses (a part that only contains parentheses, like `Fermenting… (33s · …)`, is not a hint; the spinner's own `( … )` stays).
- Screen read: rows `baseY .. baseY + rows - 1` of `term.buffer.active`, `translateToString(true)`; re-read on `term.onWriteParsed`, at most every 250 ms with a trailing call.
- Mode labels (tr): `⏵⏵ otomatik mod` (auto), `⏵⏵ düzenlemeleri kabul` (acceptEdits), `⏸ plan modu` (plan), `⏵⏵ izinler atlanıyor` (bypassPermissions); `default` or none → no part; any other value → `⏵⏵ <value>`.
- Counts shown only when > 0: `conv.bgShells` (tr `1 arka plan komutu` / `{n} arka plan komutu`), `conv.queued` (tr `1 mesaj kuyrukta` / `{n} mesaj kuyrukta`), agents with the existing `card.agentsRunning`.
- `conv.turnTokens` (tr `↓ {n} token`, `{n}` from `fmtTokens`), shown only while status is busy or shell and the turn has > 0 output tokens.
- Elapsed from `statusSinceMs` with `fmtElapsed`, for every status except idle, ticking every second.
- Background tasks and queue entries capped at 64 each, oldest dropped.
- New i18n keys in all 8 languages (`i18n.test.ts` checks the key sets match).

## Review Focus

- A session resumed after a crash: the transcript holds starts of shells whose finishes never came; once the session is seen idle they must not come back on the next busy turn. (Task 2 `settle_idle_drops_older_starts_also_read_later`.)
- A `remove` for content that is not in the queue (absorbed before its enqueue was read): no panic, no negative count. (Task 2 `queue_remove_unknown_is_ignored`.)
- Spinner and mode lines with wide glyphs (`⏵⏵`, `✽`) and trailing spaces: matched after trimming, shown without trailing spaces. (Task 4 `finds_mode_line_with_wide_glyphs_and_padding`.)
- The terminal closes while the Conversation tab is open (the `TerminalEntry` disappears): the line falls back to card data, no listener left on a disposed terminal. (Task 5 `falls_back_when_the_terminal_goes_away`.)
- A finished spinner line still on screen after the turn (`✻ Crunched for 3s · done`): state part comes from the card. (Task 4 `finished_line_is_not_a_spinner`.)

---

### Task 1: Parse the new transcript line kinds

**Files:**
- Modify: `core/src/transcript.rs` (`Entry`, `parse_line`, tests module at the end)

**Interfaces:**
- Produces, in `Entry`:
  - `PermissionMode(String)` — `{"type":"permission-mode","permissionMode":X}`.
  - `BackgroundStarted { id: String, at: DateTime<Utc> }` — a `user` line's `tool_result` whose `content` (string, or the `text` of its text blocks) starts with `Command running in background with ID: `; id = following chars up to `.` or whitespace; `at` = line timestamp.
  - `TaskFinished(String)` — task id from `<task-id>…</task-id>` inside text containing `<task-notification>`: in a `user` line's content (string or text blocks), and in an `attachment` line with `attachment.type == "queued_command"` and (`attachment.commandMode == "task-notification"` or `attachment.origin.kind == "task-notification"`), text taken from `attachment.prompt` (string or text blocks).
  - `UserPrompt { at: DateTime<Utc> }` — a `user` line, not `isMeta: true`, not `origin.kind == "task-notification"`, not `promptSource == "system"`, whose content is a non-empty string or has a text block, and whose first text does not start (after trim) with `<task-notification>`.
  - `Queue { op: QueueOp, content: Option<String>, at: DateTime<Utc> }` with `pub enum QueueOp { Enqueue, Dequeue, Remove, PopAll }` from `{"type":"queue-operation","operation":"enqueue"|"dequeue"|"remove"|"popAll"}`; unknown operations yield nothing; `content` as a string when it is one.

- [ ] **Step 1: Write the failing tests** in `transcript.rs` tests:

```rust
#[test]
fn parses_permission_mode() {
    assert_eq!(parse_line(r#"{"type":"permission-mode","permissionMode":"auto","sessionId":"s"}"#),
        vec![Entry::PermissionMode("auto".into())]);
}

#[test]
fn parses_background_start_from_string_and_blocks() {
    let s = r#"{"type":"user","timestamp":"2026-11-19T07:28:50.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t","content":"Command running in background with ID: bwef6ptw2. Output is being written to: C:\\x"}]}}"#;
    assert!(parse_line(s).iter().any(|e| matches!(e, Entry::BackgroundStarted { id, .. } if id == "bwef6ptw2")));
    let b = r#"{"type":"user","timestamp":"2026-11-19T07:28:50.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t","content":[{"type":"text","text":"Command running in background with ID: b2 now"}]}]}}"#;
    assert!(parse_line(b).iter().any(|e| matches!(e, Entry::BackgroundStarted { id, .. } if id == "b2")));
}

#[test]
fn parses_task_finished_from_user_line_and_attachment() {
    let note = "<task-notification>\\n<task-id>b1</task-id>\\n<status>killed</status>\\n</task-notification>";
    let u = format!(r#"{{"type":"user","timestamp":"2026-11-19T07:30:00.000Z","message":{{"role":"user","content":"{note}"}}}}"#);
    assert!(parse_line(&u).contains(&Entry::TaskFinished("b1".into())));
    assert!(!parse_line(&u).iter().any(|e| matches!(e, Entry::UserPrompt { .. })), "a notice is no prompt");
    let a = format!(r#"{{"type":"attachment","timestamp":"2026-11-19T07:30:00.000Z","attachment":{{"type":"queued_command","commandMode":"task-notification","prompt":"{note}"}}}}"#);
    assert!(parse_line(&a).contains(&Entry::TaskFinished("b1".into())));
}

#[test]
fn parses_user_prompt_but_not_tool_results_or_meta() {
    let p = r#"{"type":"user","timestamp":"2026-11-19T07:28:49.064Z","message":{"role":"user","content":"Run ping"}}"#;
    assert!(parse_line(p).iter().any(|e| matches!(e, Entry::UserPrompt { .. })));
    let tr = r#"{"type":"user","timestamp":"2026-11-19T07:28:49.064Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t","content":"ok"}]}}"#;
    assert!(!parse_line(tr).iter().any(|e| matches!(e, Entry::UserPrompt { .. })));
    let meta = r#"{"type":"user","isMeta":true,"timestamp":"2026-11-19T07:28:49.064Z","message":{"role":"user","content":"x"}}"#;
    assert!(!parse_line(meta).iter().any(|e| matches!(e, Entry::UserPrompt { .. })));
}

#[test]
fn parses_queue_operations() {
    let q = |op: &str, extra: &str| format!(r#"{{"type":"queue-operation","operation":"{op}","timestamp":"2026-11-19T07:15:27.968Z","sessionId":"s"{extra}}}"#);
    assert!(matches!(&parse_line(&q("enqueue", r#","content":"hi""#))[..], [Entry::Queue { op: QueueOp::Enqueue, content: Some(c), .. }] if c == "hi"));
    assert!(matches!(&parse_line(&q("dequeue", ""))[..], [Entry::Queue { op: QueueOp::Dequeue, content: None, .. }]));
    assert!(matches!(&parse_line(&q("remove", r#","content":"hi","reason":"absorbed_mid_turn""#))[..], [Entry::Queue { op: QueueOp::Remove, .. }]));
    assert!(matches!(&parse_line(&q("popAll", r#","content":"a""#))[..], [Entry::Queue { op: QueueOp::PopAll, .. }]));
    assert!(parse_line(&q("shuffle", "")).is_empty());
}
```

- [ ] **Step 2: Run** `cargo test -p station-core transcript::tests` — expected: compile errors for the missing variants.
- [ ] **Step 3: Implement** the variants and their parsing in `parse_line` (keep `ToolResult` and `Meta` as they are; the new entries are pushed alongside). Add a `Entry::*` arm doing nothing yet in `SessionStats::apply` and any other exhaustive `match` on `Entry` so the crate compiles.
- [ ] **Step 4: Run** `cargo test -p station-core` — expected: all pass.
- [ ] **Step 5: Commit** `git commit -m "Parse permission mode, background tasks, prompts and queue operations from transcripts"`

---

### Task 2: Keep mode, background tasks, queue and turn tokens in SessionStats

**Files:**
- Modify: `core/src/stats.rs` (`SessionStats`, `apply`, tests)

**Interfaces:**
- Consumes: Task 1 `Entry` variants, `QueueOp`.
- Produces on `SessionStats`:
  - `pub permission_mode: Option<String>` (last seen).
  - `pub fn background_shells(&self) -> u32` (running started ids).
  - `pub fn queued_messages(&self) -> u32` (queue entries whose content does not start, after trim, with `<task-notification>`; an entry without content counts as a message).
  - `pub turn_output_tokens: u64` (output of first-seen assistant usages since the last `UserPrompt`; `UserPrompt` resets to 0).
  - `pub fn settle_idle(&mut self, since: DateTime<Utc>)`: drops started ids and queue entries whose line time is `<= since`, and remembers `since` so later-applied `BackgroundStarted`/`Queue::Enqueue` with `at <= since` are ignored on arrival. `ReadEvent::Reset` (a new default `SessionStats`) forgets it.
- Private state (serde skip): background ids with start time, in start order; queue as a `VecDeque<(Option<String>, DateTime<Utc>)>`; `settled_before: Option<DateTime<Utc>>`.

- [ ] **Step 1: Write the failing tests** (`stats.rs` tests; build entries directly):

```rust
fn at(s: u32) -> DateTime<Utc> { Utc.with_ymd_and_hms(2026, 11, 19, 7, 0, s).unwrap() }

#[test]
fn background_tasks_start_and_finish() {
    let mut st = SessionStats::default();
    st.apply(&Entry::BackgroundStarted { id: "a".into(), at: at(1) });
    st.apply(&Entry::BackgroundStarted { id: "b".into(), at: at(2) });
    st.apply(&Entry::TaskFinished("a".into()));
    st.apply(&Entry::TaskFinished("zz".into()));
    assert_eq!(st.background_shells(), 1);
}

#[test]
fn background_tasks_are_capped() {
    let mut st = SessionStats::default();
    for i in 0..70 { st.apply(&Entry::BackgroundStarted { id: format!("t{i}"), at: at(1) }); }
    assert_eq!(st.background_shells(), 64);
}

#[test]
fn settle_idle_drops_older_starts_also_read_later() {
    let mut st = SessionStats::default();
    st.apply(&Entry::BackgroundStarted { id: "old".into(), at: at(1) });
    st.settle_idle(at(5));
    assert_eq!(st.background_shells(), 0);
    st.apply(&Entry::BackgroundStarted { id: "late".into(), at: at(4) });
    assert_eq!(st.background_shells(), 0, "written before idle, read after");
    st.apply(&Entry::BackgroundStarted { id: "new".into(), at: at(9) });
    assert_eq!(st.background_shells(), 1);
}

#[test]
fn queue_counts_messages_not_notifications() {
    let mut st = SessionStats::default();
    let q = |op, c: Option<&str>, s| Entry::Queue { op, content: c.map(Into::into), at: at(s) };
    st.apply(&q(QueueOp::Enqueue, Some("<task-notification>x</task-notification>"), 1));
    st.apply(&q(QueueOp::Enqueue, Some("hi"), 2));
    st.apply(&q(QueueOp::Enqueue, Some("again"), 3));
    assert_eq!(st.queued_messages(), 2);
    st.apply(&q(QueueOp::Dequeue, None, 4)); // takes the notification (oldest)
    assert_eq!(st.queued_messages(), 2);
    st.apply(&q(QueueOp::Remove, Some("hi"), 5));
    assert_eq!(st.queued_messages(), 1);
    st.apply(&q(QueueOp::PopAll, Some("again"), 6));
    assert_eq!(st.queued_messages(), 0);
}

#[test]
fn queue_remove_unknown_is_ignored() {
    let mut st = SessionStats::default();
    st.apply(&Entry::Queue { op: QueueOp::Remove, content: Some("x".into()), at: at(1) });
    st.apply(&Entry::Queue { op: QueueOp::Dequeue, content: None, at: at(2) });
    assert_eq!(st.queued_messages(), 0);
}

#[test]
fn settle_idle_empties_the_queue() {
    let mut st = SessionStats::default();
    st.apply(&Entry::Queue { op: QueueOp::Enqueue, content: Some("hi".into()), at: at(1) });
    st.settle_idle(at(3));
    st.apply(&Entry::Queue { op: QueueOp::Enqueue, content: Some("late".into()), at: at(2) });
    assert_eq!(st.queued_messages(), 0);
}

#[test]
fn turn_tokens_reset_by_a_prompt_only() {
    let mut st = SessionStats::default();
    // assistant usage helper: output 100, distinct message ids
    st.apply(&assistant("m1", 100, at(1)));
    st.apply(&Entry::UserPrompt { at: at(2) });
    st.apply(&assistant("m2", 30, at(3)));
    st.apply(&Entry::ToolResult { tool_use_id: "t".into(), is_error: false, at: at(4) });
    st.apply(&Entry::TaskFinished("b".into()));
    st.apply(&assistant("m3", 12, at(5)));
    st.apply(&assistant("m3", 12, at(5))); // same id: once
    assert_eq!(st.turn_output_tokens, 42);
}

#[test]
fn permission_mode_is_the_last_seen() {
    let mut st = SessionStats::default();
    st.apply(&Entry::PermissionMode("auto".into()));
    st.apply(&Entry::PermissionMode("plan".into()));
    assert_eq!(st.permission_mode.as_deref(), Some("plan"));
}
```

(`assistant(id, output, at)` is a test helper building `Entry::Assistant` with a `Usage { output, ..Default::default() }`, model `claude-opus-5-5`; add it if the module has none.)

- [ ] **Step 2: Run** `cargo test -p station-core stats::tests` — expected: compile errors / failures.
- [ ] **Step 3: Implement** the fields, the `apply` arms and `settle_idle`; add `self.turn_output_tokens += u.output` next to the existing first-time token totals.
- [ ] **Step 4: Run** `cargo test -p station-core` — expected: all pass.
- [ ] **Step 5: Commit** `git commit -m "Track mode, background tasks, the prompt queue and turn tokens per session"`

---

### Task 3: Carry the status data on session cards; settle on idle

**Files:**
- Modify: `core/src/monitor.rs` (`SessionCard`, `make_card`, `refresh`, `live_detail`, tests)
- Modify: `src-tauri/src/alerts.rs`, `src-tauri/src/tray.rs` (test `SessionCard` literals)
- Modify: `ui/src/api.ts` (`SessionCard`), every UI test card literal that lists `transcriptBytes: 0` (add the new fields after it)

**Interfaces:**
- Consumes: Task 2 `SessionStats` API.
- Produces on `SessionCard` (serde camelCase): `permission_mode: Option<String>`, `background_shells: u32`, `queued_messages: u32`, `turn_output_tokens: u64`, `status_since_ms: u64` (the live session's `status_updated_at_ms`; 0 when closed). UI: `permissionMode: string | null; backgroundShells: number; queuedMessages: number; turnOutputTokens: number; statusSinceMs: number`.
- Monitor: before building a live card (in `refresh` and `live_detail`), when the live session's status is `SessionStatus::Idle`, call `stats.settle_idle(<status_updated_at_ms as DateTime>)`.

- [ ] **Step 1: Write the failing test** in `monitor.rs` tests (use `Env`, `e.reg`, `e.append`):

```rust
#[test]
fn card_status_fields_and_idle_clears_shells_and_queue() {
    let e = Env::new();
    let sid = id(1);
    let t = e.transcript(&sid);
    e.append(&t, r#"{"type":"permission-mode","permissionMode":"auto","sessionId":"s"}"#);
    e.append(&t, r#"{"type":"user","timestamp":"2026-11-02T07:00:01Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t","content":"Command running in background with ID: b1."}]}}"#);
    e.append(&t, r#"{"type":"queue-operation","operation":"enqueue","timestamp":"2026-11-02T07:00:02Z","content":"hi"}"#);
    e.reg(1, &sid, "busy", 1_793_602_803_000, None); // 07:00:03Z
    let mut m = e.mon();
    let c = m.refresh(&FakeProbe, now()).sessions[0].clone();
    assert_eq!((c.permission_mode.as_deref(), c.background_shells, c.queued_messages, c.status_since_ms),
        (Some("auto"), 1, 1, 1_793_602_803_000));
    e.reg(1, &sid, "idle", 1_793_602_810_000, None); // 07:00:10Z
    let c = m.refresh(&FakeProbe, now()).sessions[0].clone();
    assert_eq!((c.background_shells, c.queued_messages), (0, 0));
    e.reg(1, &sid, "busy", 1_793_602_820_000, None);
    let c = m.refresh(&FakeProbe, now()).sessions[0].clone();
    assert_eq!((c.background_shells, c.queued_messages), (0, 0), "cleared for good");
    let v = serde_json::to_value(&c).unwrap();
    for k in ["permissionMode", "backgroundShells", "queuedMessages", "turnOutputTokens", "statusSinceMs"] {
        assert!(v.get(k).is_some(), "{k}");
    }
}
```

- [ ] **Step 2: Run** `cargo test -p station-core card_status_fields` — expected: compile errors.
- [ ] **Step 3: Implement** the fields in `make_card` (closed cards: `None`/0) and the idle settle; add the fields to the `alerts.rs`/`tray.rs` literals and to `api.ts` plus the UI test literals (`permissionMode: null, backgroundShells: 0, queuedMessages: 0, turnOutputTokens: 0, statusSinceMs: 0`).
- [ ] **Step 4: Run** `cargo test --workspace` and `cd ui && npx tsc --noEmit && npx vitest run` — expected: all pass.
- [ ] **Step 5: Commit** `git commit -m "Carry mode, shells, queue, turn tokens and status time on session cards"`

---

### Task 4: Find Claude's spinner and footer mode line on a terminal screen

**Files:**
- Create: `ui/src/conversation/statusLine.ts`
- Test: `ui/src/conversation/statusLine.test.ts`

**Interfaces:**
- Produces:
  - `export function findSpinner(rows: string[]): string | null` — bottom-up first spinner row (Global Constraints), trimmed, key-hint parts dropped.
  - `export function findModeLine(rows: string[]): string | null` — bottom-up first footer mode row, trimmed, key-hint parts dropped.
  - `export function screenRows(term: Terminal): string[]` — rows `baseY .. baseY + rows - 1` of `term.buffer.active`, `translateToString(true)`.

- [ ] **Step 1: Write the failing tests:**

```ts
test("finds the spinner, ASCII star included, and drops key hints", () => {
  expect(findSpinner(["", "* Fermenting… (33s · ↓ 2.8k tokens · thinking with high effort)", "  ⎿ Tip: x", "> "]))
    .toBe("* Fermenting… (33s · ↓ 2.8k tokens · thinking with high effort)");
  expect(findSpinner(["✽ Prestidigitating… (46s · ↓ 3.9k tokens) · esc to interrupt"]))
    .toBe("✽ Prestidigitating… (46s · ↓ 3.9k tokens)");
});
test("finished_line_is_not_a_spinner", () => {
  expect(findSpinner(["✻ Crunched for 3s · done 10:40 · 1 shell still running"])).toBeNull();
});
test("finds_mode_line_with_wide_glyphs_and_padding", () => {
  expect(findModeLine(["  ⏵⏵ auto mode on · 1 shell · ↓ to manage   "])).toBe("⏵⏵ auto mode on · 1 shell");
  expect(findModeLine(["⏸ plan mode on (shift+tab to cycle)"])).toBe("⏸ plan mode on");
  expect(findModeLine(["> typing ⏵⏵ here"])).toBeNull();
});
test("screenRows reads the screen at the bottom, not the scrolled view", () => {
  const lines = ["old", "a", "b"];
  const term = { rows: 2, buffer: { active: { baseY: 1, viewportY: 0,
    getLine: (i: number) => ({ translateToString: () => lines[i] }) } } };
  expect(screenRows(term as never)).toEqual(["a", "b"]);
});
```

(`⏸ plan mode on (shift+tab to cycle)`: the hint has no ` · ` before it; drop a trailing parenthesised hint that follows a space, but never the spinner's own `(…)` — the spinner keeps its parentheses because it is handled by `findSpinner`, which only drops ` · `-parts.)

- [ ] **Step 2: Run** `cd ui && npx vitest run src/conversation/statusLine.test.ts` — expected: FAIL (module missing).
- [ ] **Step 3: Implement** the three functions.
- [ ] **Step 4: Run** the same command — expected: PASS.
- [ ] **Step 5: Commit** `git commit -m "Find Claude's spinner and mode line on a terminal screen"`

---

### Task 5: Show the status line in the Conversation tab

**Files:**
- Create: `ui/src/conversation/ConvStatus.tsx`
- Modify: `ui/src/conversation/ConversationPage.tsx` (render `<ConvStatus card={card} />` at the top of `.conv-foot` when `live`)
- Modify: `ui/src/i18n/{tr,en,de,fr,es,ru,zh,az}.ts`, `ui/src/features.css`
- Test: `ui/src/conversation/ConvStatus.test.tsx`

**Interfaces:**
- Consumes: Task 3 card fields; Task 4 `findSpinner`, `findModeLine`, `screenRows`; `terminalForPid`, `getTerminals`, `subscribe` from `../terminal/store`; `statusLabel`, `statusKind`, `statusColor`, `fmtElapsed`, `fmtTokens` from `../format`.
- Produces: `export default function ConvStatus({ card }: { card: SessionCard }): JSX.Element` — a `<div className="conv-status-line" title={full text}>` that always renders (empty keeps its height); parts in spec order.
- i18n keys: `conv.mode.auto`, `conv.mode.acceptEdits`, `conv.mode.plan`, `conv.mode.bypassPermissions`, `conv.bgShells` (plural), `conv.queued` (plural), `conv.turnTokens`.

- [ ] **Step 1: Write the failing tests** (mock `../terminal/store` with a controllable `terminalForPid`/`getTerminals`/`subscribe`; fake terminal object as in Task 4 plus `onWriteParsed(cb) => { dispose }`):

```tsx
test("busy from card data: label, elapsed, turn tokens, mode, shells, queue, agents", () => {
  vi.useFakeTimers(); vi.setSystemTime(100_000);
  render(<ConvStatus card={card({ status: "busy", statusSinceMs: 54_000, turnOutputTokens: 3900,
    permissionMode: "auto", backgroundShells: 1, queuedMessages: 2, agentsRunning: 1 })} />);
  expect(line()).toBe("çalışıyor · 46 sn · ↓ 3,9K token · ⏵⏵ otomatik mod · 1 arka plan komutu · 2 mesaj kuyrukta · 1 ajan çalışıyor");
  act(() => { vi.advanceTimersByTime(1000); });
  expect(line()).toContain("47 sn");
});
test("idle: no elapsed, default mode and zero counts left out", () => {
  render(<ConvStatus card={card({ status: "idle", permissionMode: "default" })} />);
  expect(line()).toBe("hazır");
});
test("app terminal: spinner and mode line from the screen replace card parts", () => {
  screen = ["* Fermenting… (33s · ↓ 2.8k tokens)", "> ", "⏵⏵ auto mode on · 1 shell · ↓ to manage"];
  render(<ConvStatus card={card({ status: "busy", permissionMode: "plan", backgroundShells: 3, queuedMessages: 1 })} />);
  expect(line()).toBe("* Fermenting… (33s · ↓ 2.8k tokens) · ⏵⏵ auto mode on · 1 shell · 1 mesaj kuyrukta");
});
test("falls_back_when_the_terminal_goes_away", () => {
  screen = ["* Fermenting… (3s · ↓ 10 tokens)"];
  render(<ConvStatus card={card({ status: "busy", statusSinceMs: Date.now() })} />);
  expect(line()).toMatch(/^\* Fermenting/);
  act(() => { terminals = []; emitStore(); });
  expect(line()).toMatch(/^çalışıyor/);
  expect(disposed).toBe(1);
});
```

And in `ConversationPage.test.tsx`:

```tsx
test("a closed session shows no status line", async () => {
  serve({ get_conversation_page: () => page([REPLY]) });
  const { container } = render(<ConversationPage sessionId="s1" card={card({ pid: 0 })} {...props} />);
  await screen.findByText("ilk cevap");
  expect(container.querySelector(".conv-status-line")).toBeNull();
});
```

(`line()` reads the `.conv-status-line` text with whitespace collapsed; the state dot is `aria-hidden` and not part of the text. Elapsed and token strings must match what `fmtElapsed(46_000)` and `fmtTokens(3900)` return in tr; adjust the literal to the helpers' actual output if it differs, not the helpers.)

- [ ] **Step 2: Run** `cd ui && npx vitest run src/conversation/ConvStatus.test.tsx` — expected: FAIL.
- [ ] **Step 3: Implement** `ConvStatus` (1 s ticker while status is not idle; screen read throttled to 250 ms with a trailing call; re-resolve the terminal on store changes), the i18n keys in all 8 languages, the CSS (`.conv-status-line`: one line, 12px, `var(--muted)`, `min-height` of one line, ellipsis, same max width as `.conv-composer`), and the `ConversationPage` wiring.
- [ ] **Step 4: Run** `cd ui && npx tsc --noEmit && npx vitest run` — expected: all pass.
- [ ] **Step 5: Commit** `git commit -m "Show a live status line in the conversation tab"`

---

### Task 6: Verify on screen and document

**Files:**
- Modify: `README.md` (English Conversation tab section and the Turkish section: one sentence each on the status line)

- [ ] **Step 1:** Close the local test app, `npx tauri build --no-bundle`, start `D:\cargo-target\windows\release\aicstation.exe`, open a new app terminal, run `claude --model haiku` in the scratch test folder, open the Conversation tab.
- [ ] **Step 2:** Send a prompt that runs `ping -n 20 127.0.0.1` in the foreground: screenshot shows the spinner line with live seconds and the `⏵⏵ auto mode on` part.
- [ ] **Step 3:** While it runs, send a second message: screenshot shows `1 mesaj kuyrukta`, which disappears once it is taken.
- [ ] **Step 4:** Ask for a background `ping -n 60`: after the turn, screenshot shows `komut çalıştırıyor` with the shell part; after it ends, the line returns to `hazır`.
- [ ] **Step 5:** Shift+Tab in the Terminal tab, back to Conversation: the mode part changed without sending a message.
- [ ] **Step 6:** README sentences; commit `git commit -m "Describe the conversation status line"`.
