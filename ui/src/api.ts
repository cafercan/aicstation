import { invoke, type Channel } from "@tauri-apps/api/core";
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
  /** Ok / Error results among the main session's last <= 50 tool calls (running ones excluded). */
  toolOk: number;
  toolErr: number;
  lastTool: ToolCall | null;
  startedAtMs: number;
  lastActivityMs: number;
  gitBranch: string | null;
  /** Where the Claude process runs; "other" for closed sessions. */
  hostKind: HostKind;
}

export type HostKind = "embedded" | "windowsTerminal" | "other";

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

/** Rust AgentKind: internally tagged by "kind". */
export type AgentKind =
  | { kind: "agent" }
  | { kind: "workflow"; id: string }
  | { kind: "phase"; name: string };

export interface AgentStats {
  tokens: Usage;
  costUsd: number;
  toolCalls: ToolCall[];
  toolCallCount: number;
  firstAt: string | null;
  lastAt: string | null;
  model?: string | null;
}

export interface AgentNode {
  id: string;
  kind: AgentKind;
  agentType: string | null;
  description: string | null;
  state: "running" | "finished";
  stats: AgentStats;
  children: AgentNode[];
}

export interface AgentTree {
  roots: AgentNode[];
  total: number;
  finished: number;
}

export interface SessionDetail {
  card: SessionCard;
  tokens: Usage;
  toolCallCount: number;
  tree: AgentTree;
  mainToolCalls: ToolCall[];
}

export const getSnapshot = () => invoke<Snapshot>("get_snapshot");
export const getHistory = () => invoke<HistoryView>("get_history");
export const getSessionDetail = (sessionId: string) =>
  invoke<SessionDetail | null>("get_session_detail", { sessionId });
export const openFolder = (path: string) => invoke<void>("open_folder", { path });
export interface PtyInfo { id: number; pid: number }

/** Only the session id is sent; the backend validates it and finds the folder. */
export const ptySpawnResume = (
  sessionId: string, cols: number, rows: number, onOutput: Channel<number[]>, onExit: Channel<number>,
) => invoke<PtyInfo>("pty_spawn_resume", { sessionId, cols, rows, onOutput, onExit });
/**
 * Brings the session's Windows Terminal tab forward. Only the id is sent;
 * resolves to a note when the window was focused but no tab was selected.
 */
export const focusSessionTab = (sessionId: string) => invoke<string | null>("focus_session_tab", { sessionId });
/** No arguments: the backend uses fixed wt.exe arguments. */
export const openWindowsTerminal = () => invoke<void>("open_windows_terminal");

export async function onSnapshot(cb: (s: Snapshot) => void): Promise<Unlisten> {
  return listen<Snapshot>("snapshot", (e) => cb(e.payload));
}

/** Snippet split around the match (the backend cuts it; at most 120 chars in all). */
export interface SearchSnippet { before: string; hit: string; after: string }

export interface SearchHit {
  sessionId: string;
  /** Message time (ms), 0 when unknown. */
  atMs: number;
  role: "user" | "assistant";
  snippet: SearchSnippet;
}

export interface SearchOutcome {
  hits: SearchHit[];
  /** The 50-hit limit was reached. */
  truncated: boolean;
  /** A newer search replaced this one; ignore it. */
  cancelled: boolean;
}

/**
 * "İçerikte ara": streams the transcripts for user/assistant text. A call
 * with a different `generation` stops a search still running.
 */
export const searchHistory = (query: string, generation: number) =>
  invoke<SearchOutcome>("search_history", { query, generation });
