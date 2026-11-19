# Slash command menu in the conversation tab: design

## Intent

The user drives sessions from the Conversation tab and often runs Claude
Code's built-in commands (`/compact`, `/clear` most of all) and skills. Text
starting with `/` sent from the message box already runs as a command (checked:
`/clear` from the box cleared the session). What is missing is the list
Claude Code shows when `/` is typed, so commands can be found and picked
without switching to the Terminal tab.

## Goals

- Typing `/` at the start of the message box opens a list above the box:
  built-in commands first, then the session's skills, each with a short
  description. Typing narrows it.
- Keys as in Claude Code's terminal: ↑/↓ move, Enter runs the highlighted
  command (puts `/name` in the box and sends it), Tab puts `/name ` in the box
  to add arguments, Esc closes the list and keeps the text. A click runs
  like Enter.
- A command that opens a picker in the terminal (for example `/model`
  without arguments, `/config`, `/resume`) is marked; running it also
  switches to the Terminal tab, where the picker is answered.

## Non-goals

- Running commands any other way than sending their text (no new backend
  command for them).
- A list for sessions outside an app terminal or closed ones (they have no
  message box).
- Argument completion (file paths, model names).

## Approaches considered

- **A (chosen by the user).** Built-in commands from a list kept in the app;
  skills from the session's own transcript (`skill_listing` attachment).
- **B.** Type `/` into the terminal unseen and read Claude Code's own menu
  from the screen. Always current, but sends keys into the user's terminal
  behind their back and can leave stray input. Rejected.
- **C.** Skills only. No upkeep, but `/compact` and `/clear`, the commands
  used most, would be missing. Rejected.

## Data

- **Built-ins**: a list in the UI (`ui/src/conversation/slashCommands.ts`):
  name, one-line description and a `terminal` flag for those that open a
  picker. Descriptions are English, as Claude Code shows them. The list
  covers the common commands: `/add-dir`, `/agents`, `/clear`, `/compact`,
  `/config`, `/context`, `/cost`, `/doctor`, `/export`, `/help`, `/hooks`,
  `/init`, `/mcp`, `/memory`, `/model`, `/permissions`, `/plugin`,
  `/release-notes`, `/rename`, `/resume`, `/review`, `/rewind`, `/status`,
  `/todos`, `/usage`. It needs updating when Claude Code adds commands
  (accepted cost of A).
- **Skills**: the `{"type":"attachment","attachment":{"type":"skill_listing",…}}`
  lines of the session's transcript. Each machine's sessions list that
  machine's skills (personal, project and plugin ones). Claude Code writes a
  full listing once (`isInitial: true`) and later adds listings of only the
  skills that appeared since (`isInitial: false`; checked: a skill added to
  the project while the session ran showed up in such a listing of one
  entry, and ran). So the skills are the union of the listings, an initial
  listing starting the set over. Each listing's `content` holds one `- name:
  description` line per skill (plugin skills carry their namespace, e.g.
  `caveman:caveman-init`). core keeps that union per live session while
  reading the transcript (names and descriptions, the description cut to its
  first sentence and at most 120 characters); a new app command
  `get_session_commands(sessionId)` returns it. A session without a listing
  yet gets an empty list, and the menu shows the built-ins only. A skill
  removed during a session stays listed until the next session (no removal
  listing was seen); sending it just gets Claude Code's own answer.
- The menu asks for the skills when it first opens for a session, and again
  each time it opens after the transcript grew (the card's `transcriptBytes`
  changed).

## Matching and order

- The list shows while the box text is `/` followed by no space and no line
  break (the user is typing the command name); a space or line break closes
  it.
- An entry matches when its name (without the `/`) starts with what was
  typed, or, failing any such match, contains it; case-insensitive.
  Built-ins come before skills; within each group, prefix matches first,
  then alphabetical.
- At most 8 rows show at a time; the list scrolls with the highlight. No
  match: the list closes.

## Look

```
 ┌─────────────────────────────────────────────────────────────┐
 │ /compact        Clear history but keep a summary in context │  ← highlighted
 │ /config         Open settings                    terminal ⧉ │
 │ /context        Show context usage                          │
 │ caveman:caveman-help   Quick-reference card for all caveman │
 └─────────────────────────────────────────────────────────────┘
 ┌──────────────────────────────┐ ┌───────┐
 │ /co                          │ │Gönder │
 └──────────────────────────────┘ └───────┘
```

Skills are shown with their name as Claude Code accepts it after `/`. The
`terminal` mark has a tooltip ("opens in the Terminal tab", translated).

## Tests

- Matching and order (prefix before contains, built-ins before skills, no
  match closes).
- Keys: ↑/↓ wrap, Enter sends `/name` and clears the box, Tab inserts
  `/name ` without sending, Esc closes and keeps the text; a `terminal`
  command also calls the Terminal-tab callback.
- core: skill listings are joined (an added listing extends the set, an
  initial one starts it over); descriptions cut; no listing gives an empty
  list.
- On screen: `/co` lists `/compact` first; Enter compacts the test session.
