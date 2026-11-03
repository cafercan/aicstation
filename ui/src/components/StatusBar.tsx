import type { SessionCard } from "../api";
import {
  fmtBreakdown, fmtContext, fmtEstCost, fmtTokens, folderName, statusKind, statusLabel, statusTextColor, usageSum,
} from "../format";
import { useT } from "../i18n";

interface Props { sessions: SessionCard[]; selectedId: string | null }

export default function StatusBar({ sessions, selectedId }: Props) {
  const t = useT();
  const c = selectedId ? sessions.find((s) => s.sessionId === selectedId) : undefined;
  const est = c ? fmtEstCost(c.costUsd) : null;
  if (!c) return <footer className="sb">{t("sb.openSessions", { n: sessions.length })}</footer>;
  return (
    <footer className="sb">
      <span className="c2">{c.model ?? "?"}</span> │ {folderName(c.cwd) || "?"} │{" "}
      <span className="g">{c.gitBranch ?? "?"}</span> │{" "}
      {t("sb.context", { pct: fmtContext(c.contextPct) })} │{" "}
      {c.agentsTotal > 0 ? t("sb.agents", { n: c.agentsTotal }) : t("sb.noAgents")} │{" "}
      <span style={{ color: statusTextColor(statusKind(c.status, c.pid)) }}>{statusLabel(c.status, c.pid)}</span> │{" "}
      <span title={fmtBreakdown(c.tokens)}>{t("usage.tokens", { n: fmtTokens(usageSum(c.tokens)) })}</span>
      {est && <>{" "}<span className="est">{est}</span></>}
    </footer>
  );
}
