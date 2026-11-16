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
 * terminal, a closed terminal falls back to the Info tab. A Terminal tab
 * without a terminal switches to the app terminal once the session runs in
 * one, or to a terminal started by Resume/Move whose session is not listed
 * yet (`pending`; the newest wins). Returns `v` itself when nothing changes.
 */
export function resolveView(
  v: View, terminals: TerminalEntry[], sessions: SessionCard[], pending: ReadonlySet<number>,
): View {
  if (typeof v !== "object" || !("session" in v)) return v;
  if (v.terminal !== undefined) {
    const entry = terminals.find((t) => t.id === v.terminal);
    if (!entry) return { session: v.session, tab: "info" };
    const inside = sessionsIn(entry, sessions)[0];
    if (!inside) return { terminal: entry.id };
    return inside.sessionId === v.session ? v : { session: inside.sessionId, tab: v.tab, terminal: entry.id };
  }
  if (v.tab !== "terminal") return v;
  const card = sessions.find((s) => s.sessionId === v.session);
  const host = card && card.pid
    ? terminals.find((t) => sessionsIn(t, [card]).length > 0)
    : undefined;
  if (host) return { terminal: host.id };
  const waiting = terminals.filter((t) => pending.has(t.id)).map((t) => t.id);
  return waiting.length > 0 ? { terminal: Math.max(...waiting) } : v;
}

/**
 * The `Terminal | Info` tab bar for a view, or null when it has none
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
