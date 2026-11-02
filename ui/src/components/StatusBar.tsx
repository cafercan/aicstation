import type { SessionCard } from "../api";
import { fmtCost, folderName, statusLabel } from "../format";

interface Props { sessions: SessionCard[]; selectedId: string | null }

export default function StatusBar({ sessions, selectedId }: Props) {
  const c = selectedId ? sessions.find((s) => s.sessionId === selectedId) : undefined;
  if (!c) return <footer className="sb">{sessions.length} açık oturum</footer>;
  return (
    <footer className="sb">
      <span className="c2">{c.model ?? "?"}</span> │ {folderName(c.cwd) || "?"} │{" "}
      <span className="g">{c.gitBranch ?? "?"}</span> │{" "}
      ctx {c.contextPct == null ? "?" : `${Math.round(c.contextPct)}%`} │{" "}
      {c.agentsTotal > 0 ? `${c.agentsTotal} ajan` : "ajan yok"} │ {statusLabel(c.status)} │{" "}
      {fmtCost(c.costUsd)}
    </footer>
  );
}
