/**
 * Which terminal color scheme and butaglyph preset go with each app theme,
 * so the whole window can look like one piece.
 */
import type { ThemeId } from "../theme";
import type { SchemeId } from "./schemes";

export const MATCH: Record<ThemeId, { scheme: SchemeId; preset: string }> = {
  "station-dark": { scheme: "tokyo-night", preset: "tokyo-night" },
  "slate-dark": { scheme: "one-dark", preset: "aurora" },
  "nord-dark": { scheme: "nord", preset: "minimal" },
  "minimal-light": { scheme: "github-light", preset: "two-line" },
  "github-light": { scheme: "github-light", preset: "minimal" },
  "sepia-light": { scheme: "gruvbox-light", preset: "carpet" },
};
