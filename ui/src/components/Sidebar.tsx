export type View =
  | "dashboard"
  | "history"
  | { session: string }
  | { terminal: number };

interface Props {
  view: View;
  terminals: number[];
  onSelect: (v: View) => void;
}

export default function Sidebar({ view, terminals, onSelect }: Props) {
  const isDash = view === "dashboard" || (typeof view === "object" && "session" in view);
  return (
    <nav className="side">
      <div className={isDash ? "item on" : "item"} onClick={() => onSelect("dashboard")}>▦ Dashboard</div>
      <div className={view === "history" ? "item on" : "item"} onClick={() => onSelect("history")}>Geçmiş</div>
      <div className="head">TERMİNALLER</div>
      {terminals.map((id) => (
        <div key={id}
          className={typeof view === "object" && "terminal" in view && view.terminal === id ? "item sub on" : "item sub"}
          onClick={() => onSelect({ terminal: id })}>
          ○ Terminal {id}
        </div>
      ))}
      <div className="item sub new" title="yakında">＋ Yeni terminal</div>
    </nav>
  );
}
