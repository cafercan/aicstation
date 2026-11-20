import type { FileHint } from "../api";

/** The `@path` being typed at the caret: where its `@` is and what follows it. */
export interface AtQuery { start: number; end: number; query: string }

/**
 * The `@` word the caret is in or right after: an `@` at the start or after
 * white space, up to the caret, with no white space in between. `end` is
 * where the word ends (the caret may be inside it). `null` otherwise.
 */
export function atQuery(text: string, caret: number): AtQuery | null {
  let start = caret;
  while (start > 0 && !/\s/.test(text[start - 1])) start--;
  if (text[start] !== "@" || start >= caret) return null;
  // "a@b" is an address, not a file.
  if (start > 0 && !/\s/.test(text[start - 1])) return null;
  let end = caret;
  while (end < text.length && !/\s/.test(text[end])) end++;
  return { start, end, query: text.slice(start + 1, caret) };
}

/**
 * The text with the `@` word replaced by the picked entry, and the caret
 * after it. A folder ends in `/` and keeps the suggestions going inside it;
 * a file ends in a space. A path with spaces is quoted, as Claude Code does.
 */
export function completeAt(text: string, at: AtQuery, hint: FileHint): { text: string; caret: number } {
  const quoted = /\s/.test(hint.path);
  const folder = hint.dir && !quoted;
  const word = folder ? `@${hint.path.replace(/\/+$/, "")}/` : quoted ? `@"${hint.path}"` : `@${hint.path}`;
  const head = text.slice(0, at.start) + word;
  const tail = text.slice(at.end);
  if (folder) return { text: head + tail, caret: head.length };
  return { text: head + (tail.startsWith(" ") ? "" : " ") + tail, caret: head.length + 1 };
}
