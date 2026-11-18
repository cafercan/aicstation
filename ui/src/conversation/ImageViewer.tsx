import { useEffect, useRef } from "react";
import { useT } from "../i18n";

/**
 * Large view of one image over the whole app, fit to the window. Esc, the
 * close button or a click outside the image closes it.
 */
export default function ImageViewer({ src, onClose }: { src: string; onClose: () => void }) {
  const t = useT();
  const close = useRef<HTMLButtonElement>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  useEffect(() => {
    close.current?.focus();
    // Capture phase: Esc closes the viewer and reaches no other shortcut.
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault();
      e.stopPropagation();
      onCloseRef.current();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);

  return (
    <div className="conv-viewer" onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}>
      <div className="conv-viewer-box" role="dialog" aria-modal="true" aria-label={t("conv.imageOpen")}
        onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}>
        <img src={src} alt="" />
      </div>
      <button ref={close} type="button" className="conv-viewer-close" aria-label={t("conv.imageClose")}
        title={`${t("conv.imageClose")} (Esc)`} onClick={onClose}>×</button>
    </div>
  );
}
