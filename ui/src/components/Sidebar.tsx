export type View =
  | "dashboard"
  | "history"
  | { session: string }
  | { terminal: number };

export interface SidebarTerminal {
  id: number;
  label: string;
  /** Status of the Claude session inside, if any. */
  status: "busy" | "waiting" | "idle" | null;
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
          <span className={`dot ${t.status ?? "none"}`}>{t.status ? "●" : "○"}</span> {t.label}
          <span className="close" title="Terminali kapat"
            onClick={(e) => { e.stopPropagation(); onClose(t.id); }}>×</span>
        </div>
      ))}
      <div className="item sub new" onClick={onNew}>＋ Yeni terminal</div>
    </nav>
  );
}
