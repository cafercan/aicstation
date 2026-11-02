import { useEffect, type RefObject } from "react";

/**
 * "Focus the search box of the current page" (Ctrl+Shift+F). Pages register
 * a handler while they are shown; the most recent registration wins.
 */
type Handler = () => void;
const stack: Handler[] = [];

export function registerSearchFocus(h: Handler): () => void {
  stack.push(h);
  return () => {
    const i = stack.lastIndexOf(h);
    if (i >= 0) stack.splice(i, 1);
  };
}

/** Runs the current page's handler; false when no page has a search box. */
export function focusSearch(): boolean {
  const h = stack[stack.length - 1];
  if (!h) return false;
  h();
  return true;
}

/** Registers focusing (and selecting) `ref`'s input while `enabled`. */
export function useSearchFocus(ref: RefObject<HTMLInputElement>, enabled = true): void {
  useEffect(() => {
    if (!enabled) return;
    return registerSearchFocus(() => { ref.current?.focus(); ref.current?.select(); });
  }, [ref, enabled]);
}
