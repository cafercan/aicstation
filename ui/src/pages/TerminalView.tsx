import { useEffect, useRef, useState } from "react";
import "@xterm/xterm/css/xterm.css";
import type { Terminal } from "@xterm/xterm";
import { resizeTerminal, setFontSize, type TerminalEntry } from "../terminal/store";
import { ubuntuTheme } from "../terminal/theme";
import { attachInput } from "../terminal/input";
import { DEFAULT_FONT_SIZE, fontSizeAfter } from "../terminal/keys";
import { enableWebgl } from "../terminal/render";
import SearchBar from "../terminal/SearchBar";

interface Props { entry: TerminalEntry; visible: boolean }

/** Terminals that already got a renderer decision (StrictMode runs effects twice). */
const gpuTried = new WeakSet<Terminal>();

/** Stays mounted while hidden so output and scrollback are kept. */
export default function TerminalView({ entry, visible }: Props) {
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
    const ro = typeof ResizeObserver === "undefined" ? null
      : new ResizeObserver(() => { if (el.offsetParent !== null) resizeTerminal(entry); });
    ro?.observe(el);
    return () => { detach(); ro?.disconnect(); };
  }, [entry]);

  useEffect(() => {
    if (!visible) return;
    // The WebGL renderer is created once the element has a real size.
    if (!gpuTried.has(entry.term)) { gpuTried.add(entry.term); enableWebgl(entry.term); }
    resizeTerminal(entry);
    entry.term.focus();
  }, [visible, entry]);

  return (
    <div className="term-wrap" style={{ display: visible ? "block" : "none", background: ubuntuTheme.background }}>
      <div className="term-host" ref={host} />
      {searching && (
        <SearchBar search={entry.search} onClose={() => { setSearching(false); entry.term.focus(); }} />
      )}
    </div>
  );
}
