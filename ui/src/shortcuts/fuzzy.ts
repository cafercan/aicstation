/**
 * Small fuzzy matcher for the command palette: the query's characters must
 * appear in order (case- and accent-insensitive, see filter.ts `fold`).
 * Consecutive characters and word starts score higher; gaps cost a little.
 */
import { fold } from "../filter";

/** Score of `query` in `text` (higher is better), 0 for an empty query, null when it does not match. */
export function fuzzyScore(query: string, text: string): number | null {
  const q = fold(query).replace(/\s+/g, "");
  if (q === "") return 0;
  const t = fold(text);
  let score = 0;
  let ti = 0;
  let prev = -2;
  for (const ch of q) {
    const at = t.indexOf(ch, ti);
    if (at < 0) return null;
    const wordStart = at === 0 || /[\s\-_/\\.:]/.test(t[at - 1]);
    score += 1;
    if (at === prev + 1) score += 3;
    if (wordStart) score += 2;
    if (at === 0) score += 2;
    score -= Math.min(at - ti, 5) * 0.1;
    prev = at;
    ti = at + 1;
  }
  return score;
}

/** Items that match, best first; ties keep their input order. */
export function fuzzyRank<T>(items: readonly T[], query: string, text: (item: T) => string): T[] {
  return items
    .map((item, i) => ({ item, i, s: fuzzyScore(query, text(item)) }))
    .filter((x): x is { item: T; i: number; s: number } => x.s !== null)
    .sort((a, b) => b.s - a.s || a.i - b.i)
    .map((x) => x.item);
}
