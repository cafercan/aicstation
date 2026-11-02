import { useEffect, useRef, useState } from "react";
import { useT, type Key } from "../i18n";
import { KEYMAP, displayChord, loadLayoutMap, type ActionId, type LayoutMap } from "../shortcuts/keymap";

/** Dictionary key of each action's name (palette and help). */
export const ACTION_LABEL: Record<ActionId, Key> = {
  newTerminal: "act.newTerminal",
  closeTerminal: "act.closeTerminal",
  switchTerminal: "act.switchTerminal",
  nextTerminal: "act.nextTerminal",
  prevTerminal: "act.prevTerminal",
  dashboard: "act.dashboard",
  history: "act.history",
  focusSearch: "act.focusSearch",
  palette: "act.palette",
  toggleSplit: "act.toggleSplit",
  shortcuts: "act.shortcuts",
};

/** One row per action; the nine terminal chords collapse to "Ctrl+Alt+1…9". */
export function helpRows(): { id: ActionId; chord: string }[] {
  const rows: { id: ActionId; chord: string }[] = [];
  for (const b of KEYMAP) {
    if (b.id === "switchTerminal") {
      if (b.n === 1) rows.push({ id: b.id, chord: "Ctrl+Alt+1…9" });
    } else rows.push({ id: b.id, chord: b.chord });
  }
  return rows;
}

/** "Kısayollar" dialog (Ctrl+Shift+/ or from the palette). */
export default function ShortcutsHelp({ onClose }: { onClose: () => void }) {
  const t = useT();
  const close = useRef<HTMLButtonElement>(null);
  useEffect(() => { close.current?.focus(); }, []);
  // Key caps of the current layout for the "/" and "\\" chords.
  const [layout, setLayout] = useState<LayoutMap | null>(null);
  useEffect(() => {
    let alive = true;
    void loadLayoutMap().then((m) => { if (alive) setLayout(m); });
    return () => { alive = false; };
  }, []);
  return (
    <div className="overlay" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="help" role="dialog" aria-modal="true" aria-labelledby="help-title"
        onKeyDown={(e) => { if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); onClose(); } }}>
        <h3 id="help-title">{t("help.title")}</h3>
        <table>
          <tbody>
            {helpRows().map((r) => (
              <tr key={r.id}><td><kbd>{displayChord(r.chord, layout)}</kbd></td><td>{t(ACTION_LABEL[r.id])}</td></tr>
            ))}
          </tbody>
        </table>
        <div className="meta">{t("help.note")}</div>
        <button ref={close} type="button" className="btn" onClick={onClose}>{t("help.close")}</button>
      </div>
    </div>
  );
}
