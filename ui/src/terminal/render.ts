import { WebglAddon } from "@xterm/addon-webgl";

interface GpuAddon {
  onContextLoss(cb: () => void): { dispose(): void };
  dispose(): void;
}

/**
 * Loads the WebGL renderer (it draws box-drawing and block characters itself
 * with `customGlyphs`, so Claude Code's banner has no gaps). Must be called
 * after `term.open`. On load failure or a lost GPU context the addon is
 * disposed and xterm falls back to its default DOM renderer.
 */
export function enableWebgl(
  term: { loadAddon(addon: never): void },
  create: () => GpuAddon = () => new WebglAddon(),
): boolean {
  let addon: GpuAddon;
  try { addon = create(); } catch { return false; }
  try {
    addon.onContextLoss(() => addon.dispose());
    term.loadAddon(addon as never);
    return true;
  } catch {
    try { addon.dispose(); } catch { /* already unusable */ }
    return false;
  }
}
