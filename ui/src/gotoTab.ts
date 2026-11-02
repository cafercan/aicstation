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

const RESUME_TERMINAL_OPEN = /^session_resume_terminal_open(?::(\d+))?$/;

/**
 * Text for a rejected resume, plus the app terminal to offer "Go to tab" for
 * when the session is still running in the terminal that resumed it
 * (`session_resume_terminal_open:<terminal id>`; no id while that spawn is
 * still in flight).
 */
export function resumeError(e: unknown): { text: string; terminal: number | null } {
  const s = typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
  const m = RESUME_TERMINAL_OPEN.exec(s);
  if (!m) return { text: errorText(e), terminal: null };
  return { text: t("err.session_resume_terminal_open"), terminal: m[1] === undefined ? null : Number(m[1]) };
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
