import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { getHistory, getSnapshot, onSnapshot, openWindowsTerminal, type HistoryView, type Snapshot } from "./api";
import { hideToTray, onNavigate, onTrayQuit, setFocusContext } from "./api";
import Sidebar, { type View } from "./components/Sidebar";
import StatusBar from "./components/StatusBar";
import Dashboard from "./pages/Dashboard";
import History from "./pages/History";
import SessionDetailPage from "./pages/SessionDetail";
import TerminalView from "./pages/TerminalView";
import SettingsPage from "./pages/Settings";
import { currentSettings, loadSettings, pushLang } from "./settings";
import { statusKind } from "./format";
import { useLang, useT } from "./i18n";
import { useTheme } from "./theme";
import {
  allTerminalIds, closeTerminal, confirmClose, killOrphans, openResumeTerminal, openTerminal, sessionsIn,
  terminalName,
  useTerminals,
} from "./terminal/store";
import "./app.css";

const EMPTY: Snapshot = { sessions: [], runningAgents: 0, waitingCount: 0 };

export default function App() {
  // Language and theme changes re-render the whole tree (labels, inline SVG colors).
  useTheme();
  const t = useT();
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

  // PTYs left over from before a webview reload have no handle here.
  useEffect(() => { void killOrphans(); }, []);

  const terminals = useTerminals();
  const snapRef = useRef(snapshot);
  snapRef.current = snapshot;

  // Settings live in the backend; it also needs the UI language (toasts, tray).
  const lang = useLang();
  useEffect(() => { void loadSettings(); }, []);
  useEffect(() => { pushLang(lang).catch(() => {}); }, [lang]);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let alive = true;
    /** Real exit (window close without tray, or "Quit" in the tray menu). */
    const exitApp = () => {
      if (!confirmClose(allTerminalIds(), snapRef.current.sessions)) return;
      allTerminalIds().forEach((id) => { void closeTerminal(id); });
      // Destroy explicitly so a rejected destroy is logged, not swallowed.
      getCurrentWindow().destroy().catch((err) => console.error("Pencere kapatılamadı:", err));
    };
    const unlistens: Promise<() => void>[] = [onTrayQuit(exitApp)];
    try {
      unlistens.push(getCurrentWindow().onCloseRequested((e) => {
        e.preventDefault();
        if (!currentSettings()?.closeToTray) return exitApp();
        hideToTray().catch((err) => { console.error("Tepsiye küçültülemedi:", err); exitApp(); });
      }));
    } catch { /* not running inside Tauri */ }
    const all = Promise.all(unlistens);
    all.then((us) => { if (alive) unlisten = () => us.forEach((u) => u()); else us.forEach((u) => u()); })
      .catch(() => {});
    return () => { alive = false; unlisten?.(); };
  }, []);

  // Tray menu and toast clicks open a session's detail or the dashboard.
  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;
    onNavigate((id) => setView(id ? { session: id } : "dashboard"))
      .then((u) => { if (alive) unlisten = u; else u(); })
      .catch(() => {});
    return () => { alive = false; unlisten?.(); };
  }, []);

  const onNew = useCallback(() => {
    openTerminal().then((t) => setView({ terminal: t.id })).catch(() => {});
  }, []);
  /** Rejections reach the page, which shows them inline. */
  const onResume = useCallback((sessionId: string, name: string) =>
    openResumeTerminal(sessionId, name).then((t) => setView({ terminal: t.id })), []);
  const onClose = useCallback((id: number) => {
    if (!confirmClose([id], snapRef.current.sessions)) return;
    setView((v) => (typeof v === "object" && "terminal" in v && v.terminal === id ? "dashboard" : v));
    void closeTerminal(id);
  }, []);

  const termId = typeof view === "object" && "terminal" in view ? view.terminal : null;
  const sideTerms = terminals.map((t) => {
    const inside = sessionsIn(t, snapshot.sessions)[0];
    return { id: t.id, label: terminalName(t, snapshot.sessions), status: inside ? statusKind(inside.status, inside.pid) : null };
  });

  const selectedId = typeof view === "object" && "session" in view ? view.session : null;
  // The session on screen (detail, or the Claude session inside the shown
  // terminal): the backend holds back its notifications while focused.
  const termEntry = terminals.find((t) => t.id === termId);
  const focusId = selectedId ?? (termEntry ? sessionsIn(termEntry, snapshot.sessions)[0]?.sessionId ?? null : null);
  useEffect(() => { setFocusContext(focusId).catch(() => {}); }, [focusId]);

  let page;
  if (view === "dashboard") {
    page = <Dashboard snapshot={snapshot} history={history} now={now}
      onOpen={(id) => setView({ session: id })} onTerminal={(id) => setView({ terminal: id })} />;
  } else if (view === "history") {
    page = <History onOpen={(id) => setView({ session: id })} onResume={onResume}
      onTerminal={(id) => setView({ terminal: id })} />;
  } else if (view === "settings") {
    page = <SettingsPage />;
  } else if ("session" in view) {
    page = <SessionDetailPage key={view.session} sessionId={view.session}
      live={snapshot.sessions.some((s) => s.sessionId === view.session)} onResume={onResume}
      onBack={() => setView("dashboard")} onTerminal={(id) => setView({ terminal: id })} />;
  } else if (!terminals.some((t) => t.id === termId)) {
    page = (
      <div className="empty">
        <p>{t("app.terminalMissing")}</p>
        <button onClick={() => setView("dashboard")}>{t("app.backToDashboard")}</button>
      </div>
    );
  }

  return (
    <div className="app">
      <div className="body">
        <Sidebar view={view} terminals={sideTerms} onSelect={setView} onNew={onNew} onClose={onClose}
          onOpenWt={openWindowsTerminal} />
        <main className="main">
          {page}
          {terminals.map((t) => <TerminalView key={t.id} entry={t} visible={t.id === termId} />)}
        </main>
      </div>
      <StatusBar sessions={snapshot.sessions} selectedId={selectedId} />
    </div>
  );
}
