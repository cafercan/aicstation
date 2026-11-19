import { act, render } from "@testing-library/react";
import { Profiler } from "react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import type { SessionCard } from "../api";

/** A stand-in app terminal whose screen is `m.screen`. */
const m = vi.hoisted(() => {
  const state = {
    screen: [] as string[],
    disposed: 0,
    /** The terminal's write listener (onWriteParsed). */
    write: null as (() => void) | null,
    terminals: [] as { id: number; pid: number; term: unknown }[],
    listeners: new Set<() => void>(),
  };
  const term = {
    get rows() { return state.screen.length; },
    buffer: { active: { baseY: 0, getLine: (i: number) => ({ translateToString: () => state.screen[i] ?? "" }) } },
    onWriteParsed: (cb: () => void) => { state.write = cb; return { dispose: () => { state.disposed++; state.write = null; } }; },
  };
  return { state, term };
});

vi.mock("../terminal/store", () => ({
  terminalForPid: (p: number | null) => m.state.terminals.find((t) => t.pid === p)?.id,
  getTerminals: () => m.state.terminals,
  subscribe: (l: () => void) => { m.state.listeners.add(l); return () => { m.state.listeners.delete(l); }; },
}));

import ConvStatus from "./ConvStatus";

function card(over: Partial<SessionCard> = {}): SessionCard {
  return {
    sessionId: "s1", pid: 7, parentPid: 70, name: "demo", cwd: "C:/x/demo", status: "idle", waitingFor: null,
    model: null, contextPct: null, costUsd: 0, tokens: { input: 0, output: 0, cacheRead: 0, cacheCreation: 0, cacheCreation1h: 0 },
    agentsTotal: 0, agentsFinished: 0, toolOk: 0, toolErr: 0, lastTool: null, startedAtMs: 0, lastActivityMs: 1,
    transcriptBytes: 0, permissionMode: null, backgroundShells: 0, queuedMessages: 0, turnOutputTokens: 0, statusSinceMs: 0,
    gitBranch: null, hostKind: "embedded", movable: true, agentsRunning: 0, remoteControl: false, ...over,
  };
}

let container: HTMLElement;
const line = () => (container.querySelector(".conv-status-line")?.textContent ?? "").replace(/\s+/g, " ").trim();
const show = (c: SessionCard) => { container = render(<ConvStatus card={c} />).container; };
const emitStore = () => m.state.listeners.forEach((l) => l());

beforeEach(() => {
  m.state.screen = [];
  m.state.disposed = 0;
  m.state.terminals = [];
});
afterEach(() => { vi.useRealTimers(); });

test("busy from card data: label, elapsed, turn tokens, mode, shells, queue, agents", () => {
  vi.useFakeTimers();
  vi.setSystemTime(100_000);
  show(card({ status: "busy", statusSinceMs: 54_000, turnOutputTokens: 3900,
    permissionMode: "auto", backgroundShells: 1, queuedMessages: 2, agentsRunning: 1 }));
  expect(line()).toBe("⏵⏵ otomatik mod · çalışıyor · 46 sn · ↓ 3,9K token · 1 arka plan komutu · 2 mesaj kuyrukta · 1 ajan çalışıyor");
  act(() => { vi.advanceTimersByTime(1000); });
  expect(line()).toContain("47 sn");
});

test("running a command after the turn: elapsed, but not the finished turn's tokens", () => {
  vi.useFakeTimers();
  vi.setSystemTime(100_000);
  show(card({ status: "shell", statusSinceMs: 90_000, turnOutputTokens: 123, backgroundShells: 1 }));
  expect(line()).toBe("komut çalıştırıyor · 10 sn · 1 arka plan komutu");
});

test("idle: no elapsed, default mode and zero counts left out", () => {
  show(card({ status: "idle", permissionMode: "default" }));
  expect(line()).toBe("hazır");
});

test("other modes and an unknown one", () => {
  show(card({ status: "idle", permissionMode: "plan" }));
  expect(line()).toBe("⏸ plan modu · hazır");
  show(card({ status: "idle", permissionMode: "dontAsk" }));
  expect(line()).toBe("⏵⏵ dontAsk · hazır");
});

test("app terminal: spinner and mode line from the screen replace card parts", () => {
  m.state.terminals = [{ id: 3, pid: 70, term: m.term }];
  m.state.screen = ["* Fermenting… (33s · ↓ 2.8k tokens)", "─".repeat(40), "> ", "─".repeat(40),
    "⏵⏵ auto mode on · 1 shell · ↓ to manage"];
  show(card({ status: "busy", statusSinceMs: Date.now(), permissionMode: "plan", backgroundShells: 3, queuedMessages: 1 }));
  expect(line()).toBe("⏵⏵ auto mode on · * Fermenting… (33s · ↓ 2.8k tokens) · 1 shell · 1 mesaj kuyrukta");
});

test("a spinner drawn with a middle dot gets no second dot before it", () => {
  m.state.terminals = [{ id: 3, pid: 70, term: m.term }];
  m.state.screen = ["· Pollinating… (5s · ↓ 130 tokens)", "─".repeat(40), "> ", "─".repeat(40), "⏵⏵ auto mode on"];
  show(card({ status: "busy", statusSinceMs: Date.now() }));
  expect(line()).toBe("⏵⏵ auto mode on · Pollinating… (5s · ↓ 130 tokens)");
});

test("the screen is read at most every 250 ms, and once more after the last write", () => {
  vi.useFakeTimers();
  m.state.terminals = [{ id: 3, pid: 70, term: m.term }];
  m.state.screen = ["* Fermenting… (1s · ↓ 10 tokens)"];
  show(card({ status: "busy", statusSinceMs: Date.now() }));
  expect(line()).toMatch(/\(1s/);
  m.state.screen = ["* Fermenting… (2s · ↓ 20 tokens)"];
  act(() => { m.state.write!(); });
  expect(line()).toMatch(/\(1s/);
  m.state.screen = ["* Fermenting… (3s · ↓ 30 tokens)"];
  act(() => { m.state.write!(); vi.advanceTimersByTime(100); });
  expect(line()).toMatch(/\(1s/);
  // The trailing read shows the last state of the burst.
  act(() => { vi.advanceTimersByTime(200); });
  expect(line()).toMatch(/\(3s/);
});

test("an unchanged screen does not redraw the line", () => {
  vi.useFakeTimers();
  m.state.terminals = [{ id: 3, pid: 70, term: m.term }];
  m.state.screen = ["* Fermenting… (1s · ↓ 10 tokens)"];
  const renders = vi.fn();
  render(<Profiler id="s" onRender={renders}><ConvStatus card={card({ status: "busy", statusSinceMs: 0 })} /></Profiler>);
  const before = renders.mock.calls.length;
  for (let i = 0; i < 3; i++) act(() => { m.state.write!(); vi.advanceTimersByTime(300); });
  expect(renders.mock.calls.length).toBe(before);
});

test("falls_back_when_the_terminal_goes_away", () => {
  m.state.terminals = [{ id: 3, pid: 70, term: m.term }];
  m.state.screen = ["* Fermenting… (3s · ↓ 10 tokens)"];
  show(card({ status: "busy", statusSinceMs: Date.now() }));
  expect(line()).toMatch(/^\* Fermenting/);
  act(() => { m.state.terminals = []; emitStore(); });
  expect(line()).toMatch(/^çalışıyor/);
  expect(m.state.disposed).toBe(1);
});
