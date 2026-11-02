import { useEffect, useState } from "react";
import type { SessionCard } from "../api";
import {
  agentColor, clampPct, fmtContext, fmtCost, fmtElapsed, statusColor, statusKind, statusLabel, toolSuccess,
} from "../format";
import { errorText, useT } from "../i18n";
import AgentDonut from "./AgentDonut";
import SessionMark from "./SessionMark";

/** How long the "Go to tab" note stays on the card. */
export const GOTO_NOTE_MS = 6000;

interface Props {
  card: SessionCard;
  now: number;
  onOpen: (sessionId: string) => void;
  /** "Go to tab" icon; omitted when the session's tab cannot be reached. Resolves to a note (code or text). */
  onGoto?: () => Promise<string | null>;
}

export default function SessionCardView({ card, now, onOpen, onGoto }: Props) {
  const t = useT();
  /** "Go to tab" note or error; cleared after GOTO_NOTE_MS or by the next click. */
  const [note, setNote] = useState<string | null>(null);
  useEffect(() => {
    if (!note) return;
    const id = setTimeout(() => setNote(null), GOTO_NOTE_MS);
    return () => clearTimeout(id);
  }, [note]);
  const kind = statusKind(card.status, card.pid);
  const running = Math.max(0, card.agentsTotal - card.agentsFinished);
  const finished = Math.max(0, card.agentsFinished);
  const elapsed = card.startedAtMs > 0 ? fmtElapsed(now - card.startedAtMs) : "?";
  const last = card.lastTool
    ? `${t("card.lastTool", { tool: card.lastTool.name + (card.lastTool.target ? " " + card.lastTool.target : "") })} · `
    : "";
  const goto = t("card.goto");
  return (
    <div className={`card ${kind}`} role="button" tabIndex={0}
      title={`${last}${t("card.lastActivity", { t: fmtElapsed(now - card.lastActivityMs) })}`}
      onClick={() => onOpen(card.sessionId)}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onOpen(card.sessionId); }
      }}>
      <div className="top">
        <h4><SessionMark color={statusColor(kind)} /><span className="name">{card.name}</span></h4>
        <div className="st">
          <span className="badge">{statusLabel(card.status, card.pid)}</span>
          {card.waitingFor ? <span className="waiting-for">{" "}{card.waitingFor}</span> : null}
          {onGoto && (
            <button className="goto" aria-label={goto} title={goto}
              onClick={(e) => {
                e.stopPropagation();
                setNote(null);
                onGoto().then(setNote, (err: unknown) => setNote(err instanceof Error ? err.message : String(err)));
              }}
              onKeyDown={(e) => e.stopPropagation()}>⇱</button>
          )}
        </div>
      </div>
      {note && <div className="goto-note" role="alert">{errorText(note)}</div>}
      <div className="path" title={card.cwd}>
        <span className="cwd">{card.cwd}</span><span className="model">{card.model ?? "?"}</span>
      </div>
      <div className="agents">
        <AgentDonut running={running} finished={finished} />
        <div className="astats">
          {running + finished === 0 ? (
            <div className="muted">{t("card.noSubagents")}</div>
          ) : (
            <>
              <div><i style={{ color: agentColor("running") }}>●</i> <span>{t("card.agentsRunning", { n: running })}</span></div>
              <div><i style={{ color: agentColor("finished") }}>●</i> <span>{t("card.agentsFinished", { n: finished })}</span></div>
            </>
          )}
          <div className="muted">{t("card.toolSuccess")} <b>{toolSuccess(card.toolOk, card.toolErr)}</b></div>
        </div>
      </div>
      <div className="ctx"><i style={{ width: `${clampPct(card.contextPct)}%` }} /></div>
      <div className="row">
        <span>{t("card.context", { pct: fmtContext(card.contextPct) })}</span>
        <span>{`${fmtCost(card.costUsd)} · ${elapsed}`}</span>
      </div>
    </div>
  );
}
