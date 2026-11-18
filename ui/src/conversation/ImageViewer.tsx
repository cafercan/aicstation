import { useEffect, useRef, type MouseEvent } from "react";
import { useT } from "../i18n";

/**
 * Large view of one image over the whole app, fit to the window: a modal
 * dialog whose only control, the close button, holds the focus while it is
 * open; the focus goes back where it was on close. Esc, the close button or
 * a click outside the image closes it.
 */
export default function ImageViewer({ src, onClose }: { src: string; onClose: () => void }) {
  const t = useT();
  const close = useRef<HTMLButtonElement>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  useEffect(() => {
    const before = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    close.current?.focus();
    // Capture phase: Esc closes the viewer and reaches no other shortcut;
    // Tab and Shift+Tab stay on the close button. Ctrl/Alt/Meta+Tab are
    // left to the system.
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Tab" && !e.ctrlKey && !e.altKey && !e.metaKey) {
        e.preventDefault();
        close.current?.focus();
        return;
      }
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      onCloseRef.current();
    };
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      if (before?.isConnected) before.focus();
    };
  }, []);

  const outside = (e: MouseEvent) => { if (e.target === e.currentTarget) onClose(); };
  return (
    <div className="conv-viewer" role="dialog" aria-modal="true" aria-label={t("conv.imageOpen")} onClick={outside}>
      <div className="conv-viewer-box" onClick={outside}>
        <img src={src} alt="" />
      </div>
      <button ref={close} type="button" className="conv-viewer-close" aria-label={t("conv.imageClose")}
        title={`${t("conv.imageClose")} (Esc)`} onClick={onClose}>×</button>
    </div>
  );
}
