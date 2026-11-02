import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { useT } from "../i18n";
import { fuzzyRank } from "../shortcuts/fuzzy";

export interface PaletteItem {
  key: string;
  label: string;
  /** Extra text that is searched and shown dimmed (a folder, a chord). */
  detail?: string;
  kind: "action" | "session" | "terminal";
  run: () => void;
}

interface Props { items: PaletteItem[]; onClose: () => void }

/** Ctrl+Shift+P: fuzzy list of actions, open sessions and terminals. Enter runs, Esc closes. */
export default function CommandPalette({ items, onClose }: Props) {
  const t = useT();
  const [query, setQuery] = useState("");
  const [sel, setSel] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  useEffect(() => { input.current?.focus(); }, []);
  const list = fuzzyRank(items, query, (i) => `${i.label} ${i.detail ?? ""}`);
  const cur = Math.min(sel, Math.max(0, list.length - 1));
  const run = (i: PaletteItem | undefined) => {
    if (!i) return;
    onClose();
    i.run();
  };
  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); onClose(); }
    else if (e.key === "ArrowDown") { e.preventDefault(); setSel((cur + 1) % Math.max(1, list.length)); }
    else if (e.key === "ArrowUp") { e.preventDefault(); setSel((cur - 1 + list.length) % Math.max(1, list.length)); }
    else if (e.key === "Enter") { e.preventDefault(); run(list[cur]); }
  };
  const kind = (k: PaletteItem["kind"]) =>
    k === "action" ? t("palette.kindAction") : k === "session" ? t("palette.kindSession") : t("palette.kindTerminal");
  return (
    <div className="overlay" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="palette" role="dialog" aria-modal="true" aria-label={t("palette.title")} onKeyDown={onKey}>
        <input ref={input} value={query} role="combobox" aria-expanded="true" aria-controls="palette-list"
          aria-activedescendant={list[cur] ? `pal-${list[cur].key}` : undefined} aria-label={t("palette.title")}
          placeholder={t("palette.placeholder")} onChange={(e) => { setQuery(e.target.value); setSel(0); }} />
        {list.length === 0 ? <div className="none">{t("palette.none")}</div> : (
          <ul id="palette-list" role="listbox" aria-label={t("palette.title")}>
            {list.map((i, n) => (
              <li key={i.key} id={`pal-${i.key}`} role="option" aria-selected={n === cur} className={n === cur ? "on" : undefined}
                onMouseMove={() => { if (n !== cur) setSel(n); }} onClick={() => run(i)}>
                <span className="lbl">{i.label}</span>
                {i.detail && (i.kind === "action" ? <kbd>{i.detail}</kbd> : <span className="kind" title={i.detail}>{i.detail}</span>)}
                <span className="kind">{kind(i.kind)}</span>
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
