import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useT } from "../i18n";

/** Width the menu is kept inside the window with. */
const MENU_W = 160;

/**
 * Right-click menu over selected conversation text: Quote (only where there
 * is a message box) and Copy. Closes on Esc, a press outside it, scrolling or
 * a window resize; the first item takes the focus.
 */
export default function QuoteMenu({ x, y, onQuote, onCopy, onClose }: {
  x: number; y: number; onQuote?: () => void; onCopy: () => void; onClose: () => void;
}) {
  const t = useT();
  const ref = useRef<HTMLDivElement>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  useEffect(() => {
    ref.current?.querySelector("button")?.focus();
    const close = () => onCloseRef.current();
    const onKey = (e: KeyboardEvent) => {
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

  // Opened near the bottom, it opens upward enough to stay inside the window.
  const [top, setTop] = useState(y);
  useLayoutEffect(() => {
    const h = ref.current?.offsetHeight ?? 0;
    setTop(Math.max(0, Math.min(y, window.innerHeight - h)));
  }, [y]);

  const left = Math.max(0, Math.min(x, window.innerWidth - MENU_W));
  return (
    <div ref={ref} className="conv-menu" role="menu" style={{ left, top }}>
      {onQuote && <button type="button" role="menuitem" onClick={onQuote}>{t("conv.quote")}</button>}
      <button type="button" role="menuitem" onClick={onCopy}>{t("conv.copy")}</button>
    </div>
  );
}
