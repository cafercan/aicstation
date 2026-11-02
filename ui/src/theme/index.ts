/**
 * App-chrome color themes. The palettes live here as one token map per theme
 * (palettes adapted from the user's MDViewer themes); `applyTheme` sets
 * `<html data-theme>` and writes every token as a CSS custom property, which
 * app.css uses. app.css's `:root` holds the station-dark values as the fallback
 * before the first apply (theme.test.ts keeps the two equal).
 *
 * The embedded terminal is not themed: it keeps the Ubuntu scheme everywhere.
 */
import { createStore, readPref, writePref } from "../prefs";

export const THEME_IDS = [
  "station-dark", "slate-dark", "nord-dark", "minimal-light", "github-light", "sepia-light",
] as const;
export type ThemeId = (typeof THEME_IDS)[number];
export const DEFAULT_THEME: ThemeId = "station-dark";

export const TOKENS = [
  "bg", "surfaceA", "surfaceB", "raised", "hover", "selected", "border", "borderStrong",
  "text", "strong", "muted", "dim", "track", "accent", "link", "danger", "cyan", "yellow",
  "busy", "idle", "waiting", "closed", "busyText", "idleText", "waitingText", "closedText",
  "agentRunning", "ctxA", "ctxB", "sbBg", "shadow",
] as const;
export type Token = (typeof TOKENS)[number];

export interface Theme {
  scheme: "dark" | "light";
  tokens: Record<Token, string>;
}

const DARK_SHADOW = "0 1px 2px rgba(0, 0, 0, .35), 0 4px 14px rgba(0, 0, 0, .25)";
const LIGHT_SHADOW = "0 1px 2px rgba(15, 23, 42, .06), 0 4px 14px rgba(15, 23, 42, .07)";

export const THEMES: Record<ThemeId, Theme> = {
  "station-dark": {
    scheme: "dark",
    tokens: {
      bg: "#0f1115", surfaceA: "#1b1f27", surfaceB: "#161a21", raised: "#20252e", hover: "#1d222b",
      selected: "#232a36", border: "#262c37", borderStrong: "#3a4250",
      text: "#d7dce4", strong: "#ffffff", muted: "#7d8696", dim: "#9aa3b2", track: "#252b36",
      accent: "#5aa0ff", link: "#7fd8c8", danger: "#ff7b72", cyan: "#6cd3e0", yellow: "#f5c451",
      busy: "#48c774", idle: "#ff9f43", waiting: "#ff5d73", closed: "#7d8696",
      busyText: "#6fe39a", idleText: "#ffb36b", waitingText: "#ff8193", closedText: "#c3c9d3",
      agentRunning: "#5aa0ff", ctxA: "#5aa0ff", ctxB: "#9b7bff", sbBg: "#0b0d10", shadow: DARK_SHADOW,
    },
  },
  "slate-dark": {
    scheme: "dark",
    tokens: {
      bg: "#0f172a", surfaceA: "#1a2338", surfaceB: "#141d31", raised: "#1e293b", hover: "#1e293b",
      selected: "#26324a", border: "#26314a", borderStrong: "#3b4a66",
      text: "#e2e8f0", strong: "#f8fafc", muted: "#7c8aa3", dim: "#94a3b8", track: "#26324a",
      accent: "#6366f1", link: "#22d3ee", danger: "#f87171", cyan: "#22d3ee", yellow: "#fbbf24",
      busy: "#10b981", idle: "#f59e0b", waiting: "#f43f5e", closed: "#64748b",
      busyText: "#34d399", idleText: "#fbbf24", waitingText: "#fb7185", closedText: "#cbd5e1",
      agentRunning: "#818cf8", ctxA: "#6366f1", ctxB: "#06b6d4", sbBg: "#0b1222", shadow: DARK_SHADOW,
    },
  },
  "nord-dark": {
    scheme: "dark",
    tokens: {
      bg: "#2e3440", surfaceA: "#3b4252", surfaceB: "#353c4a", raised: "#434c5e", hover: "#3b4252",
      selected: "#434c5e", border: "#4c566a", borderStrong: "#616e88",
      text: "#eceff4", strong: "#ffffff", muted: "#a3adc2", dim: "#d8dee9", track: "#4c566a",
      accent: "#88c0d0", link: "#8fbcbb", danger: "#e5838b", cyan: "#88c0d0", yellow: "#ebcb8b",
      busy: "#a3be8c", idle: "#d08770", waiting: "#bf616a", closed: "#7b88a1",
      busyText: "#b9d3a1", idleText: "#e8a68c", waitingText: "#ec949c", closedText: "#d8dee9",
      agentRunning: "#81a1c1", ctxA: "#88c0d0", ctxB: "#b48ead", sbBg: "#272c36", shadow: DARK_SHADOW,
    },
  },
  "minimal-light": {
    scheme: "light",
    tokens: {
      bg: "#f8fafc", surfaceA: "#ffffff", surfaceB: "#f1f5f9", raised: "#ffffff", hover: "#eef2f7",
      selected: "#e2e8f0", border: "#e2e8f0", borderStrong: "#cbd5e1",
      text: "#1e293b", strong: "#0f172a", muted: "#64748b", dim: "#475569", track: "#e2e8f0",
      accent: "#4f46e5", link: "#0e7490", danger: "#dc2626", cyan: "#0e7490", yellow: "#a16207",
      busy: "#16a34a", idle: "#d97706", waiting: "#dc2626", closed: "#64748b",
      busyText: "#15803d", idleText: "#b45309", waitingText: "#b91c1c", closedText: "#475569",
      agentRunning: "#4f46e5", ctxA: "#4f46e5", ctxB: "#0891b2", sbBg: "#eef2f7", shadow: LIGHT_SHADOW,
    },
  },
  "github-light": {
    scheme: "light",
    tokens: {
      bg: "#ffffff", surfaceA: "#ffffff", surfaceB: "#f6f8fa", raised: "#f6f8fa", hover: "#eaeef2",
      selected: "#dde4eb", border: "#d0d7de", borderStrong: "#afb8c1",
      text: "#1f2328", strong: "#0d1117", muted: "#6e7781", dim: "#57606a", track: "#eaeef2",
      accent: "#0969da", link: "#1b7c83", danger: "#cf222e", cyan: "#1b7c83", yellow: "#9a6700",
      busy: "#1a7f37", idle: "#bc4c00", waiting: "#cf222e", closed: "#6e7781",
      busyText: "#1a7f37", idleText: "#bc4c00", waitingText: "#cf222e", closedText: "#57606a",
      agentRunning: "#0969da", ctxA: "#0969da", ctxB: "#8250df", sbBg: "#f6f8fa", shadow: LIGHT_SHADOW,
    },
  },
  "sepia-light": {
    scheme: "light",
    tokens: {
      bg: "#f4ecd8", surfaceA: "#faf4e5", surfaceB: "#f0e7d0", raised: "#faf4e5", hover: "#ece2c8",
      selected: "#e6d9bb", border: "#ddd0b2", borderStrong: "#c4b48f",
      text: "#4b3f2f", strong: "#2f271c", muted: "#7a6b52", dim: "#665843", track: "#e6d9bb",
      accent: "#a15c2b", link: "#5f6b2e", danger: "#b23a2e", cyan: "#3f6f73", yellow: "#8a6417",
      busy: "#5c7d3f", idle: "#b0661f", waiting: "#b23a2e", closed: "#8a7b62",
      busyText: "#46622e", idleText: "#8a4b14", waitingText: "#9c3126", closedText: "#5e513d",
      agentRunning: "#3f6f73", ctxA: "#a15c2b", ctxB: "#7d6b3f", sbBg: "#ece2c8", shadow: LIGHT_SHADOW,
    },
  },
};

/** `surfaceA` → `--surface-a`. */
export const cssVar = (token: Token): string => `--${token.replace(/[A-Z]/g, (c) => "-" + c.toLowerCase())}`;

const STORAGE_KEY = "station.theme";
const isTheme = (v: unknown): v is ThemeId =>
  typeof v === "string" && (THEME_IDS as readonly string[]).includes(v);

/** Stored theme, or station-dark. */
export function loadTheme(): ThemeId {
  const v = readPref(STORAGE_KEY);
  return isTheme(v) ? v : DEFAULT_THEME;
}

const store = createStore<ThemeId>(DEFAULT_THEME);
export const getTheme = store.get;
export const useTheme = store.use;

/** Token values of the current (or given) theme, for inline SVG colors. */
export function themeTokens(id: ThemeId = getTheme()): Record<Token, string> {
  return THEMES[id].tokens;
}

/** Sets data-theme and the CSS variables; an unknown id is ignored. */
export function applyTheme(id: ThemeId, root: HTMLElement = document.documentElement): void {
  if (!isTheme(id)) return;
  const theme = THEMES[id];
  root.dataset.theme = id;
  root.style.colorScheme = theme.scheme;
  for (const name of TOKENS) root.style.setProperty(cssVar(name), theme.tokens[name]);
  store.set(id);
}

/** Applies and remembers the choice. */
export function setTheme(id: ThemeId): void {
  if (!isTheme(id)) return;
  writePref(STORAGE_KEY, id);
  applyTheme(id);
}
