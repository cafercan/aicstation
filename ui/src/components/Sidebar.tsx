import { useState, type CSSProperties } from "react";
import { statusColor, statusLabel, type StatusKind } from "../format";
import { LANGS, LANG_NAMES, errorText, setLang, useLang, useT, type Lang } from "../i18n";
import { createStore, readPref, writePref } from "../prefs";
import { THEME_IDS, THEMES, setTheme, useTheme } from "../theme";
import { isWindows } from "../platform";
import SessionMark from "./SessionMark";

export type View =
  | "dashboard"
  | "history"
  | "settings"
  | { session: string }
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
/** Whether the bottom block (language, theme, settings, about) is open; remembered per viewer. */
const footerOpen = createStore<boolean>(readPref(FOOTER_KEY) !== "0");
const setFooterOpen = (open: boolean) => { footerOpen.set(open); writePref(FOOTER_KEY, open ? "1" : "0"); };

/** Collapsible bottom block: a chevron toggle row above the prefs and the about text. */
function Footer({ settingsOn, onSettings }: { settingsOn: boolean; onSettings: () => void }) {
  const t = useT();
  const open = footerOpen.use();
  return (
    <div className="foot">
      <button type="button" className="foot-toggle" aria-expanded={open} aria-controls="side-foot-body"
        onClick={() => setFooterOpen(!open)}>
        <span className="chev" aria-hidden="true">{open ? "▾" : "▸"}</span> {t("nav.footerToggle")}
      </button>
      <div id="side-foot-body" hidden={!open}>
        <Prefs settingsOn={settingsOn} onSettings={onSettings} />
        <About />
      </div>
    </div>
  );
}

/** Language select, theme swatches and the settings gear at the bottom of the sidebar. */
function Prefs({ settingsOn, onSettings }: { settingsOn: boolean; onSettings: () => void }) {
  const t = useT();
  const lang = useLang();
  const theme = useTheme();
  return (
    <div className="prefs">
      <select className="lang-select" aria-label={t("nav.language")} value={lang}
        onChange={(e) => setLang(e.target.value as Lang)}>
        {LANGS.map((l) => <option key={l} value={l} lang={l}>{LANG_NAMES[l]}</option>)}
      </select>
      <div className="theme-switch" role="group" aria-label={t("nav.theme")}>
        {THEME_IDS.map((id) => {
          const name = t(`theme.${id}`);
          const tk = THEMES[id].tokens;
          return (
            <button key={id} type="button" className={id === theme ? "theme-swatch on" : "theme-swatch"}
              aria-pressed={id === theme} aria-label={name} title={name}
              style={{ "--sw-bg": tk.bg, "--sw-ac": tk.accent } as CSSProperties}
              onClick={() => setTheme(id)} />
          );
        })}
      </div>
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
      <div className="about-name">SessionStation</div>
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
  const isDash = view === "dashboard" || (typeof view === "object" && "session" in view);
  return (
    <nav className="side">
      <div className={isDash ? "item on" : "item"} onClick={() => onSelect("dashboard")}>{t("nav.dashboard")}</div>
      <div className={view === "history" ? "item on" : "item"} onClick={() => onSelect("history")}>{t("nav.history")}</div>
      <div className="head">{t("nav.terminals")}</div>
      {terminals.map((term) => (
        <div key={term.id}
          className={typeof view === "object" && "terminal" in view && view.terminal === term.id ? "item sub on" : "item sub"}
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
