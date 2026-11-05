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

/** A file changed by successful editing tool calls (Edit, Write, ...). */
export interface ChangedFile {
  /** Full path, as the tool call named it. */
  path: string;
  edits: number;
  /** RFC3339 */
  lastAt: string;
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
  /** Main session plus its agents: exact (cost is an estimate). */
  tokens: Usage;
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
  /** Exact token totals of the session and its agents. */
  tokens: Usage;
  tokensByDay: Record<string, number>;
}

export interface HistoryRow {
  sessionId: string;
  modifiedMs: number;
  size: number;
  name: string;
  cwd: string | null;
  summary: HistorySummary | null;
}

/** A live session's cost and tokens per local day (live sessions are not in `rows`). */
export interface LiveCost {
  sessionId: string;
  cwd: string;
  model: string | null;
  costByDay: Record<string, number>;
  /** Absent in old fixtures. */
  tokensByDay?: Record<string, number>;
}

export interface HistoryView {
  rows: HistoryRow[];
  todayCost: number;
  /** Today's tokens (closed + live sessions), like `todayCost`. */
  todayTokens: number;
  todayComplete: boolean;
  indexing: boolean;
  /** Live sessions' cost per day, for the cost charts (absent in old fixtures). */
  live?: LiveCost[];
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
  /** Files changed by the session and its agents, most recent first (absent in old fixtures). */
  changedFiles?: ChangedFile[];
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
/** Shell ids installed on this machine ("auto" first). */
export const listShells = () => invoke<string[]>("list_shells");
export type ButaglyphPreset = { name: string; description: string };
export type ButaglyphInfo = { installed: boolean; version: string | null; presets: ButaglyphPreset[] };
/** Whether butaglyph is installed, its version and presets. */
export const butaglyphInfo = () => invoke<ButaglyphInfo>("butaglyph_info");
/** Two sample prompts (ANSI) drawn by buta with `preset` ("" = own config). */
export const butaglyphPreview = (preset: string) => invoke<string[]>("butaglyph_preview", { preset });
/** butaglyph's own config editor in the browser; rejects with an error code. */
export const openButaglyphEditor = () => invoke<void>("open_butaglyph_editor");
export const openSystemTerminal = () => invoke<void>("open_system_terminal");

export async function onSnapshot(cb: (s: Snapshot) => void): Promise<Unlisten> {
  return listen<Snapshot>("snapshot", (e) => cb(e.payload));
}

/** settings.json in the app data dir (Rust `Settings`). */
export interface Settings {
  notifyWaiting: boolean;
  notifyFinished: boolean;
  notifySubagent: boolean;
  notifySound: boolean;
  closeToTray: boolean;
  /** Shell id of the embedded terminals ("auto" = platform default). */
  shell: string;
  /** "own" (the shell's rc files decide) or "butaglyph". */
  prompt: string;
  /** butaglyph preset name; "" = the user's own butaglyph config. */
  promptPreset: string;
  /** Owned by the backend; a value sent here is ignored. */
  trayNoticeShown: boolean;
  /** UI language, for the texts the backend shows (toasts, tray). */
  lang: string;
}

export const getSettings = () => invoke<Settings>("get_settings");
/** Resolves to what was stored; rejects with an error code. */
export const setSettings = (settings: Settings) => invoke<Settings>("set_settings", { settings });
/**
 * The sessions on screen: the detail view's session, or the Claude sessions
 * in the visible terminal panes (at most two; empty for any other view).
 */
export const setFocusContext = (sessionIds: string[]) => invoke<void>("set_focus_context", { sessionIds });
/** Hides the window to the tray (close with "minimize to tray" on). */
export const hideToTray = () => invoke<void>("hide_to_tray");

/** Tray menu / toast click: open a session's detail (or the dashboard when null). */
export async function onNavigate(cb: (sessionId: string | null) => void): Promise<Unlisten> {
  return listen<{ session: string | null }>("navigate", (e) => cb(e.payload?.session ?? null));
}

/** "Quit" chosen in the tray menu. */
export async function onTrayQuit(cb: () => void): Promise<Unlisten> {
  return listen("tray-quit", () => cb());
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
  /** A limit was reached (50 hits, 8 s or 2 GiB read); more matches may exist. */
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

/** Stops the running content search (page left or search cleared). */
export const cancelSearch = () => invoke<void>("cancel_search");
