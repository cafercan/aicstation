import type { HistoryView, Snapshot } from "../api";
import { fmtEstCost, fmtTokens } from "../format";
import { useT } from "../i18n";

interface Props { snapshot: Snapshot; history: HistoryView | null }

export default function KpiRow({ snapshot, history }: Props) {
  const t = useT();
  const ready = !!history && history.todayComplete;
  const tokens = !history ? "?" : ready ? fmtTokens(history.todayTokens ?? 0) : t("kpi.computing");
  const est = ready ? fmtEstCost(history.todayCost) : null;
  const tiles: [string, string, string, string?][] = [
    ["open", String(snapshot.sessions.length), t("kpi.openSessions")],
    ["agents", String(snapshot.runningAgents), t("kpi.runningAgents")],
    ["waiting", String(snapshot.waitingCount), t("kpi.waiting"), snapshot.waitingCount > 0 ? "warn" : undefined],
    ["today", tokens, t("kpi.today")],
  ];
  return (
    <div className="kpis">
      {tiles.map(([key, v, l, c]) => (
        <div className="kpi" key={key}>
          <b className={c}>{v}</b>
          {key === "today" && est && <em className="est">{est}</em>}
          <span>{l}</span>
        </div>
      ))}
    </div>
  );
}
