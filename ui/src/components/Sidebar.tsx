import { useState } from "react";
import { statusColor, statusLabel, type StatusKind } from "../format";
import { errorText, useT } from "../i18n";
import { createStore, readPref, writePref } from "../prefs";
import { isWindows } from "../platform";
import SessionMark from "./SessionMark";

/** Tabs of the session view. */
export type SessionTab = "terminal" | "conversation" | "info";

/**
 * `{ session }` is the session view; `terminal` is the app terminal it is
 * shown with, which the view follows (sessionView.ts resolveView).
 */
export type View =
  | "dashboard"
  | "history"
  | "settings"
  | { session: string; tab: SessionTab; terminal?: number }
  | { terminal: number };

export interface SidebarTerminal {
  id: number;
  label: string;
  /** Status of the Claude session inside, if any. */
  status: StatusKind | null;
  /** Pane it is shown in while the terminal view is split. */
  split?: "left" | "right";
}

interface Props {
  view: View;
  terminals: SidebarTerminal[];
  onSelect: (v: View) => void;
  onNew: () => void;
  onClose: (id: number) => void;
  /** Opens a terminal outside the app (Windows Terminal on Windows); rejects with an error code. */
  onOpenWt: () => Promise<void>;
}

const FOOTER_KEY = "station.footerOpen";
/** Whether the bottom block (settings, about) is open; closed until opened, then remembered per viewer. */
const footerOpen = createStore<boolean>(readPref(FOOTER_KEY) === "1");
const setFooterOpen = (open: boolean) => { footerOpen.set(open); writePref(FOOTER_KEY, open ? "1" : "0"); };

const SIDEBAR_KEY = "station.sidebarOpen";
/** Whether the sidebar is shown (hidden: a slim rail with a show button); remembered per viewer. */
export const sidebarOpen = createStore<boolean>(readPref(SIDEBAR_KEY) !== "0");
export const setSidebarOpen = (open: boolean) => { sidebarOpen.set(open); writePref(SIDEBAR_KEY, open ? "1" : "0"); };
export const toggleSidebar = () => setSidebarOpen(!sidebarOpen.get());

/** Panel-left icon of the hide / show buttons. */
function PanelIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <rect x="1.75" y="2.75" width="12.5" height="10.5" rx="2" stroke="currentColor" strokeWidth="1.5" />
      <path d="M6 3v10" stroke="currentColor" strokeWidth="1.5" />
    </svg>
  );
}

/** Shown instead of the sidebar while it is hidden: one button that brings it back. */
export function SideRail() {
  const t = useT();
  const label = t("nav.showSidebar");
  return (
    <div className="side-rail">
      <button type="button" className="side-btn" aria-label={label} title={`${label} (Ctrl+Shift+B)`}
        onClick={() => setSidebarOpen(true)}><PanelIcon /></button>
    </div>
  );
}

/** Collapsible bottom block: a chevron toggle row above the prefs and the about text. */
function Footer({ settingsOn, onSettings }: { settingsOn: boolean; onSettings: () => void }) {
  const t = useT();
  const open = footerOpen.use();
  return (
    <div className="foot">
      <button type="button" className="foot-toggle" aria-expanded={open} aria-controls="side-foot-body"
        onClick={() => setFooterOpen(!open)}>
        <span className="chev" aria-hidden="true">{open ? "▾" : "▸"}</span> {t("nav.footerToggle")}
        <span className="ver">v{__APP_VERSION__}</span>
      </button>
      <div id="side-foot-body" hidden={!open}>
        <Prefs settingsOn={settingsOn} onSettings={onSettings} />
        <About />
      </div>
    </div>
  );
}

/** The settings gear at the bottom of the sidebar (language and theme are on the settings page). */
function Prefs({ settingsOn, onSettings }: { settingsOn: boolean; onSettings: () => void }) {
  const t = useT();
  return (
    <div className="prefs">
      <button type="button" className={settingsOn ? "gear on" : "gear"} aria-pressed={settingsOn}
        title={t("nav.settings")} onClick={onSettings}>⚙ {t("nav.settings")}</button>
    </div>
  );
}

/** Product name, tagline and the non-affiliation note (also the tooltip). */
function About() {
  const t = useT();
  const note = t("about.disclaimer");
  return (
    <footer className="about" title={note}>
      <div className="about-name">AI C-Station</div>
      <div className="about-ver">{t("about.version", { v: __APP_VERSION__ })}</div>
      <div className="about-tag">{t("about.tagline")}</div>
      <div className="about-note">{note}</div>
    </footer>
  );
}

export default function Sidebar({ view, terminals, onSelect, onNew, onClose, onOpenWt }: Props) {
  const t = useT();
  const [wtError, setWtError] = useState<string | null>(null);
  const openWt = () => {
    setWtError(null);
    onOpenWt().catch((e: unknown) =>
      setWtError(typeof e === "string" || e instanceof Error ? errorText(e)
        : t(isWindows() ? "err.wt_open_failed" : "err.terminal_open_failed")));
  };
  // A session view shown with an app terminal highlights that terminal's row, not Dashboard.
  const viewTerm = typeof view === "object" && "terminal" in view ? view.terminal : undefined;
  const isDash = view === "dashboard" || (typeof view === "object" && "session" in view && viewTerm === undefined);
  return (
    <nav className="side">
      <div className="side-top">
        <button type="button" className="side-btn" aria-label={t("nav.hideSidebar")}
          title={`${t("nav.hideSidebar")} (Ctrl+Shift+B)`} onClick={() => setSidebarOpen(false)}><PanelIcon /></button>
      </div>
      <div className={isDash ? "item nav on" : "item nav"} onClick={() => onSelect("dashboard")}>{t("nav.dashboard")}</div>
      <div className={view === "history" ? "item nav on" : "item nav"} onClick={() => onSelect("history")}>{t("nav.history")}</div>
      <div className="head">{t("nav.terminals")}</div>
      {terminals.map((term) => (
        <div key={term.id}
          className={viewTerm === term.id ? "item sub on" : "item sub"}
          onClick={() => onSelect({ terminal: term.id })}>
          {term.status
            ? <SessionMark color={statusColor(term.status)} size={12}
              label={t("nav.claudeStatus", { status: statusLabel(term.status) })} />
            : <span className="dot none">○</span>}{" "}
          <span className="lbl">{term.label}</span>
          {term.split && (
            <span className="split-mark" title={t(term.split === "left" ? "nav.splitLeft" : "nav.splitRight")}
              aria-label={t(term.split === "left" ? "nav.splitLeft" : "nav.splitRight")}>
              {term.split === "left" ? "◧" : "◨"}
            </span>
          )}
          <span className="close" title={t("nav.closeTerminal")}
            onClick={(e) => { e.stopPropagation(); onClose(term.id); }}>×</span>
        </div>
      ))}
      <div className="item sub new" onClick={onNew}>{t("nav.newTerminal")}</div>
      <div className="item sub new" onClick={openWt} title={t(isWindows() ? "nav.openWtTitle" : "nav.openTermTitle")}>
        {t(isWindows() ? "nav.openWt" : "nav.openTerm")}
      </div>
      {wtError && <div className="side-err" role="alert">{wtError}</div>}
      <Footer settingsOn={view === "settings"} onSettings={() => onSelect("settings")} />
    </nav>
  );
}
