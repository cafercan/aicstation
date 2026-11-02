import { useRef, useState, type KeyboardEvent, type PointerEvent } from "react";
import { useT } from "../i18n";
import { MAX_RATIO, MIN_RATIO, clampRatio, ratioFromPointer } from "../terminal/split";

interface BarProps {
  /** Split shown right now (the current terminal is one of its panes). */
  splitShown: boolean;
  /** Other open terminals the right pane can show. */
  others: { id: number; label: string }[];
  onSplit: (right: number | "new") => void;
  onJoin: () => void;
}

/** Toolbar above the terminal area: "Böl" (pick the right pane) / "Birleştir". */
export function TermBar({ splitShown, others, onSplit, onJoin }: BarProps) {
  const t = useT();
  const [picking, setPicking] = useState(false);
  if (splitShown) {
    return (
      <div className="term-bar">
        <button type="button" className="btn" title="Ctrl+Shift+\" onClick={onJoin}>{t("split.join")}</button>
      </div>
    );
  }
  return (
    <div className="term-bar">
      {picking && (
        <select autoFocus aria-label={t("split.pick")} value=""
          onKeyDown={(e) => { if (e.key === "Escape") setPicking(false); }}
          onChange={(e) => {
            setPicking(false);
            const v = e.target.value;
            if (v) onSplit(v === "new" ? "new" : Number(v));
          }}>
          <option value="" disabled>{t("split.pick")}</option>
          {others.map((o) => <option key={o.id} value={String(o.id)}>{o.label}</option>)}
          <option value="new">{t("split.newTerminal")}</option>
        </select>
      )}
      <button type="button" className="btn" aria-expanded={picking} title="Ctrl+Shift+\"
        onClick={() => setPicking((p) => !p)}>{t("split.split")}</button>
    </div>
  );
}

interface DividerProps {
  ratio: number;
  onRatio: (r: number) => void;
}

/** Draggable (and arrow-key) divider between the panes; each side keeps at least 20%. */
export function SplitDivider({ ratio, onRatio }: DividerProps) {
  const t = useT();
  const [drag, setDrag] = useState(false);
  const area = useRef<HTMLElement | null>(null);
  const down = (e: PointerEvent<HTMLDivElement>) => {
    area.current = e.currentTarget.parentElement;
    e.currentTarget.setPointerCapture?.(e.pointerId);
    setDrag(true);
    e.preventDefault();
  };
  const move = (e: PointerEvent<HTMLDivElement>) => {
    if (!drag || !area.current) return;
    const r = area.current.getBoundingClientRect();
    onRatio(ratioFromPointer(e.clientX, r.left, r.width));
  };
  const up = (e: PointerEvent<HTMLDivElement>) => {
    e.currentTarget.releasePointerCapture?.(e.pointerId);
    setDrag(false);
  };
  const key = (e: KeyboardEvent<HTMLDivElement>) => {
    const step = e.key === "ArrowLeft" ? -0.05 : e.key === "ArrowRight" ? 0.05 : 0;
    if (e.key === "Home") onRatio(MIN_RATIO);
    else if (e.key === "End") onRatio(MAX_RATIO);
    else if (step) onRatio(clampRatio(ratio + step));
    else return;
    e.preventDefault();
  };
  return (
    <div className={drag ? "split-divider drag" : "split-divider"} role="separator" aria-orientation="vertical"
      aria-label={t("split.divider")} aria-valuemin={MIN_RATIO * 100} aria-valuemax={MAX_RATIO * 100}
      aria-valuenow={Math.round(ratio * 100)} tabIndex={0} style={{ left: `${ratio * 100}%` }}
      onPointerDown={down} onPointerMove={move} onPointerUp={up} onPointerCancel={up} onKeyDown={key} />
  );
}
