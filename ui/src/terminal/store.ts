import { Channel, invoke } from "@tauri-apps/api/core";
import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import { useSyncExternalStore } from "react";
import type { SessionCard } from "../api";
import { ubuntuTheme, terminalFont } from "./theme";

export interface TerminalEntry {
  id: number;
  pid: number;
  /** 1-based creation counter, used for the "PowerShell N" fallback name. */
  n: number;
  term: Terminal;
  fit: FitAddon;
}

interface PtyInfo { id: number; pid: number }

let terminals: TerminalEntry[] = [];
let counter = 0;
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());

export function getTerminals(): TerminalEntry[] { return terminals; }
export function subscribe(l: () => void): () => void {
  listeners.add(l);
  return () => { listeners.delete(l); };
}
export function useTerminals(): TerminalEntry[] {
  return useSyncExternalStore(subscribe, getTerminals);
}

/** Test helper: forget all terminals without touching the backend. */
export function resetTerminals(): void {
  for (const t of terminals) t.term.dispose();
  terminals = [];
  counter = 0;
  emit();
}

/** Terminal id whose PTY pid equals the Claude session's parent pid. */
export function terminalForPid(parentPid: number | null | undefined): number | undefined {
  if (parentPid == null) return undefined;
  return terminals.find((t) => t.pid === parentPid)?.id;
}

export function sessionsIn(t: TerminalEntry, sessions: SessionCard[]): SessionCard[] {
  return sessions.filter((s) => s.parentPid === t.pid);
}

export function terminalName(t: TerminalEntry, sessions: SessionCard[]): string {
  return sessionsIn(t, sessions)[0]?.name || `PowerShell ${t.n}`;
}

function removeEntry(id: number): void {
  const t = terminals.find((x) => x.id === id);
  if (!t) return;
  terminals = terminals.filter((x) => x.id !== id);
  t.term.dispose();
  emit();
}

/** Starts a PowerShell PTY; the xterm instance lives here so it survives tab switches. */
/** OSC 8 links in terminal output never open a window or browser. */
const NO_LINKS = { activate: () => {} };

/** Spawns in flight; `killOrphans` must not kill a PTY whose id is not known yet. */
let spawning = 0;

export async function openTerminal(): Promise<TerminalEntry> {
  const term = new Terminal({
    ...terminalFont, theme: ubuntuTheme, cursorBlink: true, scrollback: 5000,
    linkHandler: NO_LINKS, windowsPty: { backend: "conpty" },
  });
  const fit = new FitAddon();
  term.loadAddon(fit);

  const onOutput = new Channel<number[]>();
  const onExit = new Channel<number>();
  onOutput.onmessage = (arr) => term.write(new Uint8Array(arr));
  let id = -1;
  let exited = false;
  onExit.onmessage = () => { exited = true; if (id >= 0) removeEntry(id); };

  // Wired before the spawn: ConPTY asks ESC[6n and powershell waits for the
  // reply, which xterm may produce before pty_spawn resolves. Data is queued
  // until the id is known, then sent in order.
  const queued: string[] = [];
  const send = (d: string) => { invoke("pty_write", { id, data: d }).catch(() => {}); };
  term.onData((d) => { if (id < 0) queued.push(d); else send(d); });

  let info: PtyInfo;
  spawning++;
  try {
    info = await invoke<PtyInfo>("pty_spawn", { cols: term.cols, rows: term.rows, onOutput, onExit });
  } catch (e) {
    term.dispose();
    throw e;
  } finally {
    spawning--;
  }
  id = info.id;
  for (const d of queued.splice(0)) send(d);

  const entry: TerminalEntry = { id, pid: info.pid, n: ++counter, term, fit };
  terminals = [...terminals, entry];
  emit();
  // The shell may have exited before pty_spawn resolved.
  if (exited) removeEntry(id);
  return entry;
}

export function resizeTerminal(t: TerminalEntry): void {
  try { t.fit.fit(); } catch { return; }
  invoke("pty_resize", { id: t.id, cols: t.term.cols, rows: t.term.rows }).catch(() => {});
}

export async function closeTerminal(id: number): Promise<void> {
  try { await invoke("pty_kill", { id }); } catch { /* already gone */ }
  removeEntry(id);
}

/** True when closing may proceed: no Busy/Waiting Claude inside, or the user agrees. */
export function confirmClose(ids: number[], sessions: SessionCard[]): boolean {
  const active = terminals
    .filter((t) => ids.includes(t.id))
    .flatMap((t) => sessionsIn(t, sessions))
    .filter((s) => s.status === "busy" || s.status === "waiting");
  if (active.length === 0) return true;
  return window.confirm(
    `Terminalde çalışan ${active.length} Claude oturumu var (${active.map((s) => s.name).join(", ")}). Yine de kapatılsın mı?`,
  );
}

export function allTerminalIds(): number[] { return terminals.map((t) => t.id); }

/**
 * After a webview reload the backend may still run PTYs this page has no
 * handle to; kill every PTY that is not one of ours. Skipped while a spawn is
 * in flight (its id is not known yet, so it would look like an orphan).
 */
export async function killOrphans(): Promise<void> {
  if (spawning > 0) return;
  let list: PtyInfo[];
  try { list = await invoke<PtyInfo[]>("pty_list"); } catch { return; }
  if (spawning > 0 || !Array.isArray(list)) return;
  const known = new Set(terminals.map((t) => t.id));
  await Promise.all(
    list.filter((p) => !known.has(p.id)).map((p) => invoke("pty_kill", { id: p.id }).catch(() => {})),
  );
}
