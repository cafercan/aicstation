import type { FileHint } from "../api";

/**
 * The `@path` being typed at the caret: where its `@` is, where the word
 * ends, what follows the `@` up to the caret, and whether it is quoted
 * (`@"a path with spaces"`).
 */
export interface AtQuery { start: number; end: number; query: string; quoted: boolean }

const space = (c: string | undefined) => c !== undefined && /\s/.test(c);

/**
 * The `@` word the caret is in or right after, or `null`. Plain: an `@` at
 * the start or after white space, with no white space up to the caret.
 * Quoted: `@"` on the caret's line with no `"` between it and the caret;
 * the word ends at its closing quote (included). `end` may lie past the
 * caret when the caret is inside the word.
 */
export function atQuery(text: string, caret: number): AtQuery | null {
  const lineStart = text.lastIndexOf("\n", caret - 1) + 1;
  const q = caret >= 2 ? text.lastIndexOf('@"', caret - 2) : -1;
  if (q >= lineStart && (q === 0 || space(text[q - 1])) && !text.slice(q + 2, caret).includes('"')) {
    let end = caret;
    while (end < text.length && text[end] !== '"' && text[end] !== "\n") end++;
    if (text[end] === '"') end++;
    return { start: q, end, query: text.slice(q + 2, caret), quoted: true };
  }
  let start = caret;
  while (start > 0 && !space(text[start - 1])) start--;
  if (text[start] !== "@" || start >= caret) return null;
  let end = caret;
  while (end < text.length && !space(text[end])) end++;
  return { start, end, query: text.slice(start + 1, caret), quoted: false };
}

/**
 * The text with the `@` word replaced by the picked entry, and the caret
 * after it. A path with spaces (or picked in a quoted word) is quoted, as
 * Claude Code does. A folder ends in `/` with its quote still open, so the
 * suggestions go on inside it; a file closes the quote and ends in a space.
 */
export function completeAt(text: string, at: AtQuery, hint: FileHint): { text: string; caret: number } {
  const quoted = at.quoted || /\s/.test(hint.path);
  const path = hint.dir ? `${hint.path.replace(/\/+$/, "")}/` : hint.path;
  const head = text.slice(0, at.start) + (quoted ? `@"${path}` : `@${path}`);
  const tail = text.slice(at.end);
  if (hint.dir) return { text: head + tail, caret: head.length };
  const word = quoted ? `${head}"` : head;
  return { text: word + (tail.startsWith(" ") ? "" : " ") + tail, caret: word.length + 1 };
}
