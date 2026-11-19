import type { SessionSkill } from "../api";

/** One entry of the message box's command menu. */
export interface SlashEntry {
  /** Name as typed after `/`. */
  name: string;
  description: string;
  /** Opens a picker in the terminal: running it also shows the Terminal tab. */
  terminal: boolean;
  /** Claude Code's own command (not a skill). */
  builtin: boolean;
}

const b = (name: string, description: string, terminal = false): SlashEntry =>
  ({ name, description, terminal, builtin: true });

/**
 * Claude Code's common built-in commands, in English as Claude Code shows
 * them. Kept by hand: a command Claude Code adds later is missing here
 * until this list is updated (it still runs when typed in full).
 */
export const BUILTINS: SlashEntry[] = [
  b("add-dir", "Add a new working directory"),
  b("agents", "Manage agent configurations", true),
  b("clear", "Clear conversation history and free up context"),
  b("compact", "Clear history but keep a summary in context"),
  b("config", "Open the settings panel", true),
  b("context", "Show current context usage"),
  b("cost", "Show the cost and duration of this session"),
  b("doctor", "Check the health of your Claude Code installation"),
  b("export", "Export the conversation to a file or the clipboard", true),
  b("help", "Show help and available commands"),
  b("hooks", "Manage hook configurations", true),
  b("init", "Create a CLAUDE.md file with codebase documentation"),
  b("mcp", "Manage MCP servers", true),
  b("memory", "Edit Claude memory files", true),
  b("model", "Set the model for Claude Code", true),
  b("permissions", "Manage allow and deny tool permission rules", true),
  b("plugin", "Manage Claude Code plugins", true),
  b("release-notes", "View release notes"),
  b("rename", "Rename the current conversation"),
  b("resume", "Resume a conversation", true),
  b("review", "Review a pull request"),
  b("rewind", "Restore the code and/or conversation to a previous point", true),
  b("status", "Show Claude Code status: version, model, account", true),
  b("todos", "List current todo items"),
  b("usage", "Show plan usage limits", true),
];

/**
 * The command name being typed: the text after a leading `/` while it has
 * no space or line break; `null` when the menu must not show.
 */
export function slashQuery(text: string): string | null {
  const m = /^\/([^\s]*)$/.exec(text);
  return m ? m[1] : null;
}

const byName = (a: SlashEntry, b: SlashEntry) => a.name.localeCompare(b.name);

/**
 * Entries whose name starts with `query` or, when none does, contains it
 * (case-insensitive); built-ins before skills, each group by name.
 */
export function matchCommands(query: string, skills: SessionSkill[]): SlashEntry[] {
  const q = query.toLowerCase();
  const all = [
    ...BUILTINS,
    ...skills.map((s) => ({ name: s.name, description: s.description, terminal: false, builtin: false })),
  ];
  const starts = all.filter((e) => e.name.toLowerCase().startsWith(q));
  const found = starts.length > 0 ? starts : all.filter((e) => e.name.toLowerCase().includes(q));
  return [...found.filter((e) => e.builtin).sort(byName), ...found.filter((e) => !e.builtin).sort(byName)];
}
