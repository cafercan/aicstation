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

/** First (from the bottom) spinner row, trimmed, without key hints. */
export function findSpinner(rows: string[]): string | null {
  for (let i = rows.length - 1; i >= 0; i--) {
    const row = rows[i].trim();
    if (!SPINNER.test(row)) continue;
    const kept = splitTop(row).filter((p) => !isHint(p));
    return kept.join(" · ").trim() || null;
  }
  return null;
}

/** First (from the bottom) footer mode row, trimmed, without key hints. */
export function findModeLine(rows: string[]): string | null {
  for (let i = rows.length - 1; i >= 0; i--) {
    const row = rows[i].trim();
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
