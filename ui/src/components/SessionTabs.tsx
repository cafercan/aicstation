import { useT } from "../i18n";
import type { SessionTab } from "./Sidebar";

const TABS: SessionTab[] = ["terminal", "info"];

/** `Terminal | Info` bar at the top of the session view; a click only switches the tab. */
export default function SessionTabs({ active, onSelect }: { active: SessionTab; onSelect: (t: SessionTab) => void }) {
  const t = useT();
  return (
    <div className="session-tabs" role="tablist">
      {TABS.map((tab) => (
        <button key={tab} type="button" role="tab" aria-selected={tab === active}
          className={tab === active ? "on" : undefined} onClick={() => { if (tab !== active) onSelect(tab); }}>
          {t(tab === "terminal" ? "tabs.terminal" : "tabs.info")}
        </button>
      ))}
    </div>
  );
}
