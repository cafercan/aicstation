/**
 * Pure keyboard/wheel decisions of the embedded terminal (Windows Terminal
 * behaviour). The xterm wiring lives in input.ts; this module has no DOM.
 */

export type KeyActionKind =
  /** Let xterm handle the key (e.g. Ctrl+C without selection sends ^C). */
  | "pass"
  /** Keep the key from xterm and do nothing else. */
  | "swallow"
  | "copy"
  /** Keep the key from xterm but leave the browser default, which fires the
   * native `paste` event that xterm turns into a (bracketed) paste. */
  | "paste"
  | "search"
  | FontAction;

export type FontAction = "fontUp" | "fontDown" | "fontReset";

export interface KeyLike {
  type: string;
  key: string;
  code: string;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  metaKey: boolean;
}

export const MIN_FONT_SIZE = 8;
export const MAX_FONT_SIZE = 32;
/** Windows Terminal's 10pt is 13 CSS px. */
export const DEFAULT_FONT_SIZE = 13;

/** What the combo means on keydown, ignoring the selection; null = not ours. */
function comboOf(ev: KeyLike): Exclude<KeyActionKind, "pass" | "swallow"> | null {
  // Alt (AltGr is reported as Ctrl+Alt) and Meta combos type characters or
  // belong to the shell.
  if (!ev.ctrlKey || ev.altKey || ev.metaKey) return null;
  switch (ev.key) {
    case "+": case "=": return "fontUp";
    case "-": return "fontDown";
    case "0": return "fontReset";
  }
  switch (ev.code) {
    case "NumpadAdd": return "fontUp";
    case "NumpadSubtract": return "fontDown";
    case "Numpad0": return "fontReset";
  }
  switch (ev.key.toLowerCase()) {
    case "c": return "copy";
    case "v": return "paste";
    case "f": return "search";
  }
  return null;
}

/**
 * Ctrl+C copies when there is a selection, otherwise ^C reaches the shell.
 * Ctrl+Shift+C always copies (no-op without selection). Ctrl+V / Ctrl+Shift+V
 * paste, Ctrl+F searches, Ctrl+= / Ctrl+- / Ctrl+0 change the font size.
 * Only keydown triggers an action; keypress/keyup of a handled combo are
 * swallowed so xterm never sees half of it.
 */
export function keyAction(ev: KeyLike, hasSelection: boolean): KeyActionKind {
  const combo = comboOf(ev);
  if (combo === null) return "pass";
  if (combo === "copy" && !hasSelection) return ev.shiftKey ? "swallow" : "pass";
  return ev.type === "keydown" ? combo : "swallow";
}

export function clampFontSize(n: number): number {
  if (!Number.isFinite(n)) return DEFAULT_FONT_SIZE;
  return Math.min(MAX_FONT_SIZE, Math.max(MIN_FONT_SIZE, Math.round(n)));
}

export function fontSizeAfter(action: FontAction, current: number): number {
  if (action === "fontReset") return DEFAULT_FONT_SIZE;
  return clampFontSize(current + (action === "fontUp" ? 1 : -1));
}

/** Ctrl+wheel changes the font size; null leaves the wheel to xterm (scroll). */
export function wheelFontAction(ev: { ctrlKey: boolean; deltaY: number }): FontAction | "swallow" | null {
  if (!ev.ctrlKey) return null;
  if (ev.deltaY < 0) return "fontUp";
  if (ev.deltaY > 0) return "fontDown";
  return "swallow";
}
