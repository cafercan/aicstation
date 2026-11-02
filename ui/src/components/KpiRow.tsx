import type { HistoryView, Snapshot } from "../api";
import { fmtCost } from "../format";
import { useT } from "../i18n";

interface Props { snapshot: Snapshot; history: HistoryView | null }

export default function KpiRow({ snapshot, history }: Props) {
  const t = useT();
  const cost = !history ? "?" : history.todayComplete ? fmtCost(history.todayCost) : t("kpi.computing");
  const tiles: [string, string, string, string?][] = [
    ["open", String(snapshot.sessions.length), t("kpi.openSessions")],
    ["agents", String(snapshot.runningAgents), t("kpi.runningAgents")],
    ["waiting", String(snapshot.waitingCount), t("kpi.waiting"), snapshot.waitingCount > 0 ? "warn" : undefined],
    ["today", cost, t("kpi.today")],
  ];
  return (
    <div className="kpis">
      {tiles.map(([key, v, l, c]) => (
        <div className="kpi" key={key}><b className={c}>{v}</b><span>{l}</span></div>
      ))}
    </div>
  );
}
