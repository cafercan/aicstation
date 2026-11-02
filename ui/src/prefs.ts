import { useSyncExternalStore } from "react";

/**
 * Per-device UI preferences in localStorage. Storage may be missing or throw
 * (blocked site data, private mode): reads then give null, writes are dropped.
 */
export function readPref(key: string): string | null {
  try { return localStorage.getItem(key); } catch { return null; }
}

export function writePref(key: string, value: string): void {
  try { localStorage.setItem(key, value); } catch { /* not persisted */ }
}

/**
 * Resets of in-memory state, run by the test-only `testing/reset.ts`. Only
 * collected under Vitest (`MODE === "test"`); in the app build the branch is
 * dead code and nothing is kept.
 */
export const testResets: (() => void)[] = [];

/** Registers `fn` to restore module state between tests (no-op outside tests). */
export function onTestReset(fn: () => void): void {
  if (import.meta.env.MODE === "test") testResets.push(fn);
}

/** A value with subscribers, read in React through `use()`. */
export function createStore<T>(initial: T) {
  let value = initial;
  const listeners = new Set<() => void>();
  const get = () => value;
  const subscribe = (l: () => void) => { listeners.add(l); return () => { listeners.delete(l); }; };
  const set = (v: T) => { if (v === value) return; value = v; listeners.forEach((l) => l()); };
  onTestReset(() => set(initial));
  return { get, set, subscribe, use: () => useSyncExternalStore(subscribe, get) };
}
