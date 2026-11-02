import type { ITheme } from "@xterm/xterm";

/** Windows Terminal "Ubuntu" scheme, spec 6.5. */
export const ubuntuTheme: ITheme = {
  background: "#300A24",
  foreground: "#EEEEEC",
  cursor: "#FFFFFF",
  selectionBackground: "#B5D5FF",
  black: "#2E3436",
  red: "#CC0000",
  green: "#2DC94E",
  yellow: "#C4A000",
  blue: "#4C93FF",
  magenta: "#75507B",
  cyan: "#06989A",
  white: "#D3D7CF",
  brightBlack: "#555753",
  brightRed: "#EF2929",
  brightGreen: "#6BEE85",
  brightYellow: "#FCE94F",
  brightBlue: "#8FBEFF",
  brightMagenta: "#AD7FA8",
  brightCyan: "#34E2E2",
  brightWhite: "#EEEEEC",
};

/**
 * Spec says 10pt (Windows Terminal), which is 13px in CSS. Cascadia ships
 * with Windows Terminal (local font, no web font); Consolas is the fallback.
 */
export const terminalFont = {
  fontFamily: '"Cascadia Mono", "Cascadia Code", Consolas, monospace',
  fontSize: 13,
} as const;

/**
 * Cell-exact rendering for TUIs such as Claude Code's banner: xterm draws
 * box-drawing/block characters itself (customGlyphs, WebGL renderer), rows
 * are exactly one cell high and colours are used as sent.
 */
export const renderOptions = {
  customGlyphs: true,
  lineHeight: 1,
  letterSpacing: 0,
  fontWeight: "normal",
  fontWeightBold: "bold",
  drawBoldTextInBrightColors: false,
  minimumContrastRatio: 1,
  allowTransparency: false,
} as const;
