import type { HistoryView, Snapshot } from "../api";
import { fmtCost } from "../format";

interface Props { snapshot: Snapshot; history: HistoryView | null }

export default function KpiRow({ snapshot, history }: Props) {
  const cost = !history ? "?" : history.todayComplete ? fmtCost(history.todayCost) : "hesaplanıyor";
  const tiles: [string, string, string?][] = [
    [String(snapshot.sessions.length), "açık oturum"],
    [String(snapshot.runningAgents), "çalışan ajan"],
    [String(snapshot.waitingCount), "onay bekliyor", snapshot.waitingCount > 0 ? "warn" : undefined],
    [cost, "bugün"],
  ];
  return (
    <div className="kpis">
      {tiles.map(([v, l, c]) => (
        <div className="kpi" key={l}><b className={c}>{v}</b><span>{l}</span></div>
      ))}
    </div>
  );
}
