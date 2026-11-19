import type { KeyboardEvent, MouseEvent } from "react";

/**
 * Props that make a clickable element (a row, a list item) usable from the
 * keyboard too: it takes the focus with Tab, Enter and Space run `fn`, and
 * screen readers hear it as a button (a table row passes null and stays a row). The event does not
 * bubble on, so a
 * control inside a clickable row acts alone.
 */
export function pressable(fn: () => void, role: "button" | "link" | null = "button") {
  return {
    ...(role ? { role } : {}),
    tabIndex: 0,
    onClick: (e: MouseEvent) => { e.stopPropagation(); fn(); },
    onKeyDown: (e: KeyboardEvent) => {
      if (e.target !== e.currentTarget) return;
      if (e.key !== "Enter" && e.key !== " ") return;
      e.preventDefault();
      e.stopPropagation();
      fn();
    },
  };
}

const FOCUSABLE = 'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

/**
 * keyDown handler of a modal dialog: Tab and Shift+Tab go round its own
 * controls instead of reaching the page behind it.
 */
export function keepFocusIn(e: KeyboardEvent<HTMLElement>): void {
  if (e.key !== "Tab" || e.ctrlKey || e.altKey || e.metaKey) return;
  const items = [...e.currentTarget.querySelectorAll<HTMLElement>(FOCUSABLE)];
  e.preventDefault();
  if (items.length === 0) return;
  const at = items.indexOf(document.activeElement as HTMLElement);
  const next = e.shiftKey ? (at <= 0 ? items.length - 1 : at - 1) : (at + 1) % items.length;
  items[next].focus();
}
