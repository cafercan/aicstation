/**
 * Pure text / status / date filters for the Dashboard cards and the Geçmiş
 * table. Matching is case- and accent-insensitive; Turkish İ, I and ı all
 * fold to "i" so "istanbul", "İSTANBUL" and "ıstanbul" find each other.
 */
import type { HistoryRow, SessionCard } from "./api";
import { statusKind, usageSum, type StatusKind } from "./format";
import { localDateKey } from "./costs";

const SPECIAL: Record<string, string> = { ı: "i", İ: "i", I: "i", ə: "e", Ə: "e", ø: "o", Ø: "o", ł: "l", Ł: "l", đ: "d", Đ: "d" };

/** Search form of a string: lowercase, accents stripped, Turkish i variants unified. */
export function fold(s: string): string {
  return s
    .replace(/[ıİIəƏøØłŁđĐ]/g, (c) => SPECIAL[c])
    .normalize("NFD")
    .replace(/\p{M}/gu, "")
    .toLowerCase();
}

/** Every whitespace-separated word of `query` occurs in at least one field. */
export function matchesQuery(query: string, fields: (string | null | undefined)[]): boolean {
  const words = fold(query).split(/\s+/).filter((w) => w !== "");
  if (words.length === 0) return true;
  const hay = fields.filter((f): f is string => !!f).map(fold);
  return words.every((w) => hay.some((h) => h.includes(w)));
}

/** Status chips on the Dashboard, in display order. */
export const STATUS_CHIPS = ["busy", "waiting", "idle", "shell"] as const satisfies readonly StatusKind[];
export type StatusChip = (typeof STATUS_CHIPS)[number];

/** Cards matching the text (name / folder / model) and, when any chip is on, one of the chips. */
export function filterSessions(cards: SessionCard[], query: string, chips: ReadonlySet<string>): SessionCard[] {
  return cards.filter((c) =>
    (chips.size === 0 || chips.has(statusKind(c.status, c.pid))) && matchesQuery(query, [c.name, c.cwd, c.model]));
}

export type HistorySort = "date" | "tokens";

export interface HistoryFilter {
  query: string;
  /** Inclusive local dates (YYYY-MM-DD); "" = open end. */
  from: string;
  to: string;
  sort: HistorySort;
}

export const rowCwd = (r: HistoryRow) => r.summary?.cwd ?? r.cwd ?? "";

/** Rows matching the filter, sorted (date: newest first; tokens: most first, unindexed last). */
export function filterHistory(rows: HistoryRow[], f: HistoryFilter): HistoryRow[] {
  const out = rows.filter((r) => {
    const day = localDateKey(r.modifiedMs);
    if (f.from && day < f.from) return false;
    if (f.to && day > f.to) return false;
    return matchesQuery(f.query, [r.summary?.name ?? r.name, rowCwd(r), r.summary?.model]);
  });
  const tokens = (r: HistoryRow) => (r.summary ? usageSum(r.summary.tokens) : -1);
  return out.sort((a, b) =>
    f.sort === "tokens" ? tokens(b) - tokens(a) || b.modifiedMs - a.modifiedMs : b.modifiedMs - a.modifiedMs);
}
