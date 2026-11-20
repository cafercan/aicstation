import { useEffect, useId, useRef } from "react";
import { keepFocusIn } from "../a11y";
import { useT } from "../i18n";
import { createStore } from "../prefs";

type Ask = { message: string; resolve: (ok: boolean) => void };
const open = createStore<Ask | null>(null);

/**
 * Asks a yes/no question in the app's own dialog (not the browser's
 * `confirm`, which shows "tauri.localhost says"). A new question refuses
 * one still open.
 */
export function askConfirm(message: string): Promise<boolean> {
  return new Promise((resolve) => {
    open.get()?.resolve(false);
    open.set({ message, resolve });
  });
}

/** Where the questions of `askConfirm` show; rendered once by the app. */
export default function ConfirmHost() {
  const t = useT();
  const ask = open.use();
  const ok = useRef<HTMLButtonElement>(null);
  const id = useId();
  useEffect(() => { ok.current?.focus(); }, [ask]);
  if (!ask) return null;
  const done = (yes: boolean) => {
    if (open.get() === ask) open.set(null);
    ask.resolve(yes);
  };
  return (
    <div className="overlay" onMouseDown={(e) => { if (e.target === e.currentTarget) done(false); }}>
      <div className="help confirm" role="alertdialog" aria-modal="true" aria-describedby={id}
        onKeyDown={(e) => {
          if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); done(false); } else keepFocusIn(e);
        }}>
        <p id={id}>{ask.message}</p>
        <div className="confirm-actions">
          <button type="button" className="btn" onClick={() => done(false)}>{t("common.cancel")}</button>
          <button ref={ok} type="button" className="btn primary" onClick={() => done(true)}>{t("common.ok")}</button>
        </div>
      </div>
    </div>
  );
}
