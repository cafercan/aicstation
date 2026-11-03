/**
 * Pure usage aggregation for the Geçmiş "Kullanım" charts. Input is the history
 * rows' `tokensByDay` / `costByDay` (local dates, YYYY-MM-DD, from the indexer)
 * plus the backend's today totals, which also cover live sessions. Tokens are
 * exact (read from the transcripts); the dollar figures are estimates (built-in
 * list prices over the recorded token usage) and are secondary everywhere.
 */
import type { HistoryRow, LiveCost } from "./api";
import { folderName } from "./format";
import type { Token } from "./theme";

/** Theme tokens the charts draw with (theme-aware via CSS variables). */
export const CHART_TOKENS = {
  bar: "accent",
  today: "yellow",
  /** Model split series, in order; the last one also colors "other". */
  series: ["accent", "ctxB", "yellow", "busy", "danger", "muted"],
} as const satisfies { bar: Token; today: Token; series: readonly Token[] };

const sumValues = (o: Record<string, number>) =>
  Object.values(o).reduce((a, b) => a + (Number.isFinite(b) ? b : 0), 0);

/**
 * History rows plus the live sessions as rows of their own, so every chart
 * (daily bars, top projects, model split) counts live sessions on every day
 * the same way. The today bar's backend total already includes them.
 */
export function withLive(rows: HistoryRow[], live: LiveCost[] | undefined): HistoryRow[] {
  if (!live?.length) return rows;
  const known = new Set(rows.map((r) => r.sessionId));
  const extra = live.filter((l) => !known.has(l.sessionId)).map((l): HistoryRow => {
    const tokensByDay = l.tokensByDay ?? {};
    return {
      sessionId: l.sessionId, modifiedMs: 0, size: 0, name: l.sessionId, cwd: l.cwd,
      summary: {
        name: l.sessionId, cwd: l.cwd, model: l.model, firstAtMs: 0, lastAtMs: 0, agentsTotal: 0,
        costUsd: sumValues(l.costByDay),
        costByDay: l.costByDay,
        tokens: { input: 0, output: 0, cacheRead: 0, cacheCreation: 0, cacheCreation1h: 0 },
        tokensByDay,
      },
    };
  });
  return [...rows, ...extra];
}

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

export interface DayBucket { date: string; tokens: number; cost: number; today: boolean }

/** Today's totals from the backend (closed + live sessions); either may be missing. */
export interface TodayTotals { tokens?: number | null; cost?: number | null }

const finite = (n: number | null | undefined): n is number => n != null && Number.isFinite(n);

/**
 * Tokens and estimated cost per day of `dates`. Today's bucket takes the
 * backend totals when given (closed + live sessions), else the indexed rows'
 * today slice.
 */
export function dailyUsage(rows: HistoryRow[], dates: string[], nowMs: number, todayTotals?: TodayTotals): DayBucket[] {
  const today = localDateKey(nowMs);
  const cost = new Map(dates.map((d) => [d, 0]));
  const tokens = new Map(dates.map((d) => [d, 0]));
  for (const r of rows) {
    for (const [d, c] of Object.entries(r.summary?.costByDay ?? {})) {
      const cur = cost.get(d);
      if (cur !== undefined && Number.isFinite(c)) cost.set(d, cur + c);
    }
    for (const [d, k] of Object.entries(r.summary?.tokensByDay ?? {})) {
      const cur = tokens.get(d);
      if (cur !== undefined && Number.isFinite(k)) tokens.set(d, cur + k);
    }
  }
  return dates.map((date) => {
    const isToday = date === today;
    return {
      date,
      tokens: isToday && finite(todayTotals?.tokens) ? todayTotals.tokens : tokens.get(date) ?? 0,
      cost: isToday && finite(todayTotals?.cost) ? todayTotals.cost : cost.get(date) ?? 0,
      today: isToday,
    };
  });
}

export interface RangeTotals { tokens: number; cost: number; avgTokens: number; avgCost: number }

export function totals(buckets: DayBucket[]): RangeTotals {
  const tokens = buckets.reduce((a, b) => a + b.tokens, 0);
  const cost = buckets.reduce((a, b) => a + b.cost, 0);
  const n = buckets.length;
  return { tokens, cost, avgTokens: n ? tokens / n : 0, avgCost: n ? cost / n : 0 };
}

/** A row's tokens and estimated cost on the given days. */
function usageIn(r: HistoryRow, days: Set<string>): { tokens: number; cost: number } {
  let tokens = 0;
  let cost = 0;
  for (const [d, v] of Object.entries(r.summary?.tokensByDay ?? {})) if (days.has(d) && Number.isFinite(v)) tokens += v;
  for (const [d, v] of Object.entries(r.summary?.costByDay ?? {})) if (days.has(d) && Number.isFinite(v)) cost += v;
  return { tokens, cost };
}

export interface ProjectUsage { cwd: string; name: string; tokens: number; cost: number }

/** The `n` project folders with the most tokens on `days` (rows without usage are ignored). */
export function topProjects(rows: HistoryRow[], days: Set<string>, n = 5): ProjectUsage[] {
  const by = new Map<string, { tokens: number; cost: number }>();
  for (const r of rows) {
    const u = usageIn(r, days);
    if (u.tokens <= 0) continue;
    const cwd = r.summary?.cwd ?? r.cwd ?? "?";
    const cur = by.get(cwd) ?? { tokens: 0, cost: 0 };
    by.set(cwd, { tokens: cur.tokens + u.tokens, cost: cur.cost + u.cost });
  }
  return [...by.entries()]
    .map(([cwd, u]) => ({ cwd, name: folderName(cwd) || cwd, ...u }))
    .sort((a, b) => b.tokens - a.tokens || a.cwd.localeCompare(b.cwd))
    .slice(0, n);
}

export interface ModelUsage {
  /** null = unknown model (or the "other" bucket when `other`). */
  model: string | null;
  tokens: number;
  cost: number;
  share: number;
  other?: boolean;
}

/**
 * Share of tokens per model on `days`. A session's usage is attributed to its
 * (last) model, so the split is approximate. Beyond `max` models the rest is
 * one "other" bucket.
 */
export function modelSplit(rows: HistoryRow[], days: Set<string>, max = 5): ModelUsage[] {
  const by = new Map<string | null, { tokens: number; cost: number }>();
  for (const r of rows) {
    const u = usageIn(r, days);
    if (u.tokens <= 0) continue;
    const m = r.summary?.model ?? null;
    const cur = by.get(m) ?? { tokens: 0, cost: 0 };
    by.set(m, { tokens: cur.tokens + u.tokens, cost: cur.cost + u.cost });
  }
  const total = [...by.values()].reduce((a, b) => a + b.tokens, 0);
  if (total <= 0) return [];
  const list: ModelUsage[] = [...by.entries()]
    .map(([model, u]) => ({ model, ...u, share: u.tokens / total }))
    .sort((a, b) => b.tokens - a.tokens || String(a.model).localeCompare(String(b.model)));
  if (list.length <= max) return list;
  const rest = list.slice(max);
  const tokens = rest.reduce((a, b) => a + b.tokens, 0);
  const cost = rest.reduce((a, b) => a + b.cost, 0);
  return [...list.slice(0, max), { model: null, tokens, cost, share: tokens / total, other: true }];
}
