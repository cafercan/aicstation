import { useEffect, useState } from "react";
import { getHistory, getSnapshot, onSnapshot, type HistoryView, type Snapshot } from "./api";
import Sidebar, { type View } from "./components/Sidebar";
import StatusBar from "./components/StatusBar";
import Dashboard from "./pages/Dashboard";
import "./app.css";

const EMPTY: Snapshot = { sessions: [], runningAgents: 0, waitingCount: 0 };

export default function App() {
  const [view, setView] = useState<View>("dashboard");
  const [snapshot, setSnapshot] = useState<Snapshot>(EMPTY);
  const [history, setHistory] = useState<HistoryView | null>(null);
  const [now, setNow] = useState(() => Date.now());

  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;
    getSnapshot().then((s) => alive && setSnapshot(s)).catch(() => {});
    onSnapshot((s) => alive && setSnapshot(s))
      .then((u) => { if (alive) unlisten = u; else u(); })
      .catch(() => {});
    const poll = () => getHistory().then((h) => alive && setHistory(h)).catch(() => {});
    poll();
    const hist = setInterval(poll, 5000);
    const tick = setInterval(() => setNow(Date.now()), 30000);
    return () => { alive = false; unlisten?.(); clearInterval(hist); clearInterval(tick); };
  }, []);

  const selectedId = typeof view === "object" && "session" in view ? view.session : null;

  let page;
  if (view === "dashboard") {
    page = <Dashboard snapshot={snapshot} history={history} now={now}
      onOpen={(id) => setView({ session: id })} />;
  } else {
    const what = view === "history" ? "Geçmiş" : "session" in view ? "Oturum ayrıntısı" : "Terminal";
    page = (
      <div className="empty">
        <p>{what} yakında.</p>
        <button onClick={() => setView("dashboard")}>← Dashboard</button>
      </div>
    );
  }

  return (
    <div className="app">
      <div className="body">
        <Sidebar view={view} terminals={[]} onSelect={setView} />
        <main className="main">{page}</main>
      </div>
      <StatusBar sessions={snapshot.sessions} selectedId={selectedId} />
    </div>
  );
}
