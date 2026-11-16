import { useCallback, useEffect, useRef, useState } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  getHistory, getSnapshot, hideToTray, onNavigate, onSnapshot, onTrayQuit, openSystemTerminal, setFocusContext,
  type HistoryView, type Snapshot,
} from "./api";
import SessionTabs from "./components/SessionTabs";
import Sidebar, { type SessionTab, type View } from "./components/Sidebar";
import StatusBar from "./components/StatusBar";
import Dashboard from "./pages/Dashboard";
import History from "./pages/History";
import SessionDetailPage from "./pages/SessionDetail";
import SessionTerminalPlaceholder from "./pages/SessionTerminalPlaceholder";
import SettingsPage from "./pages/Settings";
import { currentSettings, loadSettings, pushLang } from "./settings";
import { statusKind } from "./format";
import { useLang, useT } from "./i18n";
import { useTheme } from "./theme";
import {
  allTerminalIds, closeTerminal, confirmClose, killOrphans, openResumeTerminal, openTerminal, sessionsIn,
  terminalForPid, terminalName,
  useTerminals,
} from "./terminal/store";
import { openSessionView, resolveView, tabBarFor, viewAfterClose } from "./sessionView";
import "./app.css";
import "./features.css";
import { splitAfterClose, type Split } from "./terminal/split";
import { AreaBar, Overlays, TerminalArea, terminalShown, termOf, useShortcuts, useSplitCleanup, type Overlay } from "./workspace";

const EMPTY: Snapshot = { sessions: [], runningAgents: 0, waitingCount: 0 };
/** How long a Resume / Move terminal waits for the registry to list its session. */
const PENDING_MS = 15_000;

/** Claude sessions inside the shown terminals, in pane order, without repeats (at most two). */
function focusSessions(ids: number[], terminals: ReturnType<typeof useTerminals>, sessions: Snapshot["sessions"]): string[] {
  const out: string[] = [];
  for (const id of ids) {
    const entry = terminals.find((t) => t.id === id);
    const s = entry ? sessionsIn(entry, sessions)[0]?.sessionId : undefined;
    if (s && !out.includes(s)) out.push(s);
  }
  return out.slice(0, 2);
}

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
  const [split, setSplit] = useState<Split | null>(null);
  const [overlay, setOverlay] = useState<Overlay>(null);
  useSplitCleanup(terminals, split, setSplit, setView);
  const splitRef = useRef(split);
  splitRef.current = split;
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
        // Until the settings have loaded, the default (minimize to tray) applies.
        if (!(currentSettings()?.closeToTray ?? true)) return exitApp();
        hideToTray().catch((err) => { console.error("Tepsiye küçültülemedi:", err); exitApp(); });
      }));
    } catch { /* not running inside Tauri */ }
    const all = Promise.all(unlistens);
    all.then((us) => { if (alive) unlisten = () => us.forEach((u) => u()); else us.forEach((u) => u()); })
      .catch(() => {});
    return () => { alive = false; unlisten?.(); };
  }, []);

  // Tray menu and toast clicks open a session's view or the dashboard.
  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;
    onNavigate((id) => setView(id ? openSessionView(id, snapRef.current.sessions, terminalForPid) : "dashboard"))
      .then((u) => { if (alive) unlisten = u; else u(); })
      .catch(() => {});
    return () => { alive = false; unlisten?.(); };
  }, []);

  // Terminals started by Resume / Move to app (terminal id -> session id)
  // until the registry lists a session in them: the session view's Terminal
  // tab goes to them meanwhile instead of offering Resume again.
  const [pending, setPending] = useState<ReadonlyMap<number, string>>(() => new Map());
  const pendingTimers = useRef(new Set<ReturnType<typeof setTimeout>>());
  useEffect(() => () => pendingTimers.current.forEach(clearTimeout), []);
  const dropPending = useCallback((id: number) => setPending((m) => {
    if (!m.has(id)) return m;
    const next = new Map(m);
    next.delete(id);
    return next;
  }), []);
  const addPending = useCallback((id: number, sessionId: string) => {
    setPending((m) => new Map(m).set(id, sessionId));
    const timer = setTimeout(() => { pendingTimers.current.delete(timer); dropPending(id); }, PENDING_MS);
    pendingTimers.current.add(timer);
  }, [dropPending]);
  useEffect(() => {
    for (const id of pending.keys()) {
      const entry = terminals.find((x) => x.id === id);
      if (!entry || sessionsIn(entry, snapshot.sessions).length > 0) dropPending(id);
    }
  }, [pending, terminals, snapshot.sessions, dropPending]);

  // The session view follows its terminal (new session id, Claude exited, terminal closed).
  useEffect(() => {
    setView((v) => resolveView(v, terminals, snapshot.sessions, pending));
  }, [view, terminals, snapshot.sessions, pending]);

  const onNew = useCallback(() => {
    openTerminal().then((t) => setView({ terminal: t.id })).catch(() => {});
  }, []);
  /** Rejections reach the page, which shows them inline. */
  const onResume = useCallback((sessionId: string, name: string) =>
    openResumeTerminal(sessionId, name).then((t) => { addPending(t.id, sessionId); setView({ terminal: t.id }); }), [addPending]);
  const onAdopted = useCallback((id: number, sessionId: string) => {
    addPending(id, sessionId);
    setView({ terminal: id });
  }, [addPending]);
  /** Card click and History open: the app terminal hosting the session, else its Info tab. */
  const openSession = useCallback((id: string) =>
    setView(openSessionView(id, snapRef.current.sessions, terminalForPid)), []);
  const onClose = useCallback((id: number) => {
    if (!confirmClose([id], snapRef.current.sessions)) return;
    // Closing a split pane leaves the other pane on screen.
    const other = splitAfterClose(splitRef.current, id).show;
    setView((v) => viewAfterClose(v, id, other));
    void closeTerminal(id);
  }, []);

  const termId = termOf(view);
  const splitShown = split !== null && (termId === split.left || termId === split.right);
  const sideTerms = terminals.map((t) => {
    const inside = sessionsIn(t, snapshot.sessions)[0];
    return {
      id: t.id, label: terminalName(t, snapshot.sessions), status: inside ? statusKind(inside.status, inside.pid) : null,
      split: splitShown ? (t.id === split.left ? "left" as const : t.id === split.right ? "right" as const : undefined) : undefined,
    };
  });
  const area = {
    terminals, sessions: snapshot.sessions, labels: new Map(sideTerms.map((s) => [s.id, s.label])), termId, split, setSplit, setView,
    newTerminal: () => openTerminal().then((t) => t.id),
  };
  const shortcuts = { ...area, overlay, setOverlay, onNew, onClose };
  useShortcuts(shortcuts);

  const selectedId = typeof view === "object" && "session" in view ? view.session : null;
  // The sessions on screen (the detail view's, or the Claude sessions in the
  // visible terminal panes, both panes of a shown split): the backend holds
  // back their notifications while the window is focused.
  const shownTerms = splitShown ? [split.left, split.right] : termId !== null ? [termId] : [];
  const focusIds = selectedId ? [selectedId] : focusSessions(shownTerms, terminals, snapshot.sessions);
  const focusKey = focusIds.join("\n");
  useEffect(() => { setFocusContext(focusKey ? focusKey.split("\n") : []).catch(() => {}); }, [focusKey]);

  let page;
  if (view === "dashboard") {
    page = <Dashboard snapshot={snapshot} history={history} now={now}
      onOpen={openSession} onTerminal={(id) => setView({ terminal: id })} />;
  } else if (view === "history") {
    page = <History onOpen={openSession} onResume={onResume}
      onTerminal={(id) => setView({ terminal: id })} />;
  } else if (view === "settings") {
    page = <SettingsPage />;
  } else if ("session" in view && view.tab === "terminal" && view.terminal === undefined) {
    page = <SessionTerminalPlaceholder key={view.session} sessionId={view.session}
      card={snapshot.sessions.find((s) => s.sessionId === view.session)} onResume={onResume} onAdopted={onAdopted}
      onTerminal={(id) => setView({ terminal: id })} />;
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

  // Terminal | Info: follows the active pane in a split; Info leaves the split as it is.
  const bar = tabBarFor(view, terminals, snapshot.sessions);
  const selectTab = (tab: SessionTab) => {
    if (!bar) return;
    const term = bar.terminal;
    if (tab === "info") setView(term !== undefined ? { session: bar.sessionId, tab, terminal: term } : { session: bar.sessionId, tab });
    else setView(term !== undefined ? { terminal: term } : { session: bar.sessionId, tab });
  };

  // The split controls share the tab row when both are on screen.
  const barInTabs = bar !== null && terminalShown(area);

  return (
    <div className="app">
      <div className="body">
        <Sidebar view={view} terminals={sideTerms} onSelect={setView} onNew={onNew} onClose={onClose}
          onOpenWt={openSystemTerminal} />
        <div className="main-col">
          {bar && <SessionTabs active={bar.active} onSelect={selectTab}
            extra={barInTabs ? <AreaBar {...area} inline /> : undefined} />}
          <main className="main">
            {page}
            <TerminalArea {...area} barInTabs={barInTabs} />
          </main>
        </div>
      </div>
      <StatusBar sessions={snapshot.sessions} selectedId={selectedId} />
      <Overlays {...shortcuts} />
    </div>
  );
}
