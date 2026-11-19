import { useEffect, useRef, useState, type MouseEvent } from "react";
import { useT } from "../i18n";

/** Zoom range, as a multiple of the size that fits the window. */
const MIN_SCALE = 1;
const MAX_SCALE = 8;
/** Zoom per wheel notch (deltaY 100) and per +/- key. */
const STEP = 1.25;
/** Below this many pixels of movement a press is a click, not a drag. */
const DRAG_PX = 3;

interface View { s: number; x: number; y: number }
const FIT: View = { s: 1, x: 0, y: 0 };

/**
 * Scales `v` by `f` (clamped to the zoom range) so that the point at (px, py),
 * relative to the box centre, stays where it is. Back at the fitted size the
 * image is centred again.
 */
function zoomAt(v: View, f: number, px: number, py: number): View {
  const s = Math.min(MAX_SCALE, Math.max(MIN_SCALE, Math.round(v.s * f * 10000) / 10000));
  if (s === MIN_SCALE) return FIT;
  const k = s / v.s;
  return { s, x: px - (px - v.x) * k, y: py - (py - v.y) * k };
}

/**
 * Large view of one image over the whole app, fit to the window: a modal
 * dialog whose only control, the close button, holds the focus while it is
 * open; the focus goes back where it was on close. Esc, the close button or
 * a click outside the image closes it. Ctrl+wheel (or +, - and 0) zooms
 * around the cursor; a zoomed image is moved by dragging it.
 */
export default function ImageViewer({ src, onClose }: { src: string; onClose: () => void }) {
  const t = useT();
  const [view, setView] = useState<View>(FIT);
  const close = useRef<HTMLButtonElement>(null);
  const root = useRef<HTMLDivElement>(null);
  const box = useRef<HTMLDivElement>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;
  /** The press that just ended moved the image: its click is not a backdrop click. */
  const dragged = useRef(false);
  const [dragging, setDragging] = useState(false);

  useEffect(() => {
    const before = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    close.current?.focus();
    // Capture phase: Esc closes the viewer and reaches no other shortcut;
    // Tab and Shift+Tab stay on the close button. Ctrl/Alt/Meta+Tab are
    // left to the system. +, - and 0 zoom.
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Tab" && !e.ctrlKey && !e.altKey && !e.metaKey) {
        e.preventDefault();
        close.current?.focus();
        return;
      }
      if (e.key === "+" || e.key === "=" || e.key === "-" || e.key === "0") {
        e.preventDefault();
        e.stopPropagation();
        if (e.key === "0") setView(FIT);
        else setView((v) => zoomAt(v, e.key === "-" ? 1 / STEP : STEP, 0, 0));
        return;
      }
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      onCloseRef.current();
    };
    // Not passive: Ctrl+wheel must not also zoom the whole app.
    const onWheel = (e: WheelEvent) => {
      if (!e.ctrlKey) return;
      e.preventDefault();
      const r = box.current?.getBoundingClientRect();
      const px = e.clientX - (r ? r.left + r.width / 2 : 0);
      const py = e.clientY - (r ? r.top + r.height / 2 : 0);
      setView((v) => zoomAt(v, STEP ** (-e.deltaY / 100), px, py));
    };
    const el = root.current;
    window.addEventListener("keydown", onKey, true);
    el?.addEventListener("wheel", onWheel, { passive: false });
    return () => {
      window.removeEventListener("keydown", onKey, true);
      el?.removeEventListener("wheel", onWheel);
      if (before?.isConnected) before.focus();
    };
  }, []);

  const startDrag = (e: MouseEvent) => {
    if (e.button !== 0 || view.s === MIN_SCALE) return;
    e.preventDefault();
    const from = { cx: e.clientX, cy: e.clientY, x: view.x, y: view.y };
    let moved = false;
    dragged.current = false;
    setDragging(true);
    const move = (m: globalThis.MouseEvent) => {
      const dx = m.clientX - from.cx;
      const dy = m.clientY - from.cy;
      if (Math.abs(dx) > DRAG_PX || Math.abs(dy) > DRAG_PX) moved = true;
      setView((v) => ({ ...v, x: from.x + dx, y: from.y + dy }));
    };
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      setDragging(false);
      // The click that follows this mouseup is part of the drag.
      dragged.current = moved;
      window.setTimeout(() => { dragged.current = false; }, 0);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };

  const outside = (e: MouseEvent) => {
    if (dragged.current) { dragged.current = false; return; }
    if (e.target === e.currentTarget) onClose();
  };
  const zoomed = view.s !== MIN_SCALE;
  return (
    <div ref={root} className="conv-viewer" role="dialog" aria-modal="true" aria-label={t("conv.imageOpen")} onClick={outside}>
      <div ref={box} className="conv-viewer-box" onClick={outside}>
        <img src={src} alt="" draggable={false} onMouseDown={startDrag}
          className={zoomed ? (dragging ? "dragging" : "zoomed") : undefined}
          style={{ transform: `translate(${view.x}px, ${view.y}px) scale(${view.s})` }} />
      </div>
      {zoomed && <div className="conv-viewer-zoom">{Math.round(view.s * 100)}%</div>}
      <button ref={close} type="button" className="conv-viewer-close" aria-label={t("conv.imageClose")}
        title={`${t("conv.imageClose")} (Esc)`} onClick={onClose}>×</button>
    </div>
  );
}
