import type { Terminal } from "@xterm/xterm";

/**
 * Claude Code's own status lines, read from an app terminal's screen: the
 * spinner (`* Fermenting… (33s · ↓ 2.8k tokens · thinking with high effort)`)
 * and the footer's mode line (`⏵⏵ auto mode on · 1 shell · ↓ to manage`).
 * Key hints are dropped: they are keys of the terminal, not of the
 * conversation tab.
 */

/** One glyph that is no letter, digit or space; a word; `…`; then ` (`. */
const SPINNER = /^[^\p{L}\p{N}\s]\s+\p{L}[\p{L}'-]*…\s\(/u;
const HINTS = new Set(["shift+tab to cycle", "esc to interrupt", "↓ to manage", "ctrl+t to hide tasks"]);

/** `s` split at ` · ` outside parentheses. */
function splitTop(s: string): string[] {
  const parts: string[] = [];
  let depth = 0;
  let start = 0;
  for (let i = 0; i < s.length; i++) {
    const c = s[i];
    if (c === "(") depth++;
    else if (c === ")") depth = Math.max(0, depth - 1);
    else if (depth === 0 && s.startsWith(" · ", i)) {
      parts.push(s.slice(start, i));
      start = i + 3;
      i += 2;
    }
  }
  parts.push(s.slice(start));
  return parts;
}

const isHint = (p: string) => HINTS.has(p.trim().toLowerCase()) || /^\(.*\)$/.test(p.trim());

/** A rule of Claude Code's input box: `─` across the row (the lower one may carry a name). */
const isRule = (row: string) => row.trimStart().startsWith("─") && (row.match(/─/g)?.length ?? 0) >= 10;

/** Indices of the input box's top and bottom rules, -1 when not on screen. */
function inputBox(rows: string[]): { top: number; bottom: number } {
  let bottom = -1;
  for (let i = rows.length - 1; i >= 0; i--) if (isRule(rows[i])) { bottom = i; break; }
  let top = -1;
  for (let i = bottom - 1; i >= 0; i--) if (isRule(rows[i])) { top = i; break; }
  return { top, bottom };
}

/** First (from the bottom) spinner row above the input box, trimmed, without key hints. */
export function findSpinner(rows: string[]): string | null {
  const box = inputBox(rows);
  const end = box.top >= 0 ? box.top : box.bottom >= 0 ? box.bottom : rows.length;
  for (let i = end - 1; i >= 0; i--) {
    const row = rows[i].trim();
    if (!SPINNER.test(row)) continue;
    const kept = splitTop(row).filter((p) => !isHint(p));
    return kept.join(" · ").trim() || null;
  }
  return null;
}

/**
 * The footer mode row under the input box, trimmed, without key hints. Only
 * its left column counts: a notice on the right of the same row ("Context
 * left until auto-compact: 8%") is set off by a run of spaces.
 */
export function findModeLine(rows: string[]): string | null {
  const { bottom } = inputBox(rows);
  if (bottom < 0) return null;
  for (let i = rows.length - 1; i > bottom; i--) {
    const row = rows[i].trim().split(/\s{2,}/)[0];
    if (!row.startsWith("⏵⏵") && !row.startsWith("⏸")) continue;
    const kept = splitTop(row)
      // "plan mode on (shift+tab to cycle)": the hint rides on the part.
      .map((p) => p.replace(/\s*\([^()]*\)\s*$/, ""))
      .filter((p) => p.trim() !== "" && !isHint(p));
    return kept.join(" · ").trim() || null;
  }
  return null;
}

/** The terminal's screen rows (the bottom of the buffer, not the scrolled view). */
export function screenRows(term: Terminal): string[] {
  const buf = term.buffer.active;
  const rows: string[] = [];
  for (let y = buf.baseY; y < buf.baseY + term.rows; y++) {
    rows.push(buf.getLine(y)?.translateToString(true) ?? "");
  }
  return rows;
}
