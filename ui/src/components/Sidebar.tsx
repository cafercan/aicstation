import { useState, type CSSProperties } from "react";
import { statusColor, statusLabel, type StatusKind } from "../format";
import { LANGS, LANG_NAMES, errorText, setLang, useLang, useT, type Lang } from "../i18n";
import { THEME_IDS, THEMES, setTheme, useTheme } from "../theme";
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
  /** Opens a PowerShell tab in Windows Terminal; rejects with an error code. */
  onOpenWt: () => Promise<void>;
}

/** Language select and theme swatches at the bottom of the sidebar. */
function Prefs() {
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
    </div>
  );
}

export default function Sidebar({ view, terminals, onSelect, onNew, onClose, onOpenWt }: Props) {
  const t = useT();
  const [wtError, setWtError] = useState<string | null>(null);
  const openWt = () => {
    setWtError(null);
    onOpenWt().catch((e: unknown) =>
      setWtError(typeof e === "string" || e instanceof Error ? errorText(e) : t("err.wt_open_failed")));
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
            ? <ClaudeMark color={statusColor(term.status)} size={12}
              label={t("nav.claudeStatus", { status: statusLabel(term.status) })} />
            : <span className="dot none">○</span>}{" "}
          <span className="lbl">{term.label}</span>
          <span className="close" title={t("nav.closeTerminal")}
            onClick={(e) => { e.stopPropagation(); onClose(term.id); }}>×</span>
        </div>
      ))}
      <div className="item sub new" onClick={onNew}>{t("nav.newTerminal")}</div>
      <div className="item sub new" onClick={openWt} title={t("nav.openWtTitle")}>{t("nav.openWt")}</div>
      {wtError && <div className="side-err" role="alert">{wtError}</div>}
      <Prefs />
    </nav>
  );
}
