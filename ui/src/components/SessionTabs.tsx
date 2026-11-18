import type { ReactNode } from "react";
import RemoteMark from "./RemoteMark";
import { useT } from "../i18n";
import type { SessionTab } from "./Sidebar";

const TABS: SessionTab[] = ["terminal", "conversation", "info"];
const LABEL = { terminal: "tabs.terminal", conversation: "tabs.conversation", info: "tabs.info" } as const;

/**
 * `Terminal | Conversation | Info` bar at the top of the session view; a click only switches the tab.
 * `extra` (the split controls) goes to the right end of the same row.
 */
export default function SessionTabs({ active, onSelect, extra, remote, remoteError }: {
  active: SessionTab; onSelect: (t: SessionTab) => void; extra?: ReactNode;
  /** Opens the session on claude.ai; passed only when Remote Control is on. */
  remote?: () => void; remoteError?: string | null;
}) {
  const t = useT();
  return (
    <div className="session-tabs" role="tablist">
      {TABS.map((tab) => (
        <button key={tab} type="button" role="tab" aria-selected={tab === active}
          className={tab === active ? "on" : undefined} onClick={() => { if (tab !== active) onSelect(tab); }}>
          {t(LABEL[tab])}
        </button>
      ))}
      {remote && <RemoteMark className="remote tab-remote" onOpen={remote} error={remoteError} />}
      {extra}
    </div>
  );
}
