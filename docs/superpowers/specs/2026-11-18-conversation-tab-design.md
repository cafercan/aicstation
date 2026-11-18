# Session view: Conversation tab with images, Remote Control mark

Date: 2026-11-18
Status: agreed in conversation, pending doc review

## Intent

Claude Code's terminal UI cannot show images. When Claude reads a screenshot
or an image file (`Read grid.jpg`), the user sees only a text line; on
claude.ai/code the same step shows a thumbnail that opens a larger view on
click. The user's main pain: **seeing the images Claude shows them**. They
want a claude.ai-like conversation view for their sessions inside the app,
with those images, and the ability to write to the session from it.

Secondary: sessions that have Remote Control turned on should be
recognisable on the dashboard by a small mark.

Investigated and dropped (see Approaches): live view of sessions running on
other machines. Without the app talking to claude.ai itself, the CLI offers no
way to attach to them (`claude --cloud <url>` → "Attaching to an existing
cloud session is not enabled for your account"; `claude --teleport` makes a
local copy). The user will run `claude --teleport` in a terminal themselves
when they want one here; the copy is then an ordinary local session and gets
everything below.

## Goals

1. **Conversation tab** in the session view: `Terminal | Conversation | Info`.
   - Shows the session's conversation from its transcript: user messages,
     Claude's text replies, and Claude's tool steps grouped claude.ai style
     ("Ran a command, read grid.jpg").
   - **Images inside tool results** (Read of an image file, screenshots, any
     `tool_result` content block of `type: "image"`) appear as thumbnails on
     their step. Click → a large view in a popup over the app; Esc, the close
     button or a click outside closes it.
   - Images the user pasted (Alt+V; `type: "image"` blocks in user messages)
     appear as small thumbnails in that message. Lower priority than the
     above, same mechanism.
   - Updates live while the session writes to its transcript.
   - Works for every session the app knows: live in an app terminal, live
     outside the app, closed (opened from the dashboard or History).
2. **Message box** at the bottom of the Conversation tab:
   - Session live **in an app terminal**: enabled. Enter sends, Shift+Enter
     inserts a newline. Sending writes the text into that terminal's PTY as
     if typed, then Enter. Claude Code's own behaviour applies (a message sent
     while Claude works is queued by Claude Code; slash commands work).
   - Status **waiting for approval**: box disabled, with the note "Answer the
     approval in the Terminal tab" and a button that switches to the
     Terminal tab. (Typing there would answer the approval prompt.)
   - Session live **outside the app**: no box. One line: "This session runs
     outside the app", with the existing **Move to app** button (same
     enable rules and confirm dialog as on the Terminal tab).
   - Session **closed**: no box; the existing **Resume** button.
3. **Remote Control mark**: a live session whose registry file
   (`~/.claude/sessions/<pid>.json`) has a `bridgeSessionId` gets a small
   icon on its dashboard card and in the session view's tab row (tooltip:
   "Remote Control is on"). Clicking it opens
   `https://claude.ai/code/<bridgeSessionId>` in the system browser.

## Non-goals

- Viewing or chatting with sessions on other machines or in the cloud; any
  list of remote sessions (including scraping the `claude --teleport` picker).
- Sending images from the message box (user pastes with Alt+V in the
  terminal).
- Subagent conversations (a Task/Agent step shows as one step line; the Info
  tab keeps the agent tree).
- Showing Claude's thinking blocks, full tool inputs or tool output text.
- Remote Control mark for closed sessions.
- Editing, forking or rewinding the conversation from this view.

## Anti-goals

- **The app makes no network request.** Everything comes from local files;
  the only new outward action is handing a claude.ai URL to the system
  browser on an explicit click. README's privacy section stays true and gets
  one line about that link.
- No reading of `.credentials.json`, `sessions/*.key` or any token.
- Text sent from the box must never land in an approval prompt or in a
  terminal that no longer hosts that session (re-checked in the backend, see
  Design).
- Opening a long, image-heavy transcript must not freeze the UI or move
  megabytes of base64 on every refresh.
- No raw HTML from transcript text reaches the DOM.

## Constraints

- Claude Code's transcript format is undocumented (README note applies).
  Unknown line and block types are skipped silently.
- Observed shapes (Claude Code 2.1.327):
  - `{"type":"user","message":{"content":"text"}}` or `content` as an array
    of `text` / `image` / `tool_result` blocks.
  - `tool_result.content` may be a string or an array; image blocks are
    `{"type":"image","source":{"type":"base64","media_type":"image/png|image/jpeg",...,"data":"..."}}`.
    A single line with one image was ~130-300 KB.
  - `{"type":"assistant","message":{"id":...,"content":[thinking|text|tool_use...]}}`;
    one API message may be spread over several lines with the same
    `message.id` (existing parser already handles this for usage).
  - Registry: `"bridgeSessionId":"session_01..."` present only while Remote
    Control is on. Transcripts also carry `{"type":"bridge-session",
    "bridgeSessionId":"cse_01..."}` lines (different prefix; not used).
- CSP already allows `img-src 'self' data:`; no CSP change needed.
- Arguments from the UI are untrusted, as everywhere in the app: session ids
  must be UUIDs; the URL is built on the Rust side from the registry, never
  taken from the UI.
- New Tauri commands need their permission toml + capability entry.
- All 8 UI languages get the new strings (the i18n test requires it).
- Tests: Rust unit tests for the conversation builder (grouping, images,
  caps, broken lines, incremental reads); Vitest for the tab, box states,
  image popup, live append.

## Approaches

- **A. Rust builds the conversation, UI pages through it, images on demand
  (picked).** Core streams the transcript into a list of conversation items
  (user message, assistant text, tool-step group) with image *references*
  (line byte offset + block index + media type + size), not image bytes. UI
  asks for the newest page, older pages on scroll-up, new items since a
  cursor while live, and each image only when its thumbnail scrolls into
  view. Matches how the app already works (core parses files, UI asks via
  validated IPC) and keeps big payloads out of the refresh path.
- B. Rust sends raw transcript lines, TypeScript parses. Faster to write, but
  the parsing logic lives in two places and image-heavy transcripts push
  megabytes of base64 to the UI on each load. Dropped.
- C. Pre-index every transcript in the background (like History). Too heavy
  for opening one session. Dropped.
- For sending: write to the hosting PTY (picked). The registry's
  `messagingSocketPath` peer socket was seen but its protocol is undocumented
  and not needed. Dropped.

## Design

### Core (`core/`, Rust)

- New module (e.g. `conversation.rs`):
  - `ConvItem` variants:
    - `User { text, images: Vec<ImageRef>, at }` — skip lines that carry
      only `tool_result` blocks (they attach to tool steps) and Claude Code's
      synthetic/meta user lines (e.g. `isMeta: true`, command wrappers);
      builder decides the exact filter from real transcripts.
    - `AssistantText { text, at }` — consecutive `text` blocks of one turn.
    - `Steps { steps: Vec<Step>, at }` — consecutive tool calls between two
      texts; `Step { tool, target, is_error, images: Vec<ImageRef> }`,
      `target` via the existing `tool_target`.
  - `ImageRef { offset: u64, block: u32, media_type, approx_bytes }` where
    `offset` is the byte offset of the JSONL line and `block` identifies the
    image inside it (builder defines the indexing for nested tool_result
    content).
  - Only `image/png`, `image/jpeg`, `image/gif`, `image/webp` with
    `source.type == "base64"` become refs; others are ignored.
  - Builder is incremental: keeps the reader offset so live updates parse
    only new bytes (reuse `TranscriptReader`). Each item gets a stable
    increasing index used as the cursor.
  - Caps: text per item capped (e.g. 64 KB, marked truncated); line size cap
    as in the existing reader; items kept in memory per open session only.
- `read_image(path, offset, block) -> (media_type, base64)`: seeks to the
  offset, reads that one line (cap e.g. 20 MB), re-parses, returns the block
  after re-checking media type. Fails if the line no longer matches.
- Registry: `RawSession` gains `bridge_session_id`; accepted only if it
  matches `^session_[A-Za-z0-9]{1,64}$`. `LiveSession` and `SessionCard`
  expose `remote_control: bool` (the id itself is not sent to the UI).

### App (`src-tauri/`)

- `get_conversation(session_id, before: Option<u64>, limit) -> Page` and
  `get_conversation_since(session_id, cursor) -> Page` (names are the
  builder's call). `Page` = items + `has_older` + `cursor`. Transcript path
  resolved by session id as for the Info tab; a small cache keeps the
  builder of the last opened session(s) (e.g. 2) so live polling is cheap.
- `get_conversation_image(session_id, offset, block) -> { mediaType, data }`.
  The offset is checked against the refs the builder produced for that
  session (not trusted blindly).
- `send_to_session(session_id, text) -> ()`: backend re-checks that the
  session is live, hosted in an app PTY, and its fresh registry status is not
  `waiting`; then writes the text to that PTY using bracketed paste
  (`ESC[200~ … ESC[201~`) followed by `\r`. Text length capped (e.g. 32 KB);
  control characters other than `\n`/`\t` stripped. Error codes for each
  refusal, translated in the UI.
- `open_remote_control(session_id)`: looks up the live session's
  `bridgeSessionId`, builds `https://claude.ai/code/<id>`, opens it with the
  system browser (Windows `ShellExecuteW`/`explorer`, Linux `xdg-open`),
  following how `open_folder` launches things.
- Live updates: when a snapshot shows the open session's `lastActivityMs`
  changed, the UI calls `get_conversation_since` (no new event needed).

### UI (`ui/`, React)

- `SessionTab` becomes `"terminal" | "conversation" | "info"`; `SessionTabs`
  shows three tabs. Default tab rule from the previous spec stays (Terminal
  when in an app terminal, otherwise Info). A plain shell terminal still has
  no tab bar.
- `ConversationView`:
  - Loads the newest page (~100 items), keeps scroll pinned to the bottom
    while the user is at the bottom; loads older pages when scrolled to top
    without jumping.
  - User messages right-aligned bubbles; assistant text full width; steps as
    one muted line ("Ran 2 commands, read grid.jpg ›") that expands to the
    step list; thumbnails at the right end of the step line (as on
    claude.ai) and in the expanded list.
  - Markdown subset for assistant text: paragraphs, headings, bold/italic,
    inline code, fenced code blocks, lists, links shown as text (not
    clickable). Rendered by building React elements, never
    `dangerouslySetInnerHTML`. Own small renderer, no new dependency
    (author's call; keeps `THIRD_PARTY_NOTICES` and audits unchanged).
  - Thumbnails load lazily (IntersectionObserver), `data:` URL from the
    image command, fixed small size; an in-memory LRU of loaded images
    (e.g. 30) per view.
  - Image popup: modal over the app, image fit to the window, Esc / close
    button / click outside to close.
  - Message box per Goals 2; after sending, the box clears; the message
    appears when Claude Code writes it to the transcript (no optimistic
    copy).
- Remote Control icon on `SessionCard` and in the tab row; click calls
  `open_remote_control`.
- History page: opening a closed session lands on its session view as today;
  Conversation tab available.

## Decisions from review (author's calls, open to correction)

- **Current branch only, like Claude Code and claude.ai.** The transcript is
  a tree linked by `parentUuid`; rewinds, edits and retries leave abandoned
  branches in the file. The view shows the chain followed back from the
  latest message. The builder keeps the tree (uuid → parent, item) in memory
  for the open session. `since` returns new items when the new leaf's chain
  extends what was already sent, otherwise `reset: true`, and the UI reloads
  the newest page (scroll pinned to bottom).
- **Compaction:** history before a `/compact` (or auto-compact) stays
  visible, followed across the compact boundary (its logical parent link),
  with a divider line "Conversation compacted" at the boundary. The summary
  text itself is not shown. Rationale: the user wants to find images Claude
  showed earlier; compaction should not hide them.
- **Sending while Claude works (user's decision):** the box is enabled for
  status `idle` and `busy`; refused for `waiting` or when the registry has a
  `waitingFor` value. Accepted risk: a prompt that Claude Code reports as
  `busy` (e.g. a multiple-choice question or plan approval, if they are not
  reported as `waiting`) could take the pasted text + Enter as an answer.
  The builder checks on this machine which status those prompts report and
  notes it in the plan; if they report `waiting`, the risk is gone.
- **Half-typed text in the terminal:** the box's text is pasted after
  whatever is already in Claude Code's input and sent together. No clearing
  keys are sent (they differ in multi-line and vim mode; Esc can rewind).
  Accepted: the user writes either in the box or in the terminal.

### Checked on this machine (Claude Code 2.1.327)

- An AskUserQuestion prompt shows in the registry as `status: "waiting"`,
  `waitingFor: "input needed"`.
- A plan-mode plan approval (ExitPlanMode) shows as `status: "waiting"`,
  `waitingFor: "permission prompt"`.
- So both prompts block the message box; the accepted risk in "Sending while
  Claude works" does not occur for them.
- `/compact` writes a `system` line with `subtype: "compact_boundary"`,
  `parentUuid: null` and `logicalParentUuid: <uuid>`, followed by a `user`
  line with `isCompactSummary: true` and `isVisibleInTranscriptOnly: true`,
  as the implementation assumes.
- Parallel tool calls branch off the `parentUuid` chain (tool_use #2 is a
  child of tool_use #1, with the same `message.id`). Decided during
  implementation: the view includes assistant lines with the same
  `message.id` and the results that answer tool uses it shows.
- A message typed while Claude works is written only as an `attachment` line
  (`attachment.type: "queued_command"`, `commandMode: "prompt"`,
  `origin.kind: "human"`, the text or content blocks in `attachment.prompt`);
  it shows as a user message. A finished background task arrives as a `user`
  line (or a queued_command) with `origin.kind: "task-notification"` and a
  `<task-notification>` text; it shows as a muted note with its `<summary>`.
  Other system-written prompts (`promptSource: "system"`, other origins),
  `<bash-stdout>`/`<bash-stderr>` lines are hidden; `<bash-input>` shows as
  `! cmd`.

## Left to the builder

- Module, component, command and error-code names; file split.
- Exact grouping and summary wording of steps ("Ran a command, read X"),
  within the claude.ai style above, in all 8 languages.
- Exact filter for meta/synthetic user lines, from real transcripts.
- Page size, caps and cache sizes within the ranges above.
- Icon artwork for the Remote Control mark (fits the existing card badges).
