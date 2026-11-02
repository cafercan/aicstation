export function fmtTokens(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(2)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
  return String(n);
}

export function fmtCost(usd: number): string {
  return `$${usd.toFixed(2)}`;
}

export function fmtElapsed(ms: number): string {
  const sec = Math.floor(Math.max(0, ms) / 1000);
  if (sec < 60) return `${sec} sn`;
  const min = Math.floor(sec / 60);
  if (min < 60) return `${min} dk`;
  const h = Math.floor(min / 60);
  if (h < 24) return min % 60 ? `${h} sa ${min % 60} dk` : `${h} sa`;
  return `${Math.floor(h / 24)} gün`;
}

export type StatusKind = "busy" | "waiting" | "idle" | "closed" | "unknown";

/** A session whose process is gone (pid 0) is "closed" whatever its last status was. */
export function statusKind(status: string, pid?: number): StatusKind {
  if (pid === 0) return "closed";
  return status === "busy" || status === "waiting" || status === "idle" ? status : "unknown";
}

export function statusLabel(status: string, pid?: number): string {
  switch (statusKind(status, pid)) {
    case "busy": return "çalışıyor";
    case "waiting": return "onay bekliyor";
    case "idle": return "hazır";
    case "closed": return "kapandı";
    default: return status || "?";
  }
}

export const STATUS_COLORS: Record<StatusKind, string> = {
  busy: "#48c774",
  idle: "#ff9f43",
  waiting: "#ff5d73",
  closed: "#7d8696",
  unknown: "#7d8696",
};

export function statusColor(kind: StatusKind): string {
  return STATUS_COLORS[kind];
}

export const AGENT_RUNNING_COLOR = "#5aa0ff";
export const AGENT_FINISHED_COLOR = "#48c774";

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
  if (fl > 0) out.push({ kind: "finished", color: AGENT_FINISHED_COLOR, length: fl, offset: 25 });
  if (rl > 0) out.push({ kind: "running", color: AGENT_RUNNING_COLOR, length: rl, offset: 25 - fl });
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
