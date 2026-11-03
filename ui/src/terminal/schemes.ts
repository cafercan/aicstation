import type { ITheme } from "@xterm/xterm";

/**
 * Color schemes of the embedded terminal. Each is a full 16-color palette
 * plus background, foreground, cursor and selection, from the scheme's
 * published values. "ubuntu" (Windows Terminal's Ubuntu) is the default.
 */
export interface Scheme {
  name: string;
  dark: boolean;
  theme: Required<Pick<ITheme,
    "background" | "foreground" | "cursor" | "cursorAccent" | "selectionBackground" |
    "black" | "red" | "green" | "yellow" | "blue" | "magenta" | "cyan" | "white" |
    "brightBlack" | "brightRed" | "brightGreen" | "brightYellow" | "brightBlue" |
    "brightMagenta" | "brightCyan" | "brightWhite">>;
}

/** The 16 ANSI colors in order (normal 0–7, bright 8–15). */
export const ANSI_KEYS = [
  "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
  "brightBlack", "brightRed", "brightGreen", "brightYellow", "brightBlue", "brightMagenta", "brightCyan", "brightWhite",
] as const;

type Palette = [string, string, string, string, string, string, string, string,
  string, string, string, string, string, string, string, string];

function scheme(
  name: string, dark: boolean, bg: string, fg: string, cursor: string, selection: string, p: Palette,
): Scheme {
  const colors = Object.fromEntries(ANSI_KEYS.map((k, i) => [k, p[i]])) as Record<(typeof ANSI_KEYS)[number], string>;
  return { name, dark, theme: { background: bg, foreground: fg, cursor, cursorAccent: bg, selectionBackground: selection, ...colors } };
}

export const SCHEMES = {
  ubuntu: scheme("Ubuntu", true, "#300A24", "#EEEEEC", "#FFFFFF", "#B5D5FF", [
    "#2E3436", "#CC0000", "#2DC94E", "#C4A000", "#4C93FF", "#75507B", "#06989A", "#D3D7CF",
    "#555753", "#EF2929", "#6BEE85", "#FCE94F", "#8FBEFF", "#AD7FA8", "#34E2E2", "#EEEEEC"]),
  campbell: scheme("Campbell", true, "#0C0C0C", "#CCCCCC", "#FFFFFF", "#FFFFFF66", [
    "#0C0C0C", "#C50F1F", "#13A10E", "#C19C00", "#0037DA", "#881798", "#3A96DD", "#CCCCCC",
    "#767676", "#E74856", "#16C60C", "#F9F1A5", "#3B78FF", "#B4009E", "#61D6D6", "#F2F2F2"]),
  "tokyo-night": scheme("Tokyo Night", true, "#1A1B26", "#C0CAF5", "#C0CAF5", "#33467C", [
    "#15161E", "#F7768E", "#9ECE6A", "#E0AF68", "#7AA2F7", "#BB9AF7", "#7DCFFF", "#A9B1D6",
    "#414868", "#F7768E", "#9ECE6A", "#E0AF68", "#7AA2F7", "#BB9AF7", "#7DCFFF", "#C0CAF5"]),
  "catppuccin-mocha": scheme("Catppuccin Mocha", true, "#1E1E2E", "#CDD6F4", "#F5E0DC", "#585B70", [
    "#45475A", "#F38BA8", "#A6E3A1", "#F9E2AF", "#89B4FA", "#F5C2E7", "#94E2D5", "#BAC2DE",
    "#585B70", "#F38BA8", "#A6E3A1", "#F9E2AF", "#89B4FA", "#F5C2E7", "#94E2D5", "#A6ADC8"]),
  dracula: scheme("Dracula", true, "#282A36", "#F8F8F2", "#F8F8F2", "#44475A", [
    "#21222C", "#FF5555", "#50FA7B", "#F1FA8C", "#BD93F9", "#FF79C6", "#8BE9FD", "#F8F8F2",
    "#6272A4", "#FF6E6E", "#69FF94", "#FFFFA5", "#D6ACFF", "#FF92DF", "#A4FFFF", "#FFFFFF"]),
  nord: scheme("Nord", true, "#2E3440", "#D8DEE9", "#D8DEE9", "#434C5E", [
    "#3B4252", "#BF616A", "#A3BE8C", "#EBCB8B", "#81A1C1", "#B48EAD", "#88C0D0", "#E5E9F0",
    "#4C566A", "#BF616A", "#A3BE8C", "#EBCB8B", "#81A1C1", "#B48EAD", "#8FBCBB", "#ECEFF4"]),
  "one-dark": scheme("One Dark", true, "#282C34", "#ABB2BF", "#528BFF", "#3E4451", [
    "#282C34", "#E06C75", "#98C379", "#E5C07B", "#61AFEF", "#C678DD", "#56B6C2", "#ABB2BF",
    "#5C6370", "#E06C75", "#98C379", "#E5C07B", "#61AFEF", "#C678DD", "#56B6C2", "#FFFFFF"]),
  "gruvbox-dark": scheme("Gruvbox Dark", true, "#282828", "#EBDBB2", "#EBDBB2", "#504945", [
    "#282828", "#CC241D", "#98971A", "#D79921", "#458588", "#B16286", "#689D6A", "#A89984",
    "#928374", "#FB4934", "#B8BB26", "#FABD2F", "#83A598", "#D3869B", "#8EC07C", "#EBDBB2"]),
  "rose-pine": scheme("Rosé Pine", true, "#191724", "#E0DEF4", "#524F67", "#403D52", [
    "#26233A", "#EB6F92", "#31748F", "#F6C177", "#9CCFD8", "#C4A7E7", "#EBBCBA", "#E0DEF4",
    "#6E6A86", "#EB6F92", "#31748F", "#F6C177", "#9CCFD8", "#C4A7E7", "#EBBCBA", "#E0DEF4"]),
  kanagawa: scheme("Kanagawa", true, "#1F1F28", "#DCD7BA", "#C8C093", "#2D4F67", [
    "#16161D", "#C34043", "#76946A", "#C0A36E", "#7E9CD8", "#957FB8", "#6A9589", "#C8C093",
    "#727169", "#E82424", "#98BB6C", "#E6C384", "#7FB4CA", "#938AA9", "#7AA89F", "#DCD7BA"]),
  monokai: scheme("Monokai", true, "#272822", "#F8F8F2", "#F8F8F0", "#49483E", [
    "#272822", "#F92672", "#A6E22E", "#F4BF75", "#66D9EF", "#AE81FF", "#A1EFE4", "#F8F8F2",
    "#75715E", "#F92672", "#A6E22E", "#F4BF75", "#66D9EF", "#AE81FF", "#A1EFE4", "#F9F8F5"]),
  "solarized-dark": scheme("Solarized Dark", true, "#002B36", "#839496", "#93A1A1", "#073642", [
    "#073642", "#DC322F", "#859900", "#B58900", "#268BD2", "#D33682", "#2AA198", "#EEE8D5",
    "#002B36", "#CB4B16", "#586E75", "#657B83", "#839496", "#6C71C4", "#93A1A1", "#FDF6E3"]),
  "github-dark": scheme("GitHub Dark", true, "#0D1117", "#C9D1D9", "#C9D1D9", "#264F78", [
    "#484F58", "#FF7B72", "#3FB950", "#D29922", "#58A6FF", "#BC8CFF", "#39C5CF", "#B1BAC4",
    "#6E7681", "#FFA198", "#56D364", "#E3B341", "#79C0FF", "#D2A8FF", "#56D4DD", "#F0F6FC"]),
  matrix: scheme("Matrix", true, "#000000", "#00FF41", "#00FF41", "#003B00", [
    "#000000", "#008F11", "#00FF41", "#7FFF00", "#00B32C", "#00CC33", "#33FF66", "#B3FFB3",
    "#003B00", "#00B32C", "#5CFF7A", "#B6FF6B", "#33FF66", "#66FF85", "#99FFAA", "#E6FFE6"]),
  "catppuccin-latte": scheme("Catppuccin Latte", false, "#EFF1F5", "#4C4F69", "#DC8A78", "#ACB0BE", [
    "#5C5F77", "#D20F39", "#40A02B", "#DF8E1D", "#1E66F5", "#EA76CB", "#179299", "#ACB0BE",
    "#6C6F85", "#D20F39", "#40A02B", "#DF8E1D", "#1E66F5", "#EA76CB", "#179299", "#BCC0CC"]),
  "solarized-light": scheme("Solarized Light", false, "#FDF6E3", "#657B83", "#586E75", "#EEE8D5", [
    "#073642", "#DC322F", "#859900", "#B58900", "#268BD2", "#D33682", "#2AA198", "#EEE8D5",
    "#002B36", "#CB4B16", "#586E75", "#657B83", "#839496", "#6C71C4", "#93A1A1", "#FDF6E3"]),
  "github-light": scheme("GitHub Light", false, "#FFFFFF", "#24292F", "#044289", "#BBDFFF", [
    "#24292F", "#CF222E", "#116329", "#4D2D00", "#0969DA", "#8250DF", "#1B7C83", "#6E7781",
    "#57606A", "#A40E26", "#1A7F37", "#633C01", "#218BFF", "#A475F9", "#3192AA", "#8C959F"]),
  "gruvbox-light": scheme("Gruvbox Light", false, "#FBF1C7", "#3C3836", "#3C3836", "#D5C4A1", [
    "#FBF1C7", "#CC241D", "#98971A", "#D79921", "#458588", "#B16286", "#689D6A", "#7C6F64",
    "#928374", "#9D0006", "#79740E", "#B57614", "#076678", "#8F3F71", "#427B58", "#3C3836"]),
} satisfies Record<string, Scheme>;

export type SchemeId = keyof typeof SCHEMES;
export const SCHEME_IDS = Object.keys(SCHEMES) as SchemeId[];
export const DEFAULT_SCHEME: SchemeId = "ubuntu";

export const isScheme = (v: unknown): v is SchemeId => typeof v === "string" && Object.prototype.hasOwnProperty.call(SCHEMES, v);
