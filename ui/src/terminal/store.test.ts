import { beforeEach, expect, test, vi } from "vitest";

const { invoke, FakeChannel } = vi.hoisted(() => {
  class FakeChannel<T> {
    static all: FakeChannel<unknown>[] = [];
    onmessage: (m: T) => void = () => {};
    constructor() { FakeChannel.all.push(this as FakeChannel<unknown>); }
  }
  return { invoke: vi.fn(), FakeChannel };
});
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...a: unknown[]) => invoke(...a),
  Channel: FakeChannel,
}));
const writes: unknown[] = [];
let dataCb: (d: string) => void = () => {};
vi.mock("@xterm/xterm", () => ({
  Terminal: class {
    cols = 80; rows = 24;
    constructor(public opts: unknown) {}
    onData(cb: (d: string) => void) { dataCb = cb; return { dispose() {} }; }
    write(d: unknown) { writes.push(d); }
    dispose() {}
    loadAddon() {}
    open() {}
  },
}));
vi.mock("@xterm/addon-fit", () => ({ FitAddon: class { fit() {} } }));

import { ubuntuTheme, terminalFont } from "./theme";
import {
  confirmClose, getTerminals, openTerminal, resetTerminals, terminalForPid, terminalName,
} from "./store";
import type { SessionCard } from "../api";

function card(over: Partial<SessionCard>): SessionCard {
  return {
    sessionId: "s", pid: 1, parentPid: null, name: "demo", cwd: "C:/x", status: "idle",
    waitingFor: null, model: null, contextPct: null, costUsd: 0, agentsTotal: 0,
    agentsFinished: 0, lastTool: null, startedAtMs: 0, lastActivityMs: 0, gitBranch: null, ...over,
  };
}

beforeEach(() => {
  resetTerminals();
  FakeChannel.all = [];
  writes.length = 0;
  invoke.mockReset();
  invoke.mockImplementation(async (cmd: string) => (cmd === "pty_spawn" ? { id: 7, pid: 4242 } : undefined));
});

test("tema spec değerleriyle birebir", () => {
  expect(ubuntuTheme).toEqual({
    background: "#300A24", foreground: "#EEEEEC", cursor: "#FFFFFF", selectionBackground: "#B5D5FF",
    black: "#2E3436", red: "#CC0000", green: "#2DC94E", yellow: "#C4A000",
    blue: "#4C93FF", magenta: "#75507B", cyan: "#06989A", white: "#D3D7CF",
    brightBlack: "#555753", brightRed: "#EF2929", brightGreen: "#6BEE85", brightYellow: "#FCE94F",
    brightBlue: "#8FBEFF", brightMagenta: "#AD7FA8", brightCyan: "#34E2E2", brightWhite: "#EEEEEC",
  });
  expect(Object.keys(ubuntuTheme)).toHaveLength(20);
  expect(terminalFont).toEqual({ fontFamily: "Consolas", fontSize: 13 });
});

test("terminal adı: içindeki oturumun adı, yoksa \"PowerShell N\"", async () => {
  const t = await openTerminal();
  expect(terminalName(t, [])).toBe("PowerShell 1");
  expect(terminalName(t, [card({ parentPid: 4242, name: "projem" })])).toBe("projem");
  expect(terminalName(t, [card({ parentPid: 1, name: "başka" })])).toBe("PowerShell 1");
});

test("terminalForPid: oturumun parentPid değeri bir PTY pid ise o terminal id", async () => {
  await openTerminal();
  expect(terminalForPid(4242)).toBe(7);
  expect(terminalForPid(1)).toBeUndefined();
  expect(terminalForPid(null)).toBeUndefined();
});

test("spawn: ESC[6n yanıtı pty_write ile iletilir, çıktı bayt olarak yazılır", async () => {
  await openTerminal();
  expect(invoke).toHaveBeenCalledWith("pty_spawn", expect.objectContaining({ cols: 80, rows: 24 }));
  FakeChannel.all[0].onmessage([104, 105] as never);
  expect(writes[0]).toBeInstanceOf(Uint8Array);
  dataCb("\x1b[1;1R");
  expect(invoke).toHaveBeenCalledWith("pty_write", { id: 7, data: "\x1b[1;1R" });
});

test("kapatma: içinde Busy/Waiting oturum varsa onay ister", async () => {
  await openTerminal();
  const ask = vi.spyOn(window, "confirm").mockReturnValue(false);
  expect(confirmClose([7], [card({ parentPid: 4242, status: "idle" })])).toBe(true);
  expect(ask).not.toHaveBeenCalled();
  expect(confirmClose([7], [card({ parentPid: 4242, status: "busy" })])).toBe(false);
  expect(confirmClose([7], [card({ parentPid: 4242, status: "waiting" })])).toBe(false);
  expect(ask).toHaveBeenCalledTimes(2);
  ask.mockReturnValue(true);
  expect(confirmClose([7], [card({ parentPid: 4242, status: "busy" })])).toBe(true);
});

test("süreç çıkınca terminal menüden düşer", async () => {
  await openTerminal();
  expect(getTerminals()).toHaveLength(1);
  FakeChannel.all[1].onmessage(0 as never);
  expect(getTerminals()).toHaveLength(0);
  expect(terminalForPid(4242)).toBeUndefined();
});
