/**
 * How the embedded terminals look: color scheme, font, cursor, spacing.
 * A per-device UI preference in localStorage (like the app theme), read
 * field by field: a missing or invalid field takes its default. The
 * defaults are exactly the look the terminals had before this was
 * configurable (Ubuntu scheme, Cascadia stack at 13px, weight 350).
 *
 * Open terminals follow every change (see `store.ts`).
 */
import type { ITerminalOptions } from "@xterm/xterm";
import { createStore, readPref, writePref } from "../prefs";
import { DEFAULT_FONT_SIZE, MAX_FONT_SIZE, MIN_FONT_SIZE } from "./keys";
import { DEFAULT_SCHEME, SCHEMES, isScheme, type SchemeId } from "./schemes";
import { renderOptions, terminalFont } from "./theme";

export const CURSOR_STYLES = ["block", "bar", "underline"] as const;
export type CursorStyle = (typeof CURSOR_STYLES)[number];

/** Font choices; "default" is the built-in stack, "custom" uses `customFont`. */
export const FONTS = [
  "default", "Cascadia Code", "JetBrains Mono", "Fira Code", "Hack", "Source Code Pro", "Iosevka",
  "Ubuntu Mono", "DejaVu Sans Mono", "Noto Sans Mono", "IBM Plex Mono", "Victor Mono", "Consolas", "Menlo",
  "JetBrainsMono Nerd Font", "MesloLGS NF", "custom",
] as const;
export type FontChoice = (typeof FONTS)[number];

export const FONT_WEIGHTS = [300, 350, 400, 500, 600] as const;

export interface Appearance {
  scheme: SchemeId;
  font: FontChoice;
  /** Font family name used when `font` is "custom". */
  customFont: string;
  fontSize: number;
  fontWeight: (typeof FONT_WEIGHTS)[number];
  /** Row height as a multiple of the font size (1–2). */
  lineHeight: number;
  /** Extra pixels between characters (0–4). */
  letterSpacing: number;
  cursorStyle: CursorStyle;
  cursorBlink: boolean;
  /** Lines kept above the screen (1 000–100 000). */
  scrollback: number;
  /** Space around the text, in pixels (0–24). */
  padding: number;
  /** Bold text in the bright ANSI colors (classic terminal behavior). */
  boldBright: boolean;
  /** Raise text contrast to at least this ratio (1 = colors as sent, up to 7). */
  minContrast: number;
}

export const DEFAULT_APPEARANCE: Appearance = {
  scheme: DEFAULT_SCHEME,
  font: "default",
  customFont: "",
  fontSize: DEFAULT_FONT_SIZE,
  fontWeight: 350,
  lineHeight: renderOptions.lineHeight,
  letterSpacing: renderOptions.letterSpacing,
  cursorStyle: "block",
  cursorBlink: true,
  scrollback: 10000,
  padding: 6,
  boldBright: renderOptions.drawBoldTextInBrightColors,
  minContrast: renderOptions.minimumContrastRatio,
};

const STORAGE_KEY = "station.terminal";
const MAX_CUSTOM_FONT = 64;

const num = (v: unknown, min: number, max: number, fallback: number, step = 0): number => {
  if (typeof v !== "number" || !Number.isFinite(v) || v < min || v > max) return fallback;
  return step ? Math.round(v / step) * step : v;
};
const oneOf = <T,>(v: unknown, list: readonly T[], fallback: T): T =>
  (list as readonly unknown[]).includes(v) ? (v as T) : fallback;

/** A custom font name: printable, no quotes or CSS punctuation. */
export function validFontName(s: string): boolean {
  return s.length <= MAX_CUSTOM_FONT && /^[\p{L}\p{N} ._-]*$/u.test(s);
}

/** Any value (e.g. parsed storage) to a valid Appearance, field by field. */
export function sanitize(v: unknown): Appearance {
  const d = DEFAULT_APPEARANCE;
  const o = (v && typeof v === "object" ? v : {}) as Record<string, unknown>;
  const customFont = typeof o.customFont === "string" && validFontName(o.customFont) ? o.customFont : d.customFont;
  return {
    scheme: isScheme(o.scheme) ? o.scheme : d.scheme,
    font: oneOf(o.font, FONTS, d.font),
    customFont,
    fontSize: Math.round(num(o.fontSize, MIN_FONT_SIZE, MAX_FONT_SIZE, d.fontSize)),
    fontWeight: oneOf(o.fontWeight, FONT_WEIGHTS, d.fontWeight),
    lineHeight: num(o.lineHeight, 1, 2, d.lineHeight, 0.05),
    letterSpacing: num(o.letterSpacing, 0, 4, d.letterSpacing, 0.5),
    cursorStyle: oneOf(o.cursorStyle, CURSOR_STYLES, d.cursorStyle),
    cursorBlink: typeof o.cursorBlink === "boolean" ? o.cursorBlink : d.cursorBlink,
    scrollback: Math.round(num(o.scrollback, 1000, 100000, d.scrollback)),
    padding: Math.round(num(o.padding, 0, 24, d.padding)),
    boldBright: typeof o.boldBright === "boolean" ? o.boldBright : d.boldBright,
    minContrast: num(o.minContrast, 1, 7, d.minContrast, 0.5),
  };
}

export function loadAppearance(): Appearance {
  const raw = readPref(STORAGE_KEY);
  if (!raw) return { ...DEFAULT_APPEARANCE };
  try { return sanitize(JSON.parse(raw)); } catch { return { ...DEFAULT_APPEARANCE }; }
}

const store = createStore<Appearance>(loadAppearance());
export const getAppearance = store.get;
export const useAppearance = store.use;
export const subscribeAppearance = store.subscribe;

/** Changes some fields; the result is validated, stored and applied. */
export function updateAppearance(patch: Partial<Appearance>): void {
  const next = sanitize({ ...store.get(), ...patch });
  writePref(STORAGE_KEY, JSON.stringify(next));
  store.set(next);
}

export function resetAppearance(): void {
  updateAppearance(DEFAULT_APPEARANCE);
}

/** CSS font-family: the chosen font first, then the built-in stack. */
export function fontFamily(a: Appearance): string {
  const name = a.font === "custom" ? a.customFont.trim() : a.font === "default" ? "" : a.font;
  return name ? `"${name}", ${terminalFont.fontFamily}` : terminalFont.fontFamily;
}

/** The xterm options this appearance sets (the rest stay as created). */
export function xtermOptions(a: Appearance): ITerminalOptions {
  return {
    theme: { ...SCHEMES[a.scheme].theme },
    fontFamily: fontFamily(a),
    fontSize: a.fontSize,
    fontWeight: a.fontWeight,
    fontWeightBold: terminalFont.fontWeightBold,
    lineHeight: a.lineHeight,
    letterSpacing: a.letterSpacing,
    cursorStyle: a.cursorStyle,
    cursorBlink: a.cursorBlink,
    scrollback: a.scrollback,
    drawBoldTextInBrightColors: a.boldBright,
    minimumContrastRatio: a.minContrast,
  };
}
