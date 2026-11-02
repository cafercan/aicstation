/**
 * Pure cost aggregation for the Geçmiş "Maliyet" charts. Input is the history
 * rows' `costByDay` (local dates, YYYY-MM-DD, from the indexer) plus the
 * backend's today total, which also covers live sessions. Every figure is an
 * estimate (list prices over recorded token usage).
 */
import type { HistoryRow } from "./api";
import { folderName } from "./format";
import type { Token } from "./theme";

/** Theme tokens the charts draw with (theme-aware via CSS variables). */
export const CHART_TOKENS = {
  bar: "accent",
  today: "yellow",
  /** Model split series, in order; the last one also colors "other". */
  series: ["accent", "ctxB", "yellow", "busy", "danger", "muted"],
} as const satisfies { bar: Token; today: Token; series: readonly Token[] };

export const RANGES = [7, 30, 90] as const;
export type RangeDays = (typeof RANGES)[number];

const pad = (n: number) => String(n).padStart(2, "0");

/** Local calendar date of a timestamp as YYYY-MM-DD. */
export function localDateKey(ms: number): string {
  const d = new Date(ms);
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** The last `days` local dates ending today, oldest first (DST-safe: steps calendar days). */
export function rangeDays(nowMs: number, days: number): string[] {
  const now = new Date(nowMs);
  const out: string[] = [];
  for (let i = days - 1; i >= 0; i--) {
    const d = new Date(now.getFullYear(), now.getMonth(), now.getDate() - i, 12);
    out.push(localDateKey(d.getTime()));
  }
  return out;
}

export interface DayBucket { date: string; cost: number; today: boolean }

/**
 * Cost per day of `dates`. Today's bucket is `todayCost` when given (closed +
 * live sessions, from the backend), else the indexed rows' today slice.
 */
export function dailyCosts(rows: HistoryRow[], dates: string[], nowMs: number, todayCost?: number | null): DayBucket[] {
  const today = localDateKey(nowMs);
  const by = new Map(dates.map((d) => [d, 0]));
  for (const r of rows) {
    for (const [d, c] of Object.entries(r.summary?.costByDay ?? {})) {
      const cur = by.get(d);
      if (cur !== undefined && Number.isFinite(c)) by.set(d, cur + c);
    }
  }
  return dates.map((date) => ({
    date,
    cost: date === today && todayCost != null && Number.isFinite(todayCost) ? todayCost : by.get(date) ?? 0,
    today: date === today,
  }));
}

export function totals(buckets: DayBucket[]): { total: number; avg: number } {
  const total = buckets.reduce((a, b) => a + b.cost, 0);
  return { total, avg: buckets.length ? total / buckets.length : 0 };
}

/** A row's cost on the given days. */
function costIn(r: HistoryRow, days: Set<string>): number {
  let c = 0;
  for (const [d, v] of Object.entries(r.summary?.costByDay ?? {})) if (days.has(d) && Number.isFinite(v)) c += v;
  return c;
}

export interface ProjectCost { cwd: string; name: string; cost: number }

/** The `n` project folders that cost the most on `days` (rows without cost are ignored). */
export function topProjects(rows: HistoryRow[], days: Set<string>, n = 5): ProjectCost[] {
  const by = new Map<string, number>();
  for (const r of rows) {
    const c = costIn(r, days);
    if (c <= 0) continue;
    const cwd = r.summary?.cwd ?? r.cwd ?? "?";
    by.set(cwd, (by.get(cwd) ?? 0) + c);
  }
  return [...by.entries()]
    .map(([cwd, cost]) => ({ cwd, name: folderName(cwd) || cwd, cost }))
    .sort((a, b) => b.cost - a.cost || a.cwd.localeCompare(b.cwd))
    .slice(0, n);
}

export interface ModelCost {
  /** null = unknown model (or the "other" bucket when `other`). */
  model: string | null;
  cost: number;
  share: number;
  other?: boolean;
}

/**
 * Share of cost per model on `days`. A session's cost is attributed to its
 * (last) model, so the split is approximate. Beyond `max` models the rest is
 * one "other" bucket.
 */
export function modelSplit(rows: HistoryRow[], days: Set<string>, max = 5): ModelCost[] {
  const by = new Map<string | null, number>();
  for (const r of rows) {
    const c = costIn(r, days);
    if (c <= 0) continue;
    const m = r.summary?.model ?? null;
    by.set(m, (by.get(m) ?? 0) + c);
  }
  const total = [...by.values()].reduce((a, b) => a + b, 0);
  if (total <= 0) return [];
  const list: ModelCost[] = [...by.entries()]
    .map(([model, cost]) => ({ model, cost, share: cost / total }))
    .sort((a, b) => b.cost - a.cost || String(a.model).localeCompare(String(b.model)));
  if (list.length <= max) return list;
  const rest = list.slice(max).reduce((a, b) => a + b.cost, 0);
  return [...list.slice(0, max), { model: null, cost: rest, share: rest / total, other: true }];
}
