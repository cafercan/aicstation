import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export type Unlisten = () => void;

/** Rust ToolState serializes with its variant names. */
export type ToolState = "Running" | "Ok" | "Error";

export interface ToolCall {
  id: string;
  name: string;
  target: string;
  /** RFC3339 */
  at: string;
  state: ToolState;
}

/** "busy" | "waiting" | "idle" or any raw string the registry reports. */
export type SessionStatus = string;

export interface SessionCard {
  sessionId: string;
  pid: number;
  parentPid: number | null;
  name: string;
  cwd: string;
  status: SessionStatus;
  waitingFor: string | null;
  model: string | null;
  contextPct: number | null;
  costUsd: number;
  agentsTotal: number;
  agentsFinished: number;
  lastTool: ToolCall | null;
  startedAtMs: number;
  lastActivityMs: number;
  gitBranch: string | null;
}

export interface Snapshot {
  sessions: SessionCard[];
  runningAgents: number;
  waitingCount: number;
}

export interface HistorySummary {
  name: string;
  cwd: string | null;
  model: string | null;
  firstAtMs: number;
  lastAtMs: number;
  agentsTotal: number;
  costUsd: number;
  costByDay: Record<string, number>;
}

export interface HistoryRow {
  sessionId: string;
  path: string;
  modifiedMs: number;
  size: number;
  name: string;
  cwd: string | null;
  summary: HistorySummary | null;
}

export interface HistoryView {
  rows: HistoryRow[];
  todayCost: number;
  todayComplete: boolean;
  indexing: boolean;
}

export interface Usage {
  input: number;
  output: number;
  cacheRead: number;
  cacheCreation: number;
  cacheCreation1h: number;
}

/** Tree shape is filled in by the detail page (Task 12). */
export interface SessionDetail {
  card: SessionCard;
  tokens: Usage;
  toolCallCount: number;
  tree: { roots: unknown[]; total: number; finished: number };
  mainToolCalls: ToolCall[];
}

export const getSnapshot = () => invoke<Snapshot>("get_snapshot");
export const getHistory = () => invoke<HistoryView>("get_history");
export const getSessionDetail = (sessionId: string) =>
  invoke<SessionDetail | null>("get_session_detail", { sessionId });
export const openFolder = (path: string) => invoke<void>("open_folder", { path });

export async function onSnapshot(cb: (s: Snapshot) => void): Promise<Unlisten> {
  return listen<Snapshot>("snapshot", (e) => cb(e.payload));
}
