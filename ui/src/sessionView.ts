import type { SessionCard } from "./api";
import type { SessionTab, View } from "./components/Sidebar";
import { sessionsIn, type TerminalEntry } from "./terminal/store";

export type { SessionTab } from "./components/Sidebar";

/**
 * Where a session click leads (card, History row, tray/toast): a live session
 * inside an app terminal opens that terminal (its Terminal tab), anything
 * else (outside the app, closed, not in the snapshot) the Info tab.
 */
export function openSessionView(
  sessionId: string, sessions: SessionCard[], terminalFor: (parentPid: number | null | undefined) => number | undefined,
): View {
  const card = sessions.find((s) => s.sessionId === sessionId);
  const terminal = card && card.pid ? terminalFor(card.parentPid) : undefined;
  return terminal !== undefined ? { terminal } : { session: sessionId, tab: "info" };
}

/**
 * Follow-the-terminal rules (spec, "Decisions from review"). A session view
 * that holds a terminal follows whatever runs in that terminal: a new session
 * id (`/clear`, `/resume`) keeps the tab, no session left makes it the plain
 * terminal, a closed terminal falls back to the Info tab. Without a
 * terminal, once the session runs in an app terminal the Conversation and
 * Info tabs attach it and the Terminal tab switches to it; a Terminal tab also switches to a
 * terminal that Resume/Move started for this session before the registry
 * lists it (`pending`: terminal id -> session id; the newest wins).
 * Returns `v` itself when nothing changes.
 */
export function resolveView(
  v: View, terminals: TerminalEntry[], sessions: SessionCard[], pending: ReadonlyMap<number, string>,
): View {
  if (typeof v !== "object" || !("session" in v)) return v;
  if (v.terminal !== undefined) {
    const entry = terminals.find((t) => t.id === v.terminal);
    if (!entry) return { session: v.session, tab: "info" };
    const inside = sessionsIn(entry, sessions)[0];
    if (!inside) return { terminal: entry.id };
    return inside.sessionId === v.session ? v : { session: inside.sessionId, tab: v.tab, terminal: entry.id };
  }
  const card = sessions.find((s) => s.sessionId === v.session);
  const host = card && card.pid
    ? terminals.find((t) => sessionsIn(t, [card]).length > 0)
    : undefined;
  if (host) return v.tab === "terminal" ? { terminal: host.id } : { session: v.session, tab: v.tab, terminal: host.id };
  if (v.tab !== "terminal") return v;
  const waiting = terminals.filter((t) => pending.get(t.id) === v.session).map((t) => t.id);
  return waiting.length > 0 ? { terminal: Math.max(...waiting) } : v;
}

/**
 * View after app terminal `id` is closed: a plain view of it moves to `other`
 * (the remaining split pane) or the dashboard. Session views are left alone;
 * resolveView moves them to the Info tab once the terminal is gone.
 */
export function viewAfterClose(v: View, id: number, other: number | null): View {
  // Same test as workspace.tsx termOf, kept here so this module stays free of UI imports.
  if (typeof v !== "object" || "session" in v || v.terminal !== id) return v;
  return other !== null ? { terminal: other } : "dashboard";
}

/**
 * The `Terminal | Conversation | Info` tab bar for a view, or null when it has none
 * (non-session pages, a plain shell terminal, a terminal that is gone).
 */
export function tabBarFor(
  v: View, terminals: TerminalEntry[], sessions: SessionCard[],
): { sessionId: string; terminal?: number; active: SessionTab } | null {
  if (typeof v !== "object") return null;
  if ("session" in v) {
    return v.terminal !== undefined
      ? { sessionId: v.session, terminal: v.terminal, active: v.tab }
      : { sessionId: v.session, active: v.tab };
  }
  const entry = terminals.find((t) => t.id === v.terminal);
  const inside = entry ? sessionsIn(entry, sessions)[0] : undefined;
  return inside ? { sessionId: inside.sessionId, terminal: v.terminal, active: "terminal" } : null;
}

export type MoveReason = "busy" | "waiting" | "agents" | "notMovable";

/**
 * Whether "Move to app" is enabled for an outside session, and why not.
 * Checked in order: not movable, waiting / busy (any status but idle), running agents.
 */
export function moveState(card: SessionCard): { enabled: boolean; reason: MoveReason | null } {
  const reason: MoveReason | null = !card.movable ? "notMovable"
    : card.status === "waiting" ? "waiting"
    : card.status !== "idle" ? "busy"
    : card.agentsRunning > 0 ? "agents"
    : null;
  return { enabled: reason === null, reason };
}
