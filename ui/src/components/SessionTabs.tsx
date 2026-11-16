import type { ReactNode } from "react";
import { useT } from "../i18n";
import type { SessionTab } from "./Sidebar";

const TABS: SessionTab[] = ["terminal", "info"];

/**
 * `Terminal | Info` bar at the top of the session view; a click only switches the tab.
 * `extra` (the split controls) goes to the right end of the same row.
 */
export default function SessionTabs({ active, onSelect, extra }: {
  active: SessionTab; onSelect: (t: SessionTab) => void; extra?: ReactNode;
}) {
  const t = useT();
  return (
    <div className="session-tabs" role="tablist">
      {TABS.map((tab) => (
        <button key={tab} type="button" role="tab" aria-selected={tab === active}
          className={tab === active ? "on" : undefined} onClick={() => { if (tab !== active) onSelect(tab); }}>
          {t(tab === "terminal" ? "tabs.terminal" : "tabs.info")}
        </button>
      ))}
      {extra}
    </div>
  );
}
