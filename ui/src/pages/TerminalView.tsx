import { useEffect, useRef } from "react";
import "@xterm/xterm/css/xterm.css";
import { resizeTerminal, type TerminalEntry } from "../terminal/store";
import { ubuntuTheme } from "../terminal/theme";

interface Props { entry: TerminalEntry; visible: boolean }

/** Stays mounted while hidden so output and scrollback are kept. */
export default function TerminalView({ entry, visible }: Props) {
  const host = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = host.current;
    if (!el) return;
    entry.term.open(el);
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver(() => { if (el.offsetParent !== null) resizeTerminal(entry); });
    ro.observe(el);
    return () => ro.disconnect();
  }, [entry]);

  useEffect(() => {
    if (visible) { resizeTerminal(entry); entry.term.focus(); }
  }, [visible, entry]);

  return (
    <div className="term-host" ref={host}
      style={{ display: visible ? "block" : "none", background: ubuntuTheme.background }} />
  );
}
