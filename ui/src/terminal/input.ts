import { copyText, pasteFromClipboard, rightClickAction, systemClipboard, type ClipboardLike } from "./clipboard";
import { keyAction, wheelFontAction, type FontAction } from "./keys";

/** The part of xterm's Terminal the input wiring needs. */
export interface InputTerm {
  hasSelection(): boolean;
  getSelection(): string;
  clearSelection(): void;
  paste(data: string): void;
  attachCustomKeyEventHandler(handler: (ev: KeyboardEvent) => boolean): void;
}

export interface InputHooks {
  onSearch(): void;
  onFont(action: FontAction): void;
}

function copySelection(term: InputTerm, clip: ClipboardLike | undefined): void {
  const text = term.getSelection();
  term.clearSelection();
  void copyText(text, clip);
}

/**
 * Windows Terminal style clipboard, search and font-size keys. `host` is the
 * element xterm was opened in. Returns a function that removes the DOM
 * listeners (the key handler is replaced on the next attach).
 */
export function attachInput(
  term: InputTerm,
  host: HTMLElement,
  hooks: InputHooks,
  clip: ClipboardLike | undefined = systemClipboard(),
): () => void {
  term.attachCustomKeyEventHandler((ev) => {
    const action = keyAction(ev, term.hasSelection());
    switch (action) {
      case "pass":
        return true;
      case "paste":
        // No preventDefault: the browser fires `paste`, xterm pastes it.
        return false;
      case "copy":
        ev.preventDefault();
        copySelection(term, clip);
        return false;
      case "search":
        ev.preventDefault();
        hooks.onSearch();
        return false;
      case "swallow":
        if (ev.type === "keydown") ev.preventDefault();
        return false;
      default:
        // Also keeps WebView2 from zooming the page on Ctrl+= / Ctrl+-.
        ev.preventDefault();
        hooks.onFont(action);
        return false;
    }
  });

  const onWheel = (ev: WheelEvent) => {
    const action = wheelFontAction(ev);
    if (action === null) return;
    ev.preventDefault();
    ev.stopPropagation();
    if (action !== "swallow") hooks.onFont(action);
  };
  const onContextMenu = (ev: MouseEvent) => {
    ev.preventDefault();
    if (rightClickAction(term.hasSelection()) === "copy") copySelection(term, clip);
    else void pasteFromClipboard(term, clip);
  };
  // Right button belongs to copy/paste. While a TUI has mouse tracking on,
  // xterm would also report the press to the app, which may paste on it
  // too (one click, two pastes). Keep the press and release away from xterm.
  const onRightButton = (ev: MouseEvent) => {
    if (ev.button === 2) ev.stopPropagation();
  };
  host.addEventListener("wheel", onWheel, { capture: true, passive: false });
  host.addEventListener("contextmenu", onContextMenu, true);
  host.addEventListener("mousedown", onRightButton, true);
  host.addEventListener("mouseup", onRightButton, true);
  return () => {
    host.removeEventListener("wheel", onWheel, true);
    host.removeEventListener("contextmenu", onContextMenu, true);
    host.removeEventListener("mousedown", onRightButton, true);
    host.removeEventListener("mouseup", onRightButton, true);
  };
}
