import { getLang, t, type Lang } from "./i18n";
import { themeTokens } from "./theme";

export function fmtTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(n);
}

export function fmtCost(usd: number): string {
  return `$${usd.toFixed(2)}`;
}

export function fmtElapsed(ms: number, lang: Lang = getLang()): string {
  const sec = Math.floor(Math.max(0, ms) / 1000);
  if (sec < 60) return t("time.sec", { n: sec }, lang);
  const min = Math.floor(sec / 60);
  if (min < 60) return t("time.min", { n: min }, lang);
  const h = Math.floor(min / 60);
  if (h < 24) return min % 60 ? t("time.hourMin", { h, m: min % 60 }, lang) : t("time.hour", { n: h }, lang);
  return t("time.day", { n: Math.floor(h / 24) }, lang);
}

/** "shell": Claude Code is running a shell command (shown like busy). */
export type StatusKind = "busy" | "shell" | "waiting" | "idle" | "closed" | "unknown";

const KINDS: readonly StatusKind[] = ["busy", "shell", "waiting", "idle", "closed", "unknown"];
const isKind = (s: string): s is StatusKind => (KINDS as readonly string[]).includes(s);
const LIVE_KINDS: readonly string[] = ["busy", "shell", "waiting", "idle"];

/** A session whose process is gone (pid 0) is "closed" whatever its last status was. */
export function statusKind(status: string, pid?: number): StatusKind {
  if (pid === 0) return "closed";
  return LIVE_KINDS.includes(status) ? (status as StatusKind) : "unknown";
}

/**
 * Label of the status kind in the UI language. Only a raw status the app does
 * not know (e.g. "compacting") is shown as sent; a raw string that spells a
 * kind name ("closed", "unknown") or an empty one gets the kind's label.
 */
export function statusLabel(status: string, pid?: number, lang: Lang = getLang()): string {
  const kind = statusKind(status, pid);
  if (kind !== "unknown") return t(`status.${kind}`, undefined, lang);
  if (isKind(status)) return t(`status.${status}`, undefined, lang);
  return status || t("status.unknown", undefined, lang);
}

/** Marker color of the kind in the active theme (card stripe, mark, donut). */
export function statusColor(kind: StatusKind): string {
  const k = themeTokens();
  return kind === "busy" || kind === "shell" ? k.busy : kind === "unknown" ? k.closed : k[kind];
}

/** Text color of the kind in the active theme (status bar), readable on the theme's surfaces. */
export function statusTextColor(kind: StatusKind): string {
  const k = themeTokens();
  return kind === "busy" || kind === "shell" ? k.busyText
    : kind === "idle" ? k.idleText : kind === "waiting" ? k.waitingText : k.closedText;
}

/** Donut / agent-list colors in the active theme. */
export function agentColor(kind: "running" | "finished"): string {
  const k = themeTokens();
  return kind === "running" ? k.agentRunning : k.busy;
}

export interface DonutSegment {
  kind: "finished" | "running";
  color: string;
  /** Arc length on a circle whose circumference is 100. */
  length: number;
  /** stroke-dashoffset; 25 puts the start at 12 o'clock. */
  offset: number;
}

/** Finished arc first from 12 o'clock, running arc right after it, clockwise. */
export function donutSegments(running: number, finished: number): DonutSegment[] {
  const r = Math.max(0, running);
  const f = Math.max(0, finished);
  const total = r + f;
  if (total === 0) return [];
  const fl = (f / total) * 100;
  const rl = (r / total) * 100;
  const out: DonutSegment[] = [];
  if (fl > 0) out.push({ kind: "finished", color: agentColor("finished"), length: fl, offset: 25 });
  if (rl > 0) out.push({ kind: "running", color: agentColor("running"), length: rl, offset: 25 - fl });
  return out;
}

/** Ok / (Ok + Error) as "N%", or "—" when no call has finished. */
export function toolSuccess(ok: number, err: number): string {
  const n = ok + err;
  return n > 0 ? `${Math.round((ok / n) * 100)}%` : "—";
}

export function clampPct(pct: number | null): number {
  return pct == null ? 0 : Math.max(0, Math.min(100, pct));
}

/** Context percentage text; an unknown window stays "?" rather than a guessed number. */
export function fmtContext(pct: number | null): string {
  return pct == null ? "?" : `${Math.round(clampPct(pct))}%`;
}

/** Last component of a Windows or POSIX folder path ("" if none). */
export function folderName(cwd: string): string {
  return cwd.split(/[\\/]/).filter((s) => s !== "").pop() ?? "";
}
