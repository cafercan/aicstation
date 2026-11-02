import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getHistory, getSnapshot, onSnapshot, type HistoryView, type Snapshot } from "./api";
import Sidebar, { type View } from "./components/Sidebar";
import StatusBar from "./components/StatusBar";
import Dashboard from "./pages/Dashboard";
import History from "./pages/History";
import SessionDetailPage from "./pages/SessionDetail";
import TerminalView from "./pages/TerminalView";
import { statusClass } from "./format";
import {
  allTerminalIds, closeTerminal, confirmClose, openTerminal, sessionsIn, terminalName, useTerminals,
} from "./terminal/store";
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

  const terminals = useTerminals();
  const snapRef = useRef(snapshot);
  snapRef.current = snapshot;

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let alive = true;
    try {
      getCurrentWindow()
        .onCloseRequested((e) => {
          if (!confirmClose(allTerminalIds(), snapRef.current.sessions)) { e.preventDefault(); return; }
          allTerminalIds().forEach((id) => { void closeTerminal(id); });
        })
        .then((u) => { if (alive) unlisten = u; else u(); })
        .catch(() => {});
    } catch { /* not running inside Tauri */ }
    return () => { alive = false; unlisten?.(); };
  }, []);

  const onNew = useCallback(() => {
    openTerminal().then((t) => setView({ terminal: t.id })).catch(() => {});
  }, []);
  const onClose = useCallback((id: number) => {
    if (!confirmClose([id], snapRef.current.sessions)) return;
    setView((v) => (typeof v === "object" && "terminal" in v && v.terminal === id ? "dashboard" : v));
    void closeTerminal(id);
  }, []);

  const termId = typeof view === "object" && "terminal" in view ? view.terminal : null;
  const sideTerms = terminals.map((t) => {
    const inside = sessionsIn(t, snapshot.sessions)[0];
    return { id: t.id, label: terminalName(t, snapshot.sessions), status: inside ? statusClass(inside.status) : null };
  });

  const selectedId = typeof view === "object" && "session" in view ? view.session : null;

  let page;
  if (view === "dashboard") {
    page = <Dashboard snapshot={snapshot} history={history} now={now}
      onOpen={(id) => setView({ session: id })} />;
  } else if (view === "history") {
    page = <History onOpen={(id) => setView({ session: id })} />;
  } else if ("session" in view) {
    page = <SessionDetailPage key={view.session} sessionId={view.session}
      onBack={() => setView("dashboard")} onTerminal={(id) => setView({ terminal: id })} />;
  } else if (!terminals.some((t) => t.id === termId)) {
    page = (
      <div className="empty">
        <p>Terminal bulunamadı.</p>
        <button onClick={() => setView("dashboard")}>← Dashboard</button>
      </div>
    );
  }

  return (
    <div className="app">
      <div className="body">
        <Sidebar view={view} terminals={sideTerms} onSelect={setView} onNew={onNew} onClose={onClose} />
        <main className="main">
          {page}
          {terminals.map((t) => <TerminalView key={t.id} entry={t} visible={t.id === termId} />)}
        </main>
      </div>
      <StatusBar sessions={snapshot.sessions} selectedId={selectedId} />
    </div>
  );
}
