# Conversation Tab Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A `Conversation` tab in the session view that shows a session's conversation from its transcript with Claude's images (thumbnail → popup), a message box that writes into the hosting app terminal, and a Remote Control mark on live sessions.

**Architecture:** `station-core` gains a `conversation` module that streams a transcript (with line byte offsets) into a parent-linked node store, follows the current branch, groups it into conversation items and pages through them; images are only *referenced* (line offset + ordinal) and read on demand. `src-tauri` caches the builders of the last two opened sessions and exposes page / since / image / send / open-remote commands. The React UI adds the tab, a conversation view with lazy thumbnails, an image popup and a composer.

**Tech Stack:** Rust (station-core, Tauri 2 app), React 18 + TypeScript, Vitest + Testing Library.

**Spec:** `docs/superpowers/specs/2026-11-18-conversation-tab-design.md`

## Global Constraints

- The app makes no network request; the only outward action is handing `https://claude.ai/code/<bridgeSessionId>` to the system browser on click.
- Never read `.credentials.json`, `sessions/*.key` or any token.
- Arguments from the UI are untrusted: session ids pass `is_session_id`; the URL is built in Rust from the registry; image refs are checked against refs the builder produced.
- Images: only `source.type == "base64"` with media type `image/png`, `image/jpeg`, `image/gif`, `image/webp`.
- Caps: text per item 64 KiB (`truncated: true` when cut); image line read cap = `transcript::MAX_LINE` (16 MiB); page size 100 items; send text 32 KiB; builder cache 2 sessions; UI image cache 30 entries.
- No `dangerouslySetInnerHTML`; no new npm or cargo dependency (THIRD_PARTY_NOTICES unchanged).
- Every new error code goes into `src-tauri/src/errors.rs` (and its `ALL` list) with `err.<code>` in all 8 dictionaries; every new UI key in all 8 dictionaries (`ui/src/i18n/i18n.test.ts` enforces both).
- Every new command: `generate_handler!` in `src-tauri/src/main.rs`, `COMMANDS` in `src-tauri/build.rs`, `allow-<name>` in `src-tauri/capabilities/default.json` (`src-tauri/tests/config_guard.rs` checks they agree).
- Send while `idle` or `busy`; refuse when status is `waiting` or the registry has `waitingFor`. Bracketed paste, then `\r` in a separate write.
- Work on branch `feature/conversation-tab`. Commit messages follow the repo style (plain sentence subject, explanatory body) with **no** AI attribution or co-author lines.

## Review Focus

1. **Transcript line still being written** when the view reads it (partial last line): the item must appear once, complete, on the next read — Task 1 test `offsets_survive_partial_tail`.
2. **Image whose line no longer matches** (file rewritten/shrunk after the ref was handed out): the image command returns `conv_image_gone`, the thumbnail shows a broken-image placeholder, nothing else breaks — Task 2 test `read_image_rejects_moved_line`, Task 8 test `thumb shows placeholder on error`.
3. **Very long session** (thousands of items, tens of MB): the first open returns only the newest 100 items and older pages on demand — Task 3 test `page_returns_newest_100_of_5000`.
4. **Transcript text containing HTML or markdown links** (`<script>`, `<img onerror>`, `[x](javascript:…)`): shown as text, never live — Task 7 test `html stays text`.
5. **Hosting terminal closed or session `/clear`ed between render and Send**: backend refuses with `send_not_embedded` / `session_not_live`, the composer shows the error and keeps the text — Task 5 test `send_target_refuses_when_not_in_app_terminal`, Task 8 test `send error keeps text`.

---

### Task 0: Branch

- [ ] **Step 1:** In the repo root: `git switch -c feature/conversation-tab` — Expected: `Switched to a new branch 'feature/conversation-tab'`.

---

### Task 1: Line offsets in `TranscriptReader`

**Files:**
- Modify: `core/src/transcript.rs` (`TranscriptReader::scan_with` and a new public method)
- Test: same file, `mod tests`

**Interfaces:**
- Produces: `TranscriptReader::read_lines_at(&mut self, on_reset: impl FnMut(), f: impl FnMut(u64, &str)) -> io::Result<()>` — streams every complete line appended since the last call with the **byte offset of its first byte** in the file; calls `on_reset` once before any line when the file shrank (then offsets restart at 0). Over-long lines are skipped as today; offsets of later lines stay exact.

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn read_lines_at_gives_byte_offsets() {
    // "ab\nüç\n\nx\n": offsets 0, 3, 8, 9 (ü and ç are 2 bytes each)
    let got = collect_at(&mut reader_over("ab\nüç\n\nx\n"));
    assert_eq!(got, [(0, "ab".into()), (3, "üç".into()), (8, "".into()), (9, "x".into())]);
}
#[test]
fn offsets_cross_chunk_boundaries() {
    // first line READ_BUF + 10 bytes long, second line "z": offset of "z" == READ_BUF + 11
}
#[test]
fn offsets_survive_partial_tail() {
    // write "a\nbc" → only (0,"a"); append "d\n" → only (2,"bcd")
}
#[test]
fn offsets_restart_after_shrink() {
    // read "aaaa\nb\n"; truncate to "c\n"; next call: on_reset called once, then (0,"c")
}
#[test]
fn offsets_exact_after_overlong_line() {
    // max_line = 1000; "x"*5000 + "\nok\n" → only (5001, "ok")
}
```
(`collect_at` / `reader_over` are small test helpers over a tempfile.)

- [ ] **Step 2:** `cargo test -p station-core transcript::tests::read_lines_at` — Expected: FAIL (method missing).
- [ ] **Step 3: Implement.** Track the file offset of `carry[0]` inside `scan_with` (`self.offset - carry.len()` after each extend, adjusted when a skip ends); pass `start + position-in-carry` to a new internal callback. Keep `read_lines*` and `read_each*` unchanged in behaviour.
- [ ] **Step 4:** `cargo test -p station-core transcript` — Expected: all PASS.
- [ ] **Step 5: Commit** — "Give transcript lines their byte offset".

---

### Task 2: Conversation nodes and image reading

**Files:**
- Create: `core/src/conversation.rs` (register `pub mod conversation;` in `core/src/lib.rs`)
- Create: `core/tests/fixtures/conversation_basic.jsonl` (hand-written, tiny base64 PNG)

**Interfaces:**
- Produces (all `pub`, serde `Serialize`, camelCase):
  - `ImageRef { offset: u64, ordinal: u32, media_type: String }`
  - `fn parse_node(offset: u64, line: &str) -> Option<Node>`; `Node { uuid: String, parent: Option<String>, logical_parent: Option<String>, body: NodeBody, at_ms: i64 }`
  - `enum NodeBody { UserText { text: String, truncated: bool, images: Vec<ImageRef> }, AssistantText { text: String, truncated: bool }, ToolUses(Vec<(String /*id*/, String /*tool*/, String /*target*/)>), ToolResults(Vec<(String /*tool_use_id*/, bool /*is_error*/, Vec<ImageRef>)>), CompactBoundary, Other }`
  - `fn read_image(path: &Path, offset: u64, ordinal: u32) -> Result<(String, String), ImageError>` → `(media_type, base64)`; `enum ImageError { Gone, Io }`
- Image **ordinal** = index among *all* image blocks of the line in document order: top-level `message.content[]` blocks, and for a `tool_result` block its `content[]` array, depth-first.

Parsing rules (from real 2.1.327 transcripts):
- Skip (`None`): lines without `uuid`; `isSidechain: true`; `isMeta: true`; `isCompactSummary: true`; `isVisibleInTranscriptOnly: true`; types other than `user` / `assistant` / `system`.
- `system` with `subtype == "compact_boundary"` → `CompactBoundary` (expected shape: `parentUuid: null`, `logicalParentUuid: "<uuid>"`; confirm against a real `/compact` in Task 10). Other `system` → `Other` (kept for the chain, never displayed).
- `user` with string content, or array with `text`/`image` blocks → `UserText`. A text starting with `<command-name>` becomes the command as typed (`/name args` from `<command-name>` + `<command-args>`); text that is only `<local-command-stdout>…`, `<local-command-caveat>…` or `<system-reminder>…` → `Other`.
- `user` whose array holds `tool_result` blocks → `ToolResults` (string or array `content`; images collected from the array).
- `assistant`: `text` blocks → `AssistantText` (joined with `\n\n`); `tool_use` blocks → `ToolUses` (target via existing `tool_target`, make it `pub(crate)`); a line with only `thinking` → `Other`.
- Text capped at 64 KiB on a char boundary with `truncated: true`.

- [ ] **Step 1: Write the failing tests** (in `conversation.rs`):
  - `parses_user_text_with_pasted_image` → `UserText`, one `ImageRef { ordinal: 0, media_type: "image/png" }`, offset as given.
  - `parses_tool_result_images_in_order` → two images in one tool_result → ordinals 0 and 1.
  - `skips_meta_sidechain_and_compact_summary` → `None` for each flag.
  - `command_wrapper_becomes_command_text` → `"<command-name>/model</command-name><command-args>opus</command-args>"` → `"/model opus"`.
  - `ignores_unsupported_images` → `image/svg+xml` and `source.type: "url"` produce no ref.
  - `caps_text_at_64k` → 70 000 × `"ç"` → `text.len() <= 65536`, `truncated == true`.
  - `read_image_returns_block` → fixture, `read_image(path, off, 0) == Ok(("image/png", <base64>))`.
  - `read_image_rejects_moved_line` → offset pointing into the middle of a line or ordinal 5 → `Err(ImageError::Gone)`; base64 with characters outside `[A-Za-z0-9+/=\r\n]` → `Gone`.
- [ ] **Step 2:** `cargo test -p station-core conversation` — Expected: FAIL.
- [ ] **Step 3: Implement** `parse_node` and `read_image` (seek, read up to the next `\n` but at most `MAX_LINE`, parse, walk images in ordinal order, re-check media type and base64 charset).
- [ ] **Step 4:** `cargo test -p station-core conversation` — Expected: PASS.
- [ ] **Step 5: Commit** — "Parse transcript lines into conversation nodes".

---

### Task 3: Conversation store: current branch, grouping, paging

**Files:**
- Modify: `core/src/conversation.rs`

**Interfaces:**
- Consumes: `read_lines_at` (Task 1), `parse_node`, `NodeBody`, `ImageRef` (Task 2).
- Produces:
  - `#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")] enum ConvItem { User { text, truncated, images: Vec<ImageRef>, at_ms }, Assistant { text, truncated, at_ms }, Steps { steps: Vec<Step>, at_ms }, Compacted { at_ms } }`
  - `Step { tool: String, target: String, done: bool, is_error: bool, images: Vec<ImageRef> }`
  - `ConvPage { epoch: u32, total: u32, start: u32, items: Vec<ConvItem> }`
  - `pub const PAGE: u32 = 100;`
  - `Conversation::new(path: PathBuf) -> Self`
  - `Conversation::update(&mut self) -> io::Result<()>` — reads new lines; on reset clears everything and bumps `epoch`.
  - `Conversation::page(&self, before: Option<u32>) -> ConvPage` — `end = before.unwrap_or(total).min(total)`, `start = end.saturating_sub(PAGE)`.
  - `Conversation::since(&self, epoch: u32, from: u32) -> ConvPage` — if `epoch` differs: `page(None)`; else items `[min(from,total)..]` with `start = min(from,total)`.
  - `Conversation::has_image(&self, offset: u64, ordinal: u32) -> bool`
  - `Conversation::path(&self) -> &Path`

Algorithm (decided here):
- Store `HashMap<uuid, Node>` plus `leaf: Option<uuid>` = uuid of the last parsed node in file order whose body is not `Other`.
- After each `update` with new nodes, rebuild the chain: from `leaf` follow `parent`; when `parent` is `None` or unknown and `logical_parent` is known, emit a `Compacted` marker there and continue from `logical_parent`; stop at a root; a visited set guards cycles. Reverse to file order.
- Group the chain into items: `UserText` → `User`; consecutive `AssistantText` with no tool use in between → one `Assistant` (texts joined by `\n\n`); consecutive `ToolUses` (thinking/`Other` nodes do not break a group; a text or user message does) → one `Steps`; `ToolResults` mark their step `done`, set `is_error`, attach images; `CompactBoundary` → `Compacted`.
- Epoch: if the previous leaf is not on the new chain (rewind, edit, retry), `epoch += 1`.
- Keep a `HashSet<(u64,u32)>` of all `ImageRef`s on the current chain for `has_image`.

- [ ] **Step 1: Write the failing tests:**
  - `groups_text_and_steps` → user, assistant text, 2× Bash + Read, text → `[User, Assistant, Steps(3), Assistant]`.
  - `tool_result_marks_step_done_with_image` → step `Read grid.jpg` gets `done`, one image.
  - `rewind_shows_current_branch_and_bumps_epoch` → user A → reply; second user B whose `parentUuid` is A's parent → items contain B, not A; `epoch` increased.
  - `compaction_keeps_history_with_divider` → boundary with `logicalParentUuid` → items before, `Compacted`, items after.
  - `since_resends_last_item_and_newer` → after appending one more tool_use to the open Steps, `since(epoch, last_index)` returns that Steps with 2 steps and `start == last_index`.
  - `since_with_old_epoch_returns_newest_page`.
  - `page_returns_newest_100_of_5000` → `total == 5000`, `start == 4900`, `items.len() == 100`; `page(Some(4900))` → `start == 4800`.
  - `has_image_only_for_current_chain`.
- [ ] **Step 2:** `cargo test -p station-core conversation` — Expected: new tests FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** `cargo test -p station-core` — Expected: PASS.
- [ ] **Step 5: Commit** — "Build the current conversation branch with paging".

---

### Task 4: Conversation commands in the app

**Files:**
- Modify: `src-tauri/src/commands.rs` (`AppState` field + methods + commands), `src-tauri/src/errors.rs`, `src-tauri/src/main.rs`, `src-tauri/build.rs`, `src-tauri/capabilities/default.json`, all 8 `ui/src/i18n/*.ts` (error texts), `ui/src/api.ts`

**Interfaces:**
- Consumes: `Conversation`, `ConvPage`, `read_image` (Tasks 2-3).
- Produces (Rust): `AppState::conversation_page(&self, id: &str, before: Option<u32>) -> Result<ConvPage, String>`, `conversation_since(&self, id, epoch: u32, from: u32) -> Result<ConvPage, String>`, `conversation_image(&self, id, offset: u64, ordinal: u32) -> Result<ConvImage, String>`, `ConvImage { media_type: String, data: String }` (camelCase).
- Commands (async, `spawn_blocking` like `get_session_detail`): `get_conversation_page(session_id, before)`, `get_conversation_since(session_id, epoch, from)`, `get_conversation_image(session_id, offset, ordinal)`.
- Error codes: `CONV_UNAVAILABLE = "conv_unavailable"` (no transcript), `CONV_IMAGE_GONE = "conv_image_gone"`.
- Produces (TS, `ui/src/api.ts`): types `ConvImageRef`, `ConvStep`, `ConvItem`, `ConvPage`, `ConvImage`; `getConversationPage(sessionId: string, before?: number)`, `getConversationSince(sessionId: string, epoch: number, from: number)`, `getConversationImage(sessionId: string, offset: number, ordinal: number)`.

Cache: `conversations: Mutex<Vec<(String, Conversation)>>`, most recent first, at most 2; each call validates the id, finds or creates (path from `Locator::new(self.projects_dir.clone()).transcript(id)`), calls `update()`, answers. Image: `has_image` must be true, else `conv_image_gone`; then `read_image(conv.path(), …)` outside the lock.

- [ ] **Step 1: Failing tests** in `commands.rs` tests (reuse the temp claude-dir helpers already there): `conversation_page_reads_transcript`, `conversation_rejects_bad_id` (`"../x"` → `session_invalid`), `conversation_unavailable_without_transcript`, `conversation_image_refuses_unknown_ref`, `conversation_cache_keeps_two`.
- [ ] **Step 2:** `cargo test -p aicstation conversation` — Expected: FAIL.
- [ ] **Step 3: Implement** methods, commands, registration in the three places, error codes, `err.conv_unavailable` / `err.conv_image_gone` in all 8 dictionaries, TS wrappers.
- [ ] **Step 4:** `cargo test --workspace && npm --prefix ui test` — Expected: PASS (config_guard and i18n tests included).
- [ ] **Step 5: Commit** — "Serve conversation pages and images to the UI".

---

### Task 5: Remote Control mark, send and open commands

**Files:**
- Modify: `core/src/registry.rs` (`RawSession`, `LiveSession`), `core/src/monitor.rs` (`SessionCard`, `make_card`, new `Monitor::live_session`), `src-tauri/src/commands.rs`, `src-tauri/src/pty.rs`, `src-tauri/src/platform/mod.rs` + `unix.rs`, `src-tauri/src/errors.rs`, `main.rs`, `build.rs`, `capabilities/default.json`, 8 dictionaries, `ui/src/api.ts`, `README.md` (privacy section)

**Interfaces:**
- `LiveSession.bridge_session_id: Option<String>` (`#[serde(skip)]`), kept only when `is_bridge_id`: starts with `session_`, suffix 1..=64 ASCII alphanumerics.
- `SessionCard.remote_control: bool` (TS `remoteControl: boolean`).
- `Monitor::live_session(&self, id: &str, probe: &dyn ProcessProbe) -> Option<LiveSession>` — fresh registry read.
- `PtyManager::id_for_pid(&self, pid: u32) -> Option<u32>`.
- `pub fn paste_payload(text: &str) -> Result<String, &'static str>` — `\r\n`/`\r` → `\n`; drop every `char::is_control` except `\n`, `\t`; trim; empty → `SEND_EMPTY`; > 32 768 bytes → `SEND_TOO_LARGE`; returns `"\x1b[200~" + text + "\x1b[201~"`.
- `AppState::send_target(&self, id: &str) -> Result<u32, String>` — `session_invalid` / `session_not_live`; `Waiting` or `waiting_for.is_some()` → `SEND_WAITING`; `probe.terminal_pid(pid)` none → `SEND_NOT_EMBEDDED`; returns the shell pid.
- Command `send_to_session(session_id, text)`: payload → target → `id_for_pid` (none → `send_not_embedded`) → write payload → sleep 60 ms → write `"\r"`.
- `AppState::remote_url(&self, id: &str) -> Result<String, String>` → `https://claude.ai/code/<id>` or `REMOTE_OFF`.
- `platform::open_url(url: &str) -> io::Result<()>` — Windows: `explorer_exe()` with the URL; Linux: `xdg-open` via `find_program`. Command `open_remote_control(session_id)`; failure `URL_OPEN_FAILED`.
- Error codes: `send_empty`, `send_too_large`, `send_waiting`, `send_not_embedded`, `remote_off`, `url_open_failed`.
- TS: `sendToSession(sessionId: string, text: string)`, `openRemoteControl(sessionId: string)`.

- [ ] **Step 1: Failing tests:** registry `reads_bridge_session_id` / `rejects_bad_bridge_id` (`"session_../x"`, `"cse_01A"`, 70-char suffix → `None`); `paste_payload_strips_escapes` (`"a\x1b[201~b"` → `"\x1b[200~a[201~b\x1b[201~"`), `paste_payload_rejects_empty_and_large`; `send_target_refuses_waiting` (status `waiting`, and `idle` + `waitingFor`), `send_target_allows_busy_and_idle` (embedded probe), `send_target_refuses_when_not_in_app_terminal`; `remote_url_built_from_registry`, `remote_url_off`.
- [ ] **Step 2:** `cargo test --workspace` — Expected: FAIL.
- [ ] **Step 3: Implement**, register both commands in the three places, error texts in 8 dictionaries, TS wrappers, `remoteControl` on the TS `SessionCard`. README privacy: one line — the app opens `claude.ai/code/<session>` in the system browser only when the Remote Control mark is clicked; the app itself still makes no request.
- [ ] **Step 4:** `cargo test --workspace && cargo clippy --workspace -- -D warnings && npm --prefix ui test` — Expected: PASS.
- [ ] **Step 5: Commit** — "Mark Remote Control sessions and send text to app terminals".

---

### Task 6: UI conversation state and step summaries

**Files:**
- Create: `ui/src/conversation/convState.ts`, `ui/src/conversation/summary.ts`, `ui/src/conversation/imageCache.ts`
- Test: `ui/src/conversation/convState.test.ts`, `summary.test.ts`, `imageCache.test.ts`
- Modify: 8 dictionaries (summary keys)

**Interfaces:**
- `type ConvState = { epoch: number; start: number; total: number; items: ConvItem[] }`
- `applyPage(s: ConvState | null, p: ConvPage): ConvState` — null or other epoch → replace; page ending at or before `s.start` → prepend the part before `s.start`; `p.start >= s.start` → replace items from `p.start - s.start` on; always take `p.total`.
- `hasOlder(s: ConvState): boolean` (`s.start > 0`), `lastIndex(s): number` (`s.start + s.items.length - 1`, or 0 when empty).
- `stepSummary(steps: ConvStep[], t: TFn): string` — counts per kind, in first-seen order, joined with `", "`, first letter upper-cased: Bash → `conv.step.commands` (plural `{n}`); Read → one file `conv.step.readOne {name}`, more `conv.step.readMany` (plural `{n}`); Edit/Write/MultiEdit/NotebookEdit → `conv.step.editOne {name}` / `conv.step.editMany`; Grep/Glob → `conv.step.searched`; anything else → `conv.step.used {tool}` per distinct tool. EN copy: "ran a command" / "ran {n} commands", "read {name}", "read {n} files", "edited {name}", "edited {n} files", "searched", "used {tool}". TR copy: "bir komut çalıştırdı" / "{n} komut çalıştırdı", "{name} okundu", "{n} dosya okundu", "{name} düzenlendi", "{n} dosya düzenlendi", "arama yaptı", "{tool} kullandı". Other languages: builder translates.
- `createImageCache(max = 30)` → `{ get(key), set(key, url) }` LRU; key `"<sessionId>:<offset>:<ordinal>"`.

- [ ] **Step 1: Failing tests:** `applyPage` replace / prepend older / since replaces last item and appends / epoch change resets; `stepSummary` for `[Bash, Bash, Read grid.jpg]` → EN `"Ran 2 commands, read grid.jpg"`; unknown tool → `"Used WebFetch"`; cache evicts least recently used at 31.
- [ ] **Step 2:** `npm --prefix ui test -- conversation` — Expected: FAIL.
- [ ] **Step 3: Implement** + dictionary keys.
- [ ] **Step 4:** `npm --prefix ui test` — Expected: PASS.
- [ ] **Step 5: Commit** — "Add conversation paging state and step summaries".

---

### Task 7: Markdown subset renderer

**Files:**
- Create: `ui/src/conversation/Markdown.tsx`, test `ui/src/conversation/Markdown.test.tsx`

**Interfaces:**
- `export default function Markdown({ text }: { text: string }): JSX.Element` — paragraphs (blank-line separated), `#`..`###` headings, `**bold**`, `*italic*`, `` `code` ``, fenced code blocks (``` with optional language, shown as `<pre><code>`), `-`/`*`/`1.` lists (one level), links `[label](url)` → `<span title={url}>label</span>`. Builds React elements only.

- [ ] **Step 1: Failing tests:** each construct renders the expected element; `html stays text`: `"<script>alert(1)</script> <img src=x onerror=y>"` → no `script`/`img` element, text visible; `"[x](javascript:alert(1))"` → no `a` element; unclosed ``` renders the rest as code.
- [ ] **Step 2:** `npm --prefix ui test -- Markdown` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** `npm --prefix ui test -- Markdown` — Expected: PASS.
- [ ] **Step 5: Commit** — "Render a safe markdown subset for Claude's replies".

---

### Task 8: Conversation tab, image popup and composer

**Files:**
- Create: `ui/src/conversation/ConversationPage.tsx`, `ImageThumb.tsx`, `ImageViewer.tsx`, `Composer.tsx`, test `ui/src/conversation/ConversationPage.test.tsx`
- Modify: `ui/src/components/Sidebar.tsx` (`SessionTab = "terminal" | "conversation" | "info"`), `ui/src/components/SessionTabs.tsx`, `ui/src/sessionView.ts` (`resolveView`: the host branch keeps `session` + `terminal` for every tab except `"terminal"`), `ui/src/App.tsx` (`selectTab`: same rule; page switch renders `ConversationPage` for `tab === "conversation"`), `ui/src/features.css`, 8 dictionaries, `ui/src/sessionView.test.ts`

**Interfaces:**
- Consumes: Task 4 API, `applyPage`/`hasOlder`/`lastIndex`/`stepSummary`/`createImageCache` (Task 6), `Markdown` (Task 7), `sendToSession` (Task 5), existing `SessionTerminalPlaceholder` and `terminalForPid`.
- `ConversationPage({ sessionId, card, onShowTerminal, onTerminal, onAdopted, onResume })` — `card` from the snapshot (undefined/pid 0 = closed).
  - Mount: `getConversationPage(sessionId)`; scroll to bottom.
  - `card.lastActivityMs` change → `getConversationSince(sessionId, epoch, lastIndex)` → `applyPage`; keep pinned to bottom if the user was within 40 px of it.
  - Scrolled within 200 px of the top and `hasOlder` → `getConversationPage(sessionId, state.start)`; keep the visible content in place (adjust `scrollTop` by the height delta).
  - Items: `User` right-aligned bubble with small thumbnails; `Assistant` → `Markdown`; `Steps` one muted line `stepSummary(...) ›` with thumbnails of its steps at the right end, click expands to rows `tool target` (error rows marked); `Compacted` → divider `conv.compacted`; `truncated` → note `conv.truncated`.
  - Bottom: `card` live and `terminalForPid(card.parentPid) !== undefined` → `Composer`; otherwise the existing `SessionTerminalPlaceholder` (outside app → host line, Go to tab, Move to app; closed → Resume).
- `ImageThumb({ sessionId, image, onOpen })` — loads via `getConversationImage` when visible (IntersectionObserver; immediate load when it is unavailable, as in jsdom), `data:` URL, cache from Task 6; on error a broken-image placeholder (`conv.imageFailed`).
- `ImageViewer({ src, onClose })` — modal overlay, image fit to window, closes on Esc, close button (`conv.imageClose`) or backdrop click.
- `Composer({ sessionId, status, onShowTerminal })` — textarea; Enter sends, Shift+Enter newline; `status === "waiting"` → disabled with `conv.waiting` and a button `conv.toTerminal` calling `onShowTerminal`; on success clears; on error shows `conv.sendFailed {err}` (via `errorText`) and keeps the text.
- `SessionTabs`: tabs `terminal`, `conversation`, `info`; labels `tabs.terminal`, `tabs.conversation`, `tabs.info`.
- New keys (EN / TR): `tabs.conversation` "Conversation" / "Sohbet"; `conv.loading` "Loading…" / "Yükleniyor…"; `conv.empty` "No messages yet." / "Henüz mesaj yok."; `conv.compacted` "Conversation compacted" / "Sohbet özetlendi"; `conv.truncated` "(shortened)" / "(kısaltıldı)"; `conv.imageFailed` "Image unavailable" / "Resim açılamadı"; `conv.imageClose` "Close" / "Kapat"; `conv.placeholder` "Message Claude… (Enter sends, Shift+Enter new line)" / "Claude'a yazın… (Enter gönderir, Shift+Enter yeni satır)"; `conv.send` "Send" / "Gönder"; `conv.waiting` "Answer the approval in the Terminal tab" / "Onayı Terminal sekmesinde verin"; `conv.toTerminal` "Terminal tab" / "Terminal sekmesi"; `conv.sendFailed` "Could not send: {err}" / "Gönderilemedi: {err}".

- [ ] **Step 1: Failing tests** (mock `../api` as the existing page tests do): renders user/assistant/steps from a page; `thumb opens viewer and Esc closes it`; `thumb shows placeholder on error`; `since appends on lastActivity change`; composer: Enter calls `sendToSession` with the text and clears, Shift+Enter does not send, `waiting` disables and the button calls `onShowTerminal`, `send error keeps text`; outside-app card shows the Move to app button instead of the composer; `sessionView.test.ts`: conversation tab keeps `{ session, tab: "conversation", terminal }` when the session is hosted, and follows a `/clear` to the new id.
- [ ] **Step 2:** `npm --prefix ui test` — Expected: new tests FAIL.
- [ ] **Step 3: Implement** components, wiring, CSS (reuse theme variables; thumbnails 56 px high, popup max 90 vw/90 vh).
- [ ] **Step 4:** `npm --prefix ui test && npm --prefix ui run build` — Expected: PASS, build OK.
- [ ] **Step 5: Commit** — "Show the conversation with Claude's images in a session tab".

---

### Task 9: Remote Control icon in the UI

**Files:**
- Modify: `ui/src/components/SessionCard.tsx`, `ui/src/components/SessionTabs.tsx` (prop `remote?: () => void`), `ui/src/App.tsx`, `ui/src/features.css`, 8 dictionaries, tests in `ui/src/ui.test.tsx`

**Interfaces:**
- Consumes: `SessionCard.remoteControl`, `openRemoteControl` (Task 5).
- Card: small icon button next to the name when `remoteControl`; click stops propagation and calls `openRemoteControl`; error shown in the card's existing meta line style. Tab row: same icon at the right end before the split controls.
- Keys: `remote.on` "Remote Control is on: open on claude.ai" / "Remote Control açık: claude.ai'de aç"; `remote.openFailed` "Could not open claude.ai: {err}" / "claude.ai açılamadı: {err}".

- [ ] **Step 1: Failing tests:** icon only when `remoteControl`; click calls `openRemoteControl(sessionId)` and does not open the session; tab row shows it for a remote session.
- [ ] **Step 2:** `npm --prefix ui test` — Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4:** `npm --prefix ui test` — Expected: PASS.
- [ ] **Step 5: Commit** — "Show a Remote Control mark on sessions that have it on".

---

### Task 10: README and verification on this machine

**Files:**
- Modify: `README.md` (Features: "Conversation tab" subsection; the Remote Control mark; Turkish summary section too), `docs/superpowers/specs/2026-11-18-conversation-tab-design.md` (record findings below)

- [ ] **Step 1:** README feature text (what the tab shows, images and popup, message box rules, Remote Control mark).
- [ ] **Step 2:** `cargo test --workspace && cargo clippy --workspace -- -D warnings && npm --prefix ui test && node scripts/gen-notices.mjs --check` — Expected: all PASS, notices unchanged.
- [ ] **Step 3: Manual check with `npm run tauri dev`:**
  - A session that read an image: thumbnail on its step, popup opens and closes (Esc, backdrop, button).
  - Send from the box while idle and while busy (queued by Claude Code); multi-line text arrives as one message.
  - Open an AskUserQuestion prompt and a plan-mode approval in a test session; record the registry `status`/`waitingFor` each shows (`~/.claude/sessions/<pid>.json`) in the spec's review section.
  - `/compact` in a test session: confirm the `compact_boundary` shape assumed in Task 2 and the divider; Esc Esc rewind: view switches to the new branch.
  - `claude --remote-control` session: icon on card and tab row, click opens claude.ai in the browser.
  - Closed session from History: Conversation tab readable, Resume at the bottom.
- [ ] **Step 4: Commit** — "Describe the conversation tab".
