import { useRef } from "react";
import type { HistoryView, Snapshot } from "../api";
import KpiRow from "../components/KpiRow";
import SessionCardView from "../components/SessionCard";
import { STATUS_CHIPS, filterSessions } from "../filter";
import { statusLabel } from "../format";
import { goToTab, gotoTarget } from "../gotoTab";
import { useT } from "../i18n";
import { createStore } from "../prefs";
import { useSearchFocus } from "../shortcuts/bus";
import { terminalForPid } from "../terminal/store";

interface Props {
  snapshot: Snapshot;
  history: HistoryView | null;
  now: number;
  onOpen: (sessionId: string) => void;
  onTerminal?: (id: number) => void;
}

/** Search text and status chips; in memory for the app session only. */
const filterStore = createStore<{ query: string; chips: ReadonlySet<string> }>({ query: "", chips: new Set() });

/** Test helper: reset the in-memory filter. */
export function resetDashboardFilter(): void {
  filterStore.set({ query: "", chips: new Set() });
}

export default function Dashboard({ snapshot, history, now, onOpen, onTerminal }: Props) {
  const t = useT();
  const f = filterStore.use();
  const searchRef = useRef<HTMLInputElement>(null);
  useSearchFocus(searchRef);
  const toggle = (chip: string) => {
    const chips = new Set(f.chips);
    if (chips.has(chip)) chips.delete(chip); else chips.add(chip);
    filterStore.set({ ...f, chips });
  };
  const cards = filterSessions(snapshot.sessions, f.query, f.chips);
  return (
    <div>
      <KpiRow snapshot={snapshot} history={history} />
      <div className="filters">
        <input ref={searchRef} type="search" value={f.query} aria-label={t("filter.searchLabel")}
          placeholder={t("filter.searchPlaceholder")} onChange={(e) => filterStore.set({ ...f, query: e.target.value })} />
        <div className="chips" role="group" aria-label={t("filter.status")}>
          {STATUS_CHIPS.map((c) => (
            <button key={c} type="button" className={`chip ${c}${f.chips.has(c) ? " on" : ""}`}
              aria-pressed={f.chips.has(c)} onClick={() => toggle(c)}>{statusLabel(c)}</button>
          ))}
        </div>
      </div>
      {snapshot.sessions.length === 0 ? (
        <p className="empty">{t("dash.empty")}</p>
      ) : cards.length === 0 ? (
        <p className="empty">{t("dash.noMatch")}</p>
      ) : (
        <div className="grid">
          {cards.map((c) => {
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
