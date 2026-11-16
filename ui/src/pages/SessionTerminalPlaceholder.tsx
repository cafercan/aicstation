import { useRef, useState } from "react";
import type { SessionCard } from "../api";
import { goToTab, gotoTarget, resumeError } from "../gotoTab";
import { errorText, useT } from "../i18n";
import { moveState } from "../sessionView";
import { openAdoptTerminal, terminalForPid } from "../terminal/store";

interface Props {
  /** The session's card in the current snapshot; missing (or pid 0) when it is closed. */
  card: SessionCard | undefined;
  sessionId: string;
  /** Shows an app terminal ("Go to tab" on an embedded target, the terminal that resumed the session). */
  onTerminal: (id: number) => void;
  /** "Move to app" started terminal `id` for `sessionId`. */
  onAdopted: (id: number, sessionId: string) => void;
  /** "▶ Resume" on a closed session; a rejection is shown inline. */
  onResume: (sessionId: string, name: string) => Promise<unknown>;
}

/**
 * Terminal tab of a session view without an app terminal. A live session
 * outside the app (case 2): where it runs, "Go to tab" and "Move to app",
 * which is enabled only while the session is idle. A closed session
 * (case 3): "▶ Resume".
 */
export default function SessionTerminalPlaceholder({ card, sessionId, onTerminal, onAdopted, onResume }: Props) {
  const tx = useT();
  const [busy, setBusy] = useState(false);
  // Guards a second click before the disabled state has rendered.
  const busyRef = useRef(false);
  const [err, setErr] = useState<{ text: string; terminal: number | null } | null>(null);
  const [note, setNote] = useState<string | null>(null);

  const run = (work: () => Promise<unknown>, onError: (e: unknown) => { text: string; terminal: number | null }) => {
    if (busyRef.current) return;
    busyRef.current = true;
    setBusy(true);
    setErr(null);
    setNote(null);
    work()
      .catch((e: unknown) => setErr(onError(e)))
      .finally(() => { busyRef.current = false; setBusy(false); });
  };

  const live = card !== undefined && card.pid !== 0;
  let body;
  if (!live) {
    body = (
      <>
        <p>{tx("place.closed")}</p>
        <div className="place-actions">
          <button className="btn" disabled={busy} onClick={() => run(
            () => onResume(sessionId, card?.name ?? ""),
            (e) => { const r = resumeError(e); return { text: tx("detail.resumeFailed", { err: r.text }), terminal: r.terminal }; },
          )}>{tx("place.resume")}</button>
        </div>
      </>
    );
  } else {
    const target = gotoTarget(card, terminalForPid(card.parentPid));
    const move = moveState(card);
    const reason = move.reason === null ? null
      : move.reason === "agents" ? tx("place.reason.agents", { n: card.agentsRunning })
      : tx(`place.reason.${move.reason}`);
    body = (
      <>
        <p>{tx(card.hostKind === "windowsTerminal" ? "place.externalHost" : "place.otherHost")}</p>
        <div className="place-actions">
          {target && (
            <button className="btn" onClick={() => {
              setErr(null);
              setNote(null);
              goToTab(sessionId, target, onTerminal)
                .then(setNote, (e: unknown) => setErr({ text: e instanceof Error ? e.message : String(e), terminal: null }));
            }}>{tx("detail.goto")}</button>
          )}
          <button className="btn" disabled={!move.enabled || busy} onClick={() => {
            if (!move.enabled || busyRef.current || !window.confirm(tx("place.moveConfirm"))) return;
            run(
              () => openAdoptTerminal(sessionId, card.name).then((t) => onAdopted(t.id, sessionId)),
              (e) => ({ text: errorText(e), terminal: null }),
            );
          }}>{tx("place.move")}</button>
        </div>
        {reason && <div className="meta">{reason}</div>}
      </>
    );
  }

  return (
    <div className="placeholder">
      {body}
      {err && (
        <div className="meta r" role="alert">
          {err.text}
          {err.terminal !== null && (
            <>{" "}<button className="btn" onClick={() => onTerminal(err.terminal!)}>{tx("detail.goto")}</button></>
          )}
        </div>
      )}
      {note && <div className="meta" role="status">{errorText(note)}</div>}
    </div>
  );
}
