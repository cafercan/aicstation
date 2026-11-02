import { focusSessionTab, type SessionCard } from "./api";
import { errorText, t } from "./i18n";

/** Where "Go to tab" leads for a session, or null when the button is hidden. */
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
 * focus the Windows Terminal tab. Resolves to a note code (see errorText) or
 * null; rejects with a message in the UI language.
 */
export async function goToTab(
  sessionId: string, target: GotoTarget, onTerminal?: (id: number) => void,
): Promise<string | null> {
  if (!target) return null;
  if (target.kind === "embedded") { onTerminal?.(target.terminal); return null; }
  try {
    return await focusSessionTab(sessionId);
  } catch (e) {
    throw new Error(t("goto.failed", { err: errorText(e) }));
  }
}
