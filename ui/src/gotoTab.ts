import { focusSessionTab, type SessionCard } from "./api";

/** Where "Sekmeye git" leads for a session, or null when the button is hidden. */
export type GotoTarget = { kind: "embedded"; terminal: number } | { kind: "windowsTerminal" } | null;

/**
 * `terminal` is the embedded terminal hosting the session (terminalForPid).
 * Closed sessions and sessions in other hosts (VS Code, ...) have no target.
 */
export function gotoTarget(card: Pick<SessionCard, "pid" | "hostKind">, terminal: number | undefined): GotoTarget {
  if (!card.pid) return null;
  if (terminal !== undefined) return { kind: "embedded", terminal };
  if (card.hostKind === "windowsTerminal") return { kind: "windowsTerminal" };
  return null;
}

/**
 * Switches to the embedded terminal or asks the backend (session id only) to
 * focus the Windows Terminal tab. Resolves to a note to show, or null;
 * rejects with a Turkish message.
 */
export async function goToTab(
  sessionId: string, target: GotoTarget, onTerminal?: (id: number) => void,
): Promise<string | null> {
  if (!target) return null;
  if (target.kind === "embedded") { onTerminal?.(target.terminal); return null; }
  try {
    return await focusSessionTab(sessionId);
  } catch (e) {
    throw new Error(`Sekmeye gidilemedi: ${String(e)}`);
  }
}
