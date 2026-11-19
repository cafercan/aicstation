import { useEffect, useState, useSyncExternalStore } from "react";
import type { SessionCard } from "../api";
import { fmtElapsed, fmtTokens, statusColor, statusKind, statusLabel } from "../format";
import { useT, type Key } from "../i18n";
import { getTerminals, subscribe, terminalForPid } from "../terminal/store";
import { findModeLine, findSpinner, screenRows } from "./statusLine";

/** The app terminal's screen is read at most this often. */
const SCREEN_MS = 250;

const MODES: Record<string, Key> = {
  auto: "conv.mode.auto",
  acceptEdits: "conv.mode.acceptEdits",
  plan: "conv.mode.plan",
  bypassPermissions: "conv.mode.bypassPermissions",
};

type Screen = { spinner: string | null; mode: string | null };
const NONE: Screen = { spinner: null, mode: null };

/**
 * Claude's spinner and footer mode line on the screen of the app terminal
 * the session runs in; nothing for a session outside the app. Re-read as the
 * terminal writes, throttled, with a trailing read so the last state shows.
 */
function useScreen(parentPid: number | null): Screen {
  const terminals = useSyncExternalStore(subscribe, getTerminals);
  const id = terminalForPid(parentPid);
  const term = terminals.find((t) => t.id === id)?.term;
  const [screen, setScreen] = useState<Screen>(NONE);
  useEffect(() => {
    if (!term) { setScreen(NONE); return; }
    const read = () => {
      const rows = screenRows(term);
      setScreen({ spinner: findSpinner(rows), mode: findModeLine(rows) });
    };
    read();
    let timer: number | undefined;
    let last = Date.now();
    const sub = term.onWriteParsed(() => {
      if (timer !== undefined) return;
      const wait = Math.max(0, SCREEN_MS - (Date.now() - last));
      timer = window.setTimeout(() => { timer = undefined; last = Date.now(); read(); }, wait);
    });
    return () => { sub.dispose(); if (timer !== undefined) window.clearTimeout(timer); };
  }, [term]);
  return screen;
}

/** Wall clock, ticking every second while `on`. */
function useNow(on: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!on) return;
    setNow(Date.now());
    const h = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(h);
  }, [on]);
  return now;
}

/**
 * One line above the message box saying what a live session is doing: for
 * a session in an app terminal Claude's own spinner and mode line from its
 * screen, else (and for the rest) state, elapsed time, turn tokens, mode,
 * background shells, queued messages and running agents from the card.
 */
export default function ConvStatus({ card }: { card: SessionCard }) {
  const t = useT();
  const screen = useScreen(card.parentPid);
  const kind = statusKind(card.status, card.pid);
  // Tokens belong to a turn in progress; "shell" also covers a background
  // command left running after the turn, whose tokens are the finished turn's.
  const working = card.status === "busy";
  const now = useNow(card.status !== "idle");

  // The mode rarely changes, so it leads; what changes follows it. The
  // screen's mode row ("⏵⏵ auto mode on · 1 shell") is split the same way.
  const parts: string[] = [];
  const footer = screen.mode ? screen.mode.split(" · ") : null;
  if (footer) parts.push(footer[0]);
  else {
    const mode = card.permissionMode;
    if (mode && mode !== "default") parts.push(MODES[mode] ? t(MODES[mode]) : `⏵⏵ ${mode}`);
  }
  // A spinner drawn with "·" would read "· ·" after the separator.
  if (screen.spinner) parts.push(screen.spinner.replace(/^·\s+/, ""));
  else {
    let state = statusLabel(card.status, card.pid);
    if (card.status !== "idle" && card.statusSinceMs > 0) state += ` · ${fmtElapsed(now - card.statusSinceMs)}`;
    if (working && card.turnOutputTokens > 0) state += ` · ${t("conv.turnTokens", { n: fmtTokens(card.turnOutputTokens) })}`;
    parts.push(state);
  }
  if (footer) parts.push(...footer.slice(1));
  else if (card.backgroundShells > 0) parts.push(t("conv.bgShells", { n: card.backgroundShells }));
  if (card.queuedMessages > 0) parts.push(t("conv.queued", { n: card.queuedMessages }));
  if (card.agentsRunning > 0) parts.push(t("card.agentsRunning", { n: card.agentsRunning }));
  const text = parts.join(" · ");

  return (
    <div className="conv-status-line" title={text}>
      <span className="dot" aria-hidden="true" style={{ background: statusColor(kind) }} />
      <span className="text">{text}</span>
    </div>
  );
}
