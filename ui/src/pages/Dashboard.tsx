import type { HistoryView, Snapshot } from "../api";
import KpiRow from "../components/KpiRow";
import SessionCardView from "../components/SessionCard";

interface Props {
  snapshot: Snapshot;
  history: HistoryView | null;
  now: number;
  onOpen: (sessionId: string) => void;
}

export default function Dashboard({ snapshot, history, now, onOpen }: Props) {
  return (
    <div>
      <KpiRow snapshot={snapshot} history={history} />
      {snapshot.sessions.length === 0 ? (
        <p className="empty">Açık Claude Code oturumu yok.</p>
      ) : (
        <div className="grid">
          {snapshot.sessions.map((c) => (
            <SessionCardView key={c.sessionId} card={c} now={now} onOpen={onOpen} />
          ))}
        </div>
      )}
    </div>
  );
}
