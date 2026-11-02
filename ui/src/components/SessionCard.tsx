import type { SessionCard } from "../api";
import { fmtElapsed, statusClass, statusLabel } from "../format";

interface Props {
  card: SessionCard;
  now: number;
  onOpen: (sessionId: string) => void;
}

export default function SessionCardView({ card, now, onOpen }: Props) {
  const cls = statusClass(card.status);
  const agents = card.agentsTotal > 0
    ? `${card.agentsTotal} ajan · ${card.agentsFinished} bitti`
    : "ajan yok";
  const last = card.lastTool
    ? `son: ${card.lastTool.name}${card.lastTool.target ? " " + card.lastTool.target : ""}`
    : "—";
  const pct = card.contextPct == null ? null : Math.max(0, Math.min(100, card.contextPct));
  return (
    <div className={`card ${cls}`} role="button" tabIndex={0}
      onClick={() => onOpen(card.sessionId)}
      onKeyDown={(e) => { if (e.key === "Enter") onOpen(card.sessionId); }}>
      <h4>
        <span className="name">{card.name}</span>{" "}
        <span className="badge">{statusLabel(card.status)}</span>
      </h4>
      <div className="path" title={card.cwd}>{card.cwd}</div>
      <div className="row"><span>{agents}</span><span>{card.model ?? "?"}</span></div>
      <div className="row"><span>{last}</span><span>{fmtElapsed(now - card.lastActivityMs)}</span></div>
      <div className="row"><span>{pct == null ? "bağlam ?" : `bağlam ${Math.round(pct)}%`}</span></div>
      <div className="bar"><i style={{ width: `${pct ?? 0}%` }} /></div>
    </div>
  );
}
