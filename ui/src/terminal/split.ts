/**
 * Split terminal view (pure state). Two open terminals side by side: `left`
 * is the one shown when "Böl" was pressed, `right` the one picked or created.
 * The focused pane is the current terminal view, so focus needs no state of
 * its own. Both xterm instances stay alive; only their boxes change.
 */

export interface Split {
  left: number;
  right: number;
  /** Left pane width as a fraction of the area (0.2–0.8). */
  ratio: number;
}

export const MIN_RATIO = 0.2;
export const MAX_RATIO = 0.8;

export function clampRatio(r: number): number {
  if (!Number.isFinite(r)) return 0.5;
  return Math.min(MAX_RATIO, Math.max(MIN_RATIO, r));
}

/** Ratio for a pointer at `x` over an area starting at `left` that is `width` wide. */
export function ratioFromPointer(x: number, left: number, width: number): number {
  if (!(width > 0)) return 0.5;
  return clampRatio((x - left) / width);
}

/** New even split; null when both are the same terminal. */
export function openSplit(left: number, right: number): Split | null {
  if (left === right) return null;
  return { left, right, ratio: 0.5 };
}

export const inSplit = (s: Split | null, id: number | null): s is Split =>
  !!s && id !== null && (id === s.left || id === s.right);

export interface Pane { id: number; left: number; width: number; active: boolean }

/**
 * Boxes (percent of the area) of the visible terminals. The split shows only
 * while one of its terminals is the current one (that pane is active); any
 * other terminal is shown alone.
 */
export function paneLayout(s: Split | null, current: number | null): Pane[] {
  if (current === null) return [];
  if (!inSplit(s, current)) return [{ id: current, left: 0, width: 100, active: true }];
  const w = Math.round(s.ratio * 1000) / 10;
  return [
    { id: s.left, left: 0, width: w, active: current === s.left },
    { id: s.right, left: w, width: Math.round((100 - w) * 10) / 10, active: current === s.right },
  ];
}

/** After terminal `id` is gone: the split ends if it was a pane, and the other pane is the one to show. */
export function splitAfterClose(s: Split | null, id: number): { split: Split | null; show: number | null } {
  if (!s) return { split: null, show: null };
  if (id === s.left) return { split: null, show: s.right };
  if (id === s.right) return { split: null, show: s.left };
  return { split: s, show: null };
}
