import { useEffect, useState, useSyncExternalStore } from "react";
import { getTerminals, subscribe, terminalForPid } from "../terminal/store";
import { screenRows } from "./statusLine";

/** The app terminal's screen is read at most this often. */
const SCREEN_MS = 250;

/**
 * `read` of the screen rows of the app terminal the session runs in
 * (`parentPid` is the shell's pid); `none` for a session outside the app.
 * Re-read as the terminal writes, throttled, with a trailing read so the
 * last state shows; a value `same` as the shown one does not re-render.
 * `read` and `same` must be stable (module-level) functions.
 */
export function useTerminalScreen<T>(
  parentPid: number | null, read: (rows: string[]) => T, same: (a: T, b: T) => boolean, none: T,
): T {
  const terminals = useSyncExternalStore(subscribe, getTerminals);
  const id = terminalForPid(parentPid);
  const term = terminals.find((t) => t.id === id)?.term;
  const [value, setValue] = useState<T>(none);
  useEffect(() => {
    if (!term) { setValue(none); return; }
    let shown: { v: T } | null = null;
    const update = () => {
      const next = read(screenRows(term));
      if (shown && same(shown.v, next)) return;
      shown = { v: next };
      setValue(next);
    };
    update();
    let timer: number | undefined;
    let last = Date.now();
    const sub = term.onWriteParsed(() => {
      if (timer !== undefined) return;
      const wait = Math.max(0, SCREEN_MS - (Date.now() - last));
      timer = window.setTimeout(() => { timer = undefined; last = Date.now(); update(); }, wait);
    });
    return () => { sub.dispose(); if (timer !== undefined) window.clearTimeout(timer); };
  }, [term]); // eslint-disable-line react-hooks/exhaustive-deps
  return value;
}
