import { Fragment, useEffect, useLayoutEffect, useRef, useState, type KeyboardEvent } from "react";

export interface MenuItem {
  label: string;
  onSelect: () => void;
  /** A destructive action (shown in the danger color). */
  danger?: boolean;
  disabled?: boolean;
  /** A line goes above this item. */
  separated?: boolean;
}

/**
 * A right-click menu at (x, y), kept inside the window. The first item takes
 * the focus; ↑/↓, Home and End move it. Closes on Esc, a press outside it,
 * scrolling or a window resize; picking an item closes it first.
 */
export default function ContextMenu({ x, y, items, label, onClose }: {
  x: number; y: number; items: MenuItem[]; label?: string; onClose: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  useEffect(() => {
    ref.current?.querySelector<HTMLButtonElement>("button:not([disabled])")?.focus();
    const close = () => onCloseRef.current();
    const onKey = (e: globalThis.KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      close();
    };
    const onDown = (e: MouseEvent) => { if (!ref.current?.contains(e.target as Node)) close(); };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("mousedown", onDown, true);
    window.addEventListener("resize", close);
    window.addEventListener("scroll", close, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("mousedown", onDown, true);
      window.removeEventListener("resize", close);
      window.removeEventListener("scroll", close, true);
    };
  }, []);

  // Opened near the right or bottom edge, it moves left or up to stay inside the window.
  const [pos, setPos] = useState({ left: x, top: y });
  useLayoutEffect(() => {
    const w = ref.current?.offsetWidth ?? 0;
    const h = ref.current?.offsetHeight ?? 0;
    setPos({
      left: Math.max(0, Math.min(x, window.innerWidth - w)),
      top: Math.max(0, Math.min(y, window.innerHeight - h)),
    });
  }, [x, y]);

  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const buttons = [...(ref.current?.querySelectorAll<HTMLButtonElement>("button:not([disabled])") ?? [])];
    if (buttons.length === 0) return;
    const at = buttons.indexOf(document.activeElement as HTMLButtonElement);
    const to = e.key === "ArrowDown" ? (at + 1) % buttons.length
      : e.key === "ArrowUp" ? (at <= 0 ? buttons.length - 1 : at - 1)
        : e.key === "Home" ? 0 : e.key === "End" ? buttons.length - 1 : -1;
    if (to < 0) return;
    e.preventDefault();
    buttons[to].focus();
  };

  return (
    <div ref={ref} className="conv-menu" role="menu" aria-label={label} style={pos} onKeyDown={onKeyDown}>
      {items.map((it, i) => (
        <Fragment key={i}>
          {it.separated && <div className="menu-sep" role="separator" />}
          <button type="button" role="menuitem" className={it.danger ? "danger" : undefined} disabled={it.disabled}
            onClick={() => { onClose(); it.onSelect(); }}>{it.label}</button>
        </Fragment>
      ))}
    </div>
  );
}
