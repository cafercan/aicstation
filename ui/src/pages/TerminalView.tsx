import { useEffect, useRef, useState, type CSSProperties } from "react";
import "@xterm/xterm/css/xterm.css";
import type { Terminal } from "@xterm/xterm";
import { resizeTerminal, setFontSize, type TerminalEntry } from "../terminal/store";
import { ubuntuTheme } from "../terminal/theme";
import { attachInput } from "../terminal/input";
import { DEFAULT_FONT_SIZE, fontSizeAfter } from "../terminal/keys";
import { enableWebgl } from "../terminal/render";
import SearchBar from "../terminal/SearchBar";
import { registerSearchFocus } from "../shortcuts/bus";
import { throttle } from "../throttle";

interface Props {
  entry: TerminalEntry;
  visible: boolean;
  /** Box in percent of the terminal area when split; full size otherwise. */
  pane?: { left: number; width: number };
  /** The pane that gets keyboard input (always true when not split). */
  active?: boolean;
  /** Pointer or focus entered this pane. */
  onActivate?: () => void;
}

/** Terminals that already got a renderer decision (StrictMode runs effects twice). */
const gpuTried = new WeakSet<Terminal>();

/** fit + pty_resize at most this often while a split divider is dragged. */
const RESIZE_MS = 80;

/** Stays mounted while hidden so output and scrollback are kept. */
export default function TerminalView({ entry, visible, pane, active = true, onActivate }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const [searching, setSearching] = useState(false);

  useEffect(() => {
    const el = host.current;
    if (!el) return;
    entry.term.open(el);
    const detach = attachInput(entry.term, el, {
      onSearch: () => setSearching(true),
      onFont: (a) => { setFontSize(entry, fontSizeAfter(a, entry.term.options.fontSize ?? DEFAULT_FONT_SIZE)); },
    });
    const resize = throttle(() => { if (el.offsetParent !== null) resizeTerminal(entry); }, RESIZE_MS);
    const ro = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(resize);
    ro?.observe(el);
    return () => { detach(); ro?.disconnect(); resize.cancel(); };
  }, [entry]);

  useEffect(() => {
    if (!visible) return;
    // The WebGL renderer is created once the element has a real size.
    if (!gpuTried.has(entry.term)) { gpuTried.add(entry.term); enableWebgl(entry.term); }
    resizeTerminal(entry);
    if (active) entry.term.focus();
  }, [visible, active, entry]);

  // Ctrl+Shift+F on a terminal page opens this pane's search bar.
  useEffect(() => {
    if (!visible || !active) return;
    return registerSearchFocus(() => setSearching(true));
  }, [visible, active]);

  const style: CSSProperties = { display: visible ? "block" : "none", background: ubuntuTheme.background };
  if (pane) { style.left = `${pane.left}%`; style.width = `${pane.width}%`; style.right = "auto"; }
  const cls = `term-wrap${pane ? " split" : ""}${pane && active ? " active" : ""}`;
  return (
    <div className={cls} style={style} data-terminal={entry.id}
      onMouseDownCapture={onActivate} onFocusCapture={onActivate}>
      <div className="term-host" ref={host} />
      {searching && (
        <SearchBar search={entry.search} onClose={() => { setSearching(false); entry.term.focus(); }} />
      )}
    </div>
  );
}
