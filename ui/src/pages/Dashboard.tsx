import type { HistoryView, Snapshot } from "../api";
import KpiRow from "../components/KpiRow";
import SessionCardView from "../components/SessionCard";
import { goToTab, gotoTarget } from "../gotoTab";
import { useT } from "../i18n";
import { terminalForPid } from "../terminal/store";

interface Props {
  snapshot: Snapshot;
  history: HistoryView | null;
  now: number;
  onOpen: (sessionId: string) => void;
  onTerminal?: (id: number) => void;
}

export default function Dashboard({ snapshot, history, now, onOpen, onTerminal }: Props) {
  const t = useT();
  return (
    <div>
      <KpiRow snapshot={snapshot} history={history} />
      {snapshot.sessions.length === 0 ? (
        <p className="empty">{t("dash.empty")}</p>
      ) : (
        <div className="grid">
          {snapshot.sessions.map((c) => {
            const target = gotoTarget(c, terminalForPid(c.parentPid));
            return (
              <SessionCardView key={c.sessionId} card={c} now={now} onOpen={onOpen}
                onGoto={target ? () => goToTab(c.sessionId, target, onTerminal) : undefined} />
            );
          })}
        </div>
      )}
    </div>
  );
}
