/**
 * How the Conversation tab looks: its font size. A per-device UI preference
 * in localStorage, like the terminal appearance; an invalid stored value
 * takes the default (13 px, the size the tab had before).
 */
import { createStore, readPref, writePref } from "../prefs";

export const CONV_FONT_MIN = 10;
export const CONV_FONT_MAX = 24;
export const CONV_FONT_DEFAULT = 13;
const STORAGE_KEY = "station.conversation";

const clamp = (n: number) => Math.min(CONV_FONT_MAX, Math.max(CONV_FONT_MIN, Math.round(n)));

/** The stored size, or the default when there is none or it is invalid. */
export function loadConvFontSize(): number {
  try {
    const v = (JSON.parse(readPref(STORAGE_KEY) ?? "{}") as { fontSize?: unknown }).fontSize;
    return typeof v === "number" && Number.isFinite(v) ? clamp(v) : CONV_FONT_DEFAULT;
  } catch {
    return CONV_FONT_DEFAULT;
  }
}

const store = createStore<number>(loadConvFontSize());
export const getConvFontSize = store.get;
export const useConvFontSize = store.use;

/** Sets the size (clamped to 10–24 px), stores it and applies it. */
export function setConvFontSize(n: number): void {
  const v = Number.isFinite(n) ? clamp(n) : CONV_FONT_DEFAULT;
  writePref(STORAGE_KEY, JSON.stringify({ fontSize: v }));
  store.set(v);
}
