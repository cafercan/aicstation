/**
 * App-wide keyboard chords (pure; the window listener lives in App.tsx).
 *
 * Every chord is Ctrl+Shift+<key>, Ctrl+Tab or Ctrl+Alt+<digit>, chosen so
 * none of them means anything to PowerShell/PSReadLine, Claude Code or the
 * embedded terminal's own keys (terminal/keys.ts: Ctrl+C/V/F, Ctrl+=/-/0).
 * Anything that is not one of these chords returns null and must reach xterm
 * untouched. Ctrl+Alt is also how Windows reports AltGr, which types
 * characters on many layouts (Turkish Q: AltGr+7 = "{"), so a Ctrl+Alt+digit
 * only counts when the key really is that digit and AltGraph is not down.
 */

export type ActionId =
  | "newTerminal" | "closeTerminal" | "switchTerminal" | "dashboard" | "history" | "focusSearch"
  | "nextTerminal" | "prevTerminal" | "palette" | "toggleSplit" | "shortcuts";

export interface Binding {
  id: ActionId;
  chord: string;
  /** Terminal number (1-based) of `switchTerminal`. */
  n?: number;
}

export interface Chord { ctrl: boolean; shift: boolean; alt: boolean; key: string }

/** What the window listener needs from a KeyboardEvent. */
export interface KeyEventLike {
  type: string;
  key: string;
  code: string;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  metaKey: boolean;
  /** `getModifierState("AltGraph")`. */
  altGraph?: boolean;
}

const DIGITS = [1, 2, 3, 4, 5, 6, 7, 8, 9];

export const KEYMAP: readonly Binding[] = [
  { id: "newTerminal", chord: "Ctrl+Shift+T" },
  { id: "closeTerminal", chord: "Ctrl+Shift+W" },
  ...DIGITS.map((n): Binding => ({ id: "switchTerminal", chord: `Ctrl+Alt+${n}`, n })),
  { id: "nextTerminal", chord: "Ctrl+Tab" },
  { id: "prevTerminal", chord: "Ctrl+Shift+Tab" },
  { id: "dashboard", chord: "Ctrl+Shift+D" },
  { id: "history", chord: "Ctrl+Shift+H" },
  { id: "focusSearch", chord: "Ctrl+Shift+F" },
  { id: "palette", chord: "Ctrl+Shift+P" },
  { id: "toggleSplit", chord: "Ctrl+Shift+\\" },
  { id: "shortcuts", chord: "Ctrl+Shift+/" },
];

/** "Ctrl+Shift+T" → chord; null when malformed. Modifier order does not matter. */
export function parseChord(s: string): Chord | null {
  const parts = s.split("+");
  // "Ctrl++" style keys are not used; a trailing "+" is malformed.
  const key = parts.pop();
  if (!key) return null;
  const c: Chord = { ctrl: false, shift: false, alt: false, key: key.toLowerCase() };
  for (const m of parts) {
    if (m === "Ctrl") c.ctrl = true;
    else if (m === "Shift") c.shift = true;
    else if (m === "Alt") c.alt = true;
    else return null;
  }
  return c;
}

/** Normalized key of an event: a letter, a digit, "/", "\\", "tab", or null. */
function eventKey(ev: KeyEventLike): string | null {
  const digit = /^(?:Digit|Numpad)([0-9])$/.exec(ev.code)?.[1];
  if (digit !== undefined) return ev.key === digit ? digit : null;
  if (ev.code === "Slash" || ev.key === "/" || ev.key === "?") return "/";
  if (ev.code === "Backslash" || ev.key === "\\" || ev.key === "|") return "\\";
  if (ev.key === "Tab") return "tab";
  if (/^[a-z]$/i.test(ev.key)) return ev.key.toLowerCase();
  return null;
}

const PARSED = KEYMAP.map((b) => ({ b, c: parseChord(b.chord)! }));

/** The app binding of a keydown, or null (the key belongs to the page / terminal). */
export function matchChord(ev: KeyEventLike): Binding | null {
  if (ev.type !== "keydown" || ev.metaKey || ev.altGraph || !ev.ctrlKey) return null;
  const key = eventKey(ev);
  if (key === null) return null;
  const hit = PARSED.find(({ c }) => c.key === key && c.ctrl && c.shift === ev.shiftKey && c.alt === ev.altKey);
  return hit?.b ?? null;
}

/** Next (`delta` 1) or previous (-1) id after `current`, wrapping; first/last when current is not in `ids`. */
export function cycle(ids: number[], current: number | null, delta: 1 | -1): number | null {
  if (ids.length === 0) return null;
  const i = current === null ? -1 : ids.indexOf(current);
  if (i < 0) return delta === 1 ? ids[0] : ids[ids.length - 1];
  return ids[(i + delta + ids.length) % ids.length];
}
