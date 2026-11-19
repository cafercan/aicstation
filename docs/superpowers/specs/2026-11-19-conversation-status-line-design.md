# Conversation tab status line: design

## Intent

The user follows and drives sessions from the Conversation tab, but switches to
the Terminal tab for two reasons: to see whether anything is still going on,
and to find out why a message did not go. Claude Code shows the first on its
own screen (a spinner line such as `✽ Prestidigitating… (46s · ↓ 3.9k
tokens)` and a footer such as `⏵⏵ auto mode on · 1 shell`); the Conversation
tab shows neither. This line brings that into the tab, so the Terminal tab is
no longer needed to answer "is it still working?".

## Goals

- One status line, directly above the message box (or above the Move to app /
  Go to tab placeholder for a live session outside the app). Live sessions
  only; a closed session shows no line.
- It answers at a glance: what the session is doing, for how long, how much
  output this turn has produced, the permission mode, background shells still
  running, agents still running, and messages sent while Claude was busy that
  are still waiting in its queue (the second reason: "did my message go?").
- For a session in an app terminal, Claude's own spinner line and the mode
  part of its footer (`⏵⏵ auto mode on · 1 shell`) are shown verbatim when
  they are on the terminal's screen, so a Shift+Tab mode change shows at once.

## Non-goals

- Acting on the session from the line (stop, approve, change mode). Possible
  later, not here.
- Mirroring anything from Claude Code's screen besides the spinner line and
  the footer's mode line (no tips, no input box, no other footer lines).
- Our own list of whimsical spinner verbs.

## Anti-goals

- A shell count that stays up after the shells ended (a missed notification
  must not leave "1 shell" on forever).
- A line that flickers or jumps the conversation list as it updates.
- Reading the terminal buffer so often that typing or output slows down.

## Approaches considered

- **A. Built from data files only.** Status and its start time from the
  session registry; mode, background shells and turn tokens from the
  transcript. Works for every live session (also in Windows Terminal) and
  survives Claude Code screen changes. No spinner verb; tokens grow per
  finished message, not live.
- **B. Mirror the app terminal's screen.** The spinner and footer lines exactly
  as Claude draws them, live. Only for sessions in an app terminal; breaks
  when Claude Code changes its screen layout.
- **C. A as the base, B's spinner line on top (chosen).** Every live session
  gets A's line; when the session is in an app terminal and its spinner line
  is found on screen, that line replaces A's state/elapsed/tokens part.
  Chosen by the user: the look they asked for, with a fallback that always
  works. After the builder check the user also chose to take the footer's
  mode line from the screen (the transcript writes the mode only around
  turns, so a Shift+Tab change would show late), and to show queued
  messages.

## What the line shows

Parts, left to right, separated by ` · `; a part with nothing to say is left
out.

1. **State** (A): a coloured dot and the status label already used on cards
   (`çalışıyor`, `komut çalıştırıyor`, `onay bekliyor`, `hazır`), then, for
   every state except idle, the time since the status began (`fmtElapsed`),
   ticking every second. While working, the turn's output tokens follow:
   `↓ 3,9K token` (the app's existing token format).
   **Spinner (B):** for a session in an app terminal, when a spinner line is
   on screen, this whole part is that line's text instead, verbatim (for
   example `✽ Prestidigitating… (46s · ↓ 3.9k tokens)`); a trailing key hint
   such as `· esc to interrupt` is dropped, as in the footer.
2. **Mode and shells.** **Footer (B):** for a session in an app terminal,
   when the footer's mode line is on screen (it starts with `⏵⏵` or `⏸`, e.g.
   `⏵⏵ auto mode on · 1 shell · ↓ to manage`), its text is shown verbatim in
   place of parts 2 and 3 below; hints such as `(shift+tab to cycle)`,
   `esc to interrupt` or `↓ to manage` are dropped (they are keys of the
   terminal, not of this tab). **Fallback (A):**
   - Permission mode, only when not the default: `⏵⏵ otomatik mod` (auto),
     `⏵⏵ düzenlemeleri kabul` (acceptEdits), `⏸ plan modu` (plan),
     `⏵⏵ izinler atlanıyor` (bypassPermissions); another value as is after
     `⏵⏵`.
   - Background shells: `1 arka plan komutu` / `{n} arka plan komutu` when
     n > 0.
3. **Queued messages** (A, every live session): `1 mesaj kuyrukta` / `{n}
   mesaj kuyrukta` when your messages sent while Claude was busy are still
   waiting.
4. **Agents running**: the card's existing `agentsRunning`, with the existing
   `card.agentsRunning` wording, when > 0.

Sketch (working, in an app terminal, spinner and footer found, one message
queued):

```
  * Fermenting… (33s · ↓ 2.8k tokens · thinking with high effort) · ⏵⏵ auto mode on · 1 shell · 1 mesaj kuyrukta
 ┌──────────────────────────────────────────────────────────┐ ┌───────┐
 │ Claude'a yazın… (Enter gönderir, Shift+Enter yeni satır) │ │Gönder │
 └──────────────────────────────────────────────────────────┘ └───────┘
```

A session in Windows Terminal (no screen to read), idle with a background
shell (registry status "shell"):

```
  ● komut çalıştırıyor · 12 sn · ⏵⏵ otomatik mod · 1 arka plan komutu
```

Styling: one line, small muted text like the composer note, aligned with the
message box (same max width), ellipsis when too long (full text in a
tooltip). The line keeps its height whether or not it has content, so the
list does not jump.

## Data

### Registry (core `registry.rs`, already read)

- `status` and `statusUpdatedAt` → the card gets `statusSinceMs` (the
  existing `LiveSession.status_updated_at_ms`).

### Transcript (core `transcript.rs` / `stats.rs`)

New `Entry` variants from `parse_line`:

- `PermissionMode(String)` from `{"type":"permission-mode","permissionMode":"auto"}`.
- `BackgroundStarted(String)` from a `user` line's `tool_result` whose text
  (a string, or the text blocks of an array) starts with
  `Command running in background with ID: <id>`; the id runs to the next `.`
  or whitespace.
- `TaskFinished(String)` from a line whose text contains `<task-notification>`
  with `<task-id><id></task-id>`, whatever its `<status>` (completed, failed,
  killed): a `user` line (string or text blocks), and also an `attachment`
  line of the `queued_command` kind with `commandMode: "task-notification"`
  (the form the conversation parser already reads). In real transcripts most
  finishes come only in the attachment form.
- `UserPrompt` for a `user` line that is a real prompt: neither only
  `tool_result` blocks nor a task notification (the conversation parser's
  rules for "user message" apply).

- `Queue(op)` from `{"type":"queue-operation", "operation": …}`:
  `enqueue` (with `content`), `dequeue` (takes the oldest), `remove` (with
  `content`; `reason` such as `absorbed_mid_turn`, `delivered_to_agent`),
  `popAll` (the user took them all back to edit). An entry is a message when
  its `content` is not a `<task-notification>` (those are queued too and are
  not the user's).

`SessionStats` keeps:

- `queue`: the queued entries in order (message or notification), following
  those operations (`remove` takes the first entry with the same content);
  `queued_messages` is the number of message entries. Capped at 64.

- `permission_mode: Option<String>`, the last one seen.
- `background_tasks`: ids started and not yet finished, capped at 64 (oldest
  dropped).
- `turn_output_tokens: u64`: output tokens of assistant messages since the
  last `UserPrompt` (deduped by message id like the existing totals); reset to
  0 by `UserPrompt`.

### Session card (core `monitor.rs` `SessionCard`, UI `api.ts`)

New fields: `permissionMode: string | null`, `backgroundShells: number`,
`queuedMessages: number`, `turnOutputTokens: number`, `statusSinceMs: number`.

`queuedMessages` follows the same idle rule as `backgroundShells` below: an
idle session has an empty queue (Claude takes queued messages as soon as it
is free), so seeing idle clears it for good, entries written before the idle
time included.

`backgroundShells` is the transcript count, **cleared for good when the
registry status is seen idle**: Claude Code reports "shell" while any shell
runs, so idle means none is running, whatever the transcript missed (a missed
finish, or shells of a dead earlier process read again after a resume). The
clearing lives in the monitor, next to the registry: each started id keeps
the time of its line, and when a snapshot sees status idle with
`statusSinceMs` T, every id started before T is dropped. Ids started before
T but read later (Claude Code marks idle before it writes the turn's last
lines) are dropped on arrival for the same reason. With status "shell" and a
count of 0, the state label alone ("komut çalıştırıyor") says it.

## Spinner from the app terminal (UI)

- Only when the session's terminal is an app terminal
  (`terminalForPid(card.parentPid)` gives its id; `getTerminals()` holds the
  `TerminalEntry` with that id).
- Read `entry.term.buffer.active`: the screen rows at the bottom of the
  buffer (from `baseY`, `rows` lines), whatever the user scrolled to, bottom
  to top, `translateToString(true)` each.
- A spinner line: after optional spaces, one non-letter glyph, a space, a
  word, `…`, then ` (`. The glyph varies: `✽ ✶ ✻ ✢ ·` and, in a Windows
  console, a plain ASCII `*` (seen: `* Fermenting… (33s · ↓ 2.8k tokens ·
  thinking with high effort)`). The whole line is shown as drawn, so
  `thinking with high effort` and the like come along. The `⎿ Tip: …` line
  under it is not part of it. A finished line (`✻ Crunched for 3s · done
  10:40`) has no `…` and does not match.
- The footer's mode line: the row (in the same visible rows, below the input
  box) whose text, trimmed, starts with `⏵⏵` or `⏸`; its ` · `-separated
  parts that are key hints (`shift+tab to cycle`, `esc to interrupt`,
  `↓ to manage`, anything in parentheses) are dropped. Claude Code shows no
  mode line in default mode; then the shells part comes from A.
- Re-read on `term.onWriteParsed`, at most every 250 ms (trailing call so the
  last state is caught), and when the line mounts. Dispose on unmount.
- No spinner found: A's state part. No mode line found: A's mode and shells.

## i18n

New keys in all 8 languages: the four mode labels, `conv.bgShells` (plural),
`conv.queued` (plural), `conv.turnTokens` (`↓ {n} token`). State labels and agent wording reuse
existing keys.

## Tests

- core: each new `Entry` from sample lines; stats count start/finish pairs
  (user-line and attachment finishes), the cap, mode, the queue through
  enqueue/dequeue/remove/popAll with notifications not counted, and turn
  tokens reset by a prompt but not by tool results or task notifications;
  monitor clears shells and queue for good on idle, including a start line
  read after the idle time but written before it.
- UI: the footer mode line replaces A's mode and shells, its key hints
  dropped; `1 mesaj kuyrukta` shown only when > 0.
- UI: line parts for each status; mode hidden for default; shells and agents
  shown only when > 0; elapsed ticks (fake timers); spinner text replaces the
  state part when a fake terminal buffer holds one, not when it holds a
  finished line; no line for a closed session.

## Left to the builder

Exact spinner regex, the throttle mechanics, CSS details, where the new
component file lives, wording in the other seven languages.
