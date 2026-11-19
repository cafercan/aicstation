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
