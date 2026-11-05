/**
 * Data of the per-terminal Claude bar: one shared poll of the status line
 * data (only while a bar is on screen) and the pure formatting of its parts.
 */
import { useEffect, useState, useSyncExternalStore } from "react";
import { claudeStatus, gitDirty, type ClaudeStatus, type UsageWindow } from "../api";
import { getLang, t, type Lang } from "../i18n";
import { onTestReset } from "../prefs";

/** How often the status line data is read while a bar is shown. */
export const STATUS_POLL_MS = 5_000;
/** How often a bar asks whether its folder has uncommitted changes. */
export const DIRTY_POLL_MS = 10_000;
/** Cells of a usage meter. */
export const METER_CELLS = 10;

let status: ClaudeStatus | null = null;
const listeners = new Set<() => void>();
let timer: ReturnType<typeof setInterval> | null = null;

function emit(): void { for (const l of listeners) l(); }

/** Reads the status line data now (the Settings toggle calls it after a change). */
export async function refreshClaudeStatus(): Promise<void> {
  try { status = await claudeStatus(); } catch { status = null; }
  emit();
}

function subscribe(l: () => void): () => void {
  listeners.add(l);
  if (listeners.size === 1) {
    void refreshClaudeStatus();
    timer = setInterval(() => { void refreshClaudeStatus(); }, STATUS_POLL_MS);
  }
  return () => {
    listeners.delete(l);
    if (listeners.size === 0 && timer !== null) { clearInterval(timer); timer = null; }
  };
}

export function useClaudeStatus(): ClaudeStatus | null {
  return useSyncExternalStore(subscribe, () => status);
}

onTestReset(() => {
  status = null;
  listeners.clear();
  if (timer !== null) { clearInterval(timer); timer = null; }
});

/** Uncommitted changes in `cwd` (null: unknown), asked every {@link DIRTY_POLL_MS}. */
export function useGitDirty(cwd: string): boolean | null {
  const [dirty, setDirty] = useState<boolean | null>(null);
  useEffect(() => {
    let live = true;
    const ask = () => { gitDirty(cwd).then((d) => { if (live) setDirty(d); }, () => { if (live) setDirty(null); }); };
    setDirty(null);
    ask();
    const id = setInterval(ask, DIRTY_POLL_MS);
    return () => { live = false; clearInterval(id); };
  }, [cwd]);
  return dirty;
}

/** "claude-opus-5-5" -> "opus-5-5". */
export function shortModel(model: string | null): string | null {
  return model ? model.replace(/^claude-/, "") : null;
}

/** Compact time until a reset: "3d", "2h10", "45m". */
export function fmtReset(ms: number, lang: Lang = getLang()): string {
  const min = Math.max(1, Math.ceil(ms / 60_000));
  if (min < 60) return t("cbar.minutes", { n: min }, lang);
  const h = Math.floor(min / 60);
  if (h < 24) return min % 60 ? t("cbar.hoursMinutes", { h, m: String(min % 60).padStart(2, "0") }, lang) : t("cbar.hours", { n: h }, lang);
  return t("cbar.days", { n: Math.round(h / 24) }, lang);
}

export interface LimitView {
  /** Percent left, 0–100, rounded. */
  left: number;
  /** Filled cells of a {@link METER_CELLS}-cell meter. */
  filled: number;
  /** Time until the reset, when known. */
  resetIn: number | null;
  level: "ok" | "low" | "critical";
}

/**
 * A usage window as shown; null when unknown, or when its reset time has
 * passed (the numbers are from the window that ended).
 */
export function limitView(w: UsageWindow | null | undefined, now: number): LimitView | null {
  if (!w || !Number.isFinite(w.usedPct)) return null;
  if (w.resetsAtMs !== null && w.resetsAtMs <= now) return null;
  const left = Math.round(Math.min(100, Math.max(0, 100 - w.usedPct)));
  return {
    left,
    filled: Math.round((left / 100) * METER_CELLS),
    resetIn: w.resetsAtMs === null ? null : w.resetsAtMs - now,
    level: left < 10 ? "critical" : left < 30 ? "low" : "ok",
  };
}

export function meter(filled: number): string {
  return "═".repeat(filled) + "─".repeat(METER_CELLS - filled);
}
