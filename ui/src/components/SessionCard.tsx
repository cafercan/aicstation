import type { SessionCard } from "../api";
import {
  AGENT_FINISHED_COLOR, AGENT_RUNNING_COLOR, clampPct, fmtContext, fmtCost, fmtElapsed, statusColor,
  statusKind, statusLabel, toolSuccess,
} from "../format";
import AgentDonut from "./AgentDonut";
import ClaudeMark from "./ClaudeMark";

interface Props {
  card: SessionCard;
  now: number;
  onOpen: (sessionId: string) => void;
}

export default function SessionCardView({ card, now, onOpen }: Props) {
  const kind = statusKind(card.status, card.pid);
  const running = Math.max(0, card.agentsTotal - card.agentsFinished);
  const finished = Math.max(0, card.agentsFinished);
  const elapsed = card.startedAtMs > 0 ? fmtElapsed(now - card.startedAtMs) : "?";
  const last = card.lastTool
    ? `son: ${card.lastTool.name}${card.lastTool.target ? " " + card.lastTool.target : ""} · `
    : "";
  return (
    <div className={`card ${kind}`} role="button" tabIndex={0}
      title={`${last}son hareket ${fmtElapsed(now - card.lastActivityMs)}`}
      onClick={() => onOpen(card.sessionId)}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") { e.preventDefault(); onOpen(card.sessionId); }
      }}>
      <div className="top">
        <h4><ClaudeMark color={statusColor(kind)} /><span className="name">{card.name}</span></h4>
        <div className="st">
          <span className="badge">{statusLabel(card.status, card.pid)}</span>
          {card.waitingFor ? <span className="waiting-for">{" "}{card.waitingFor}</span> : null}
        </div>
      </div>
      <div className="path" title={card.cwd}>
        <span className="cwd">{card.cwd}</span><span className="model">{card.model ?? "?"}</span>
      </div>
      <div className="agents">
        <AgentDonut running={running} finished={finished} />
        <div className="astats">
          {running + finished === 0 ? (
            <div className="muted">alt ajan yok</div>
          ) : (
            <>
              <div><i style={{ color: AGENT_RUNNING_COLOR }}>●</i> <span>{`${running} ajan çalışıyor`}</span></div>
              <div><i style={{ color: AGENT_FINISHED_COLOR }}>●</i> <span>{`${finished} ajan tamamlandı`}</span></div>
            </>
          )}
          <div className="muted">tool başarı <b>{toolSuccess(card.toolOk, card.toolErr)}</b></div>
        </div>
      </div>
      <div className="ctx"><i style={{ width: `${clampPct(card.contextPct)}%` }} /></div>
      <div className="row">
        <span>{`bağlam ${fmtContext(card.contextPct)}`}</span>
        <span>{`${fmtCost(card.costUsd)} · ${elapsed}`}</span>
      </div>
    </div>
  );
}
