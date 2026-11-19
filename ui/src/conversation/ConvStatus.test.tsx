import { act, render } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import type { SessionCard } from "../api";

/** A stand-in app terminal whose screen is `m.screen`. */
const m = vi.hoisted(() => {
  const state = {
    screen: [] as string[],
    disposed: 0,
    terminals: [] as { id: number; pid: number; term: unknown }[],
    listeners: new Set<() => void>(),
  };
  const term = {
    get rows() { return state.screen.length; },
    buffer: { active: { baseY: 0, getLine: (i: number) => ({ translateToString: () => state.screen[i] ?? "" }) } },
    onWriteParsed: () => ({ dispose: () => { state.disposed++; } }),
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
  expect(line()).toBe("çalışıyor · 46 sn · ↓ 3,9K token · ⏵⏵ otomatik mod · 1 arka plan komutu · 2 mesaj kuyrukta · 1 ajan çalışıyor");
  act(() => { vi.advanceTimersByTime(1000); });
  expect(line()).toContain("47 sn");
});

test("idle: no elapsed, default mode and zero counts left out", () => {
  show(card({ status: "idle", permissionMode: "default" }));
  expect(line()).toBe("hazır");
});

test("other modes and an unknown one", () => {
  show(card({ status: "idle", permissionMode: "plan" }));
  expect(line()).toBe("hazır · ⏸ plan modu");
  show(card({ status: "idle", permissionMode: "dontAsk" }));
  expect(line()).toBe("hazır · ⏵⏵ dontAsk");
});

test("app terminal: spinner and mode line from the screen replace card parts", () => {
  m.state.terminals = [{ id: 3, pid: 70, term: m.term }];
  m.state.screen = ["* Fermenting… (33s · ↓ 2.8k tokens)", "> ", "⏵⏵ auto mode on · 1 shell · ↓ to manage"];
  show(card({ status: "busy", statusSinceMs: Date.now(), permissionMode: "plan", backgroundShells: 3, queuedMessages: 1 }));
  expect(line()).toBe("* Fermenting… (33s · ↓ 2.8k tokens) · ⏵⏵ auto mode on · 1 shell · 1 mesaj kuyrukta");
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
