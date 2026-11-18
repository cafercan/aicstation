import type { ConvItem, ConvPage } from "../api";

/** The loaded window of a conversation: items `start .. start + items.length` of `total`. */
export type ConvState = { epoch: number; start: number; total: number; items: ConvItem[] };

/**
 * Merges a page from `getConversationPage` (older items) or
 * `getConversationSince` (replaces the last loaded item and appends).
 * A new epoch, or a page that would leave a gap, replaces the window.
 */
export function applyPage(s: ConvState | null, p: ConvPage): ConvState {
  const fresh = { epoch: p.epoch, start: p.start, total: p.total, items: p.items };
  if (!s || s.epoch !== p.epoch) return fresh;
  if (p.start < s.start) {
    const head = p.items.slice(0, s.start - p.start);
    if (p.start + head.length < s.start) return fresh;
    return { epoch: s.epoch, start: p.start, total: p.total, items: [...head, ...s.items] };
  }
  const at = p.start - s.start;
  if (at > s.items.length) return fresh;
  return { epoch: s.epoch, start: s.start, total: p.total, items: [...s.items.slice(0, at), ...p.items] };
}

export const hasOlder = (s: ConvState): boolean => s.start > 0;

export const lastIndex = (s: ConvState): number => (s.items.length ? s.start + s.items.length - 1 : 0);
