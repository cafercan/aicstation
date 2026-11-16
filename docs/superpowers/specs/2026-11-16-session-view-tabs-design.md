# Session view: Terminal and Info tabs, "Move to app"

Date: 2026-11-16
Status: approved in conversation, pending doc review

## Intent

Today a session has two unrelated places in the UI: the info page
(`SessionDetailPage`, opened from a dashboard card) and, when the session runs
in the app, its terminal tab (`{ terminal: id }` view). The user wants them to
be one place: a session view with two tabs at the top, **Terminal** and
**Info**, switched by clicking the tab.

For sessions running outside the app (Windows Terminal or another host) the
Terminal tab cannot show the live terminal. It shows a placeholder with "Go to
tab" and a new **Move to app** action that ends the external `claude` process
and resumes the same session in an app terminal - but only when that cannot
interrupt work.

Ships in one release together. Built and tried on this machine before any
release is cut.

## Goals

- One session view, tab bar at the top: `Terminal | Info`.
- Dashboard card click opens the session view. Default tab: Terminal when the
  session runs in an app terminal, Info otherwise.
- Tab per case:
  1. **Live, in app terminal**: Terminal tab is the real xterm (the existing
     `TerminalView`, Claude bar included).
  2. **Live, outside the app**: Terminal tab is a placeholder: where it runs,
     "Go to tab" (when `gotoTarget` gives one), and "Move to app".
  3. **Closed**: Terminal tab offers "Resume" (existing `openResumeTerminal`
     flow, errors via `resumeError`). After resume the view becomes case 1.
  4. **Plain shell terminal, no Claude session in it**: no tab bar, terminal
     only (today's behaviour).
- "Move to app" enabled only when the session is idle: registry status
  `idle` **and** no running subagent (`AgentState::Running` in its tree).
  Disabled otherwise, with the reason ("busy", "waiting for approval",
  "N agents running"). Confirm dialog before acting; the dialog says
  background shell commands started by Claude may be stopped.

## Non-goals

- Attaching to an external process's console (not possible).
- Moving a busy/waiting session, queueing a move until idle.
- Moving an in-app session out to Windows Terminal.
- Closing the Windows Terminal tab after a move (its shell stays, prompt shown).

## Anti-goals

- Killing a session that is doing work. The idle check is re-done in the
  backend right before the kill; the UI's enabled state is not trusted.
- Two `claude` processes on one session at the same time.
- Killing a reused pid: the kill verifies the process start time
  (`procStart`) matches the registry entry.

## Approaches

- **A. End the external process, then resume in app (picked).** Backend
  re-checks idle, kills the verified pid, waits until the session is no longer
  live, then runs the existing `pty_spawn_resume` path. Cross-platform, reuses
  resume, the cwd derivation and the resume hold.
- B. Type `/exit` into the Windows Terminal tab via UI Automation. Windows
  only, brittle tab lookup. Dropped.
- C. `claude --resume <id> --fork-session` in app, external keeps running.
  Creates a second, diverging conversation - not a move. Dropped.

## Design

### UI (React, `ui/src`)

- View model: the session view is keyed by session id with a tab:
  `{ session: id, tab: "terminal" | "info" }`. When the tab is `terminal` and
  the session has an embedded terminal (`terminalForPid`), `termId` is that
  terminal so the persistent `TerminalArea` shows it; otherwise the page area
  shows the placeholder. `{ terminal: id }` stays for plain shells; the
  sidebar's terminal entry for a terminal hosting a session opens the session
  view on its Terminal tab.
- New `SessionTabs` bar at the top of the main area; tabs only switch `tab`.
  Info tab content is today's `SessionDetailPage` (its own "Go to tab" /
  "Resume" buttons stay).
- Tray/toast navigation (`onNavigate`) and History "open" go to the session
  view with the default tab rule above.
- Split view: the tab bar follows the active pane's session; switching to Info
  leaves the split state intact for when Terminal is chosen again.
- Sidebar selection: a session view whose session is in an app terminal
  highlights that terminal's sidebar entry.
- i18n: Turkish and English strings for tabs, placeholder, reasons, dialog,
  move errors.

### Backend (Rust)

- New command `adopt_session(session_id) -> TerminalInfo` (name is the
  builder's call), in `src-tauri/src/commands.rs`:
  1. Validate id (`is_session_id`).
  2. From the monitor's current state: session must be live, not hosted in an
     app PTY, status `Idle`, zero running agents. Else error code
     (`adopt_busy`, `adopt_waiting`, `adopt_agents_running`, `adopt_not_live`,
     `adopt_embedded`).
  3. Kill the pid after verifying start time equals `procStart`
     (Windows: `OpenProcess` + `TerminateProcess`; Unix: `SIGTERM`, then
     `SIGKILL` after a short grace).
  4. Wait until the session is not live (poll registry, ~10 s timeout,
     `adopt_timeout` on expiry).
  5. Call the existing resume path (`check_resume_terminal`,
     `resume_target`, `spawn_resume`) and return its result.
- Process kill lives behind the `platform` module (Windows / Unix impls,
  stubs where unsupported).

## Constraints

- Existing resume guard (`session_live`, `commands.rs:318`) must still refuse
  a plain resume of a live session.
- Session ↔ terminal mapping stays `session.parentPid === terminal.pid`.
- Tauri permission for the new command (autogenerated toml + capability).
- Tests: Rust unit tests for the idle gate and pid verification; Vitest for
  tab default rule, view mapping, placeholder states, disabled reasons.
- Verification before release: `npm test`, `cargo test`, local
  `tauri build`, manual try on this machine: move an idle WT session, try a
  busy one (button disabled, backend refuses), closed session resume, plain
  shell terminal.

## Decisions from review (author's calls, open to correction)

- **View follows the terminal, not a fixed id.** When the session view is
  showing an app terminal, the view also holds that terminal id. If the
  session inside it changes (`/clear`, `/resume` give the same pid a new id)
  the view moves to the new id; if Claude exits and the terminal is a plain
  shell again, the view becomes that plain terminal (no tab bar). A plain
  shell that starts `claude` gains the tab bar. Right after "Move to app" /
  "Resume", the view is the new terminal until the registry lists the
  session, so the user never sees the case-3 placeholder in between.
- **Which outside sessions can be moved:** only registry entries with
  `kind: "interactive"` and `entrypoint: "cli"` (both present in
  `~/.claude/sessions/<pid>.json`; add them to `RawSession`). Any host
  (Windows Terminal, VS Code terminal, other terminals). Others (SDK, `-p`,
  IDE extension) get no button; placeholder says why.
- **Resume folder:** the backend reads the session's first recorded cwd from
  its live transcript **before** the kill and passes it to the spawn, so no
  wait for history indexing. Rule "first recorded cwd, never from UI" holds.
- **Launch flags are not carried over (user's decision).** The moved session
  starts with the app's own launch settings; flags, model, permission mode,
  `--add-dir`, MCP config and environment of the external process are lost.
  The confirm dialog says so: "Command-line options given outside are not
  carried over; the app's settings are used."
- **Idle re-check** forces a fresh registry read, not the cached snapshot.
- **Languages:** all nine dictionaries get the new strings and `adopt_*`
  error codes (the i18n test requires it); the builder writes them.

## Left to the builder

- Exact component split and file names.
- Command and error code names.
- Placeholder layout and copy (TR/EN), within the cases above.
