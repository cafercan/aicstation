import { statusColor, statusLabel, type StatusKind } from "../format";
import ClaudeMark from "./ClaudeMark";

export type View =
  | "dashboard"
  | "history"
  | { session: string }
  | { terminal: number };

export interface SidebarTerminal {
  id: number;
  label: string;
  /** Status of the Claude session inside, if any. */
  status: StatusKind | null;
}

interface Props {
  view: View;
  terminals: SidebarTerminal[];
  onSelect: (v: View) => void;
  onNew: () => void;
  onClose: (id: number) => void;
}

export default function Sidebar({ view, terminals, onSelect, onNew, onClose }: Props) {
  const isDash = view === "dashboard" || (typeof view === "object" && "session" in view);
  return (
    <nav className="side">
      <div className={isDash ? "item on" : "item"} onClick={() => onSelect("dashboard")}>▦ Dashboard</div>
      <div className={view === "history" ? "item on" : "item"} onClick={() => onSelect("history")}>Geçmiş</div>
      <div className="head">TERMİNALLER</div>
      {terminals.map((t) => (
        <div key={t.id}
          className={typeof view === "object" && "terminal" in view && view.terminal === t.id ? "item sub on" : "item sub"}
          onClick={() => onSelect({ terminal: t.id })}>
          {t.status
            ? <ClaudeMark color={statusColor(t.status)} size={12} label={`Claude: ${statusLabel(t.status)}`} />
            : <span className="dot none">○</span>}{" "}
          <span className="lbl">{t.label}</span>
          <span className="close" title="Terminali kapat"
            onClick={(e) => { e.stopPropagation(); onClose(t.id); }}>×</span>
        </div>
      ))}
      <div className="item sub new" onClick={onNew}>＋ Yeni terminal</div>
    </nav>
  );
}
