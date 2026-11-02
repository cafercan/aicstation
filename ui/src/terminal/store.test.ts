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
    addons: unknown[] = [];
    unicode = { activeVersion: "6" };
    options: Record<string, unknown>;
    constructor(public opts: Record<string, unknown>) { this.options = { ...opts }; }
    onData(cb: (d: string) => void) { dataCb = cb; return { dispose() {} }; }
    write(d: unknown) { writes.push(d); }
    dispose() {}
    loadAddon(a: unknown) { this.addons.push(a); }
    open() {}
  },
}));
vi.mock("@xterm/addon-fit", () => ({ FitAddon: class { fit() {} } }));
vi.mock("@xterm/addon-unicode11", () => ({ Unicode11Addon: class { kind = "unicode11"; } }));
vi.mock("@xterm/addon-search", () => ({ SearchAddon: class { kind = "search"; } }));

import { ubuntuTheme, terminalFont, renderOptions } from "./theme";
import {
  confirmClose, getTerminals, killOrphans, openResumeTerminal, openTerminal, resetTerminals, setFontSize, terminalForPid, terminalName,
} from "./store";
import type { SessionCard } from "../api";
import { setLang } from "../i18n";

function card(over: Partial<SessionCard>): SessionCard {
  return {
    sessionId: "s", pid: 1, parentPid: null, name: "demo", cwd: "C:/x", status: "idle",
    waitingFor: null, model: null, contextPct: null, costUsd: 0, agentsTotal: 0,
    agentsFinished: 0, toolOk: 0, toolErr: 0, lastTool: null, startedAtMs: 0, lastActivityMs: 0, gitBranch: null, hostKind: "other", ...over,
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
  expect(terminalFont).toEqual({
    fontFamily: '"Cascadia Mono", "Cascadia Code", Consolas, monospace', fontSize: 13,
    fontWeight: 350, fontWeightBold: "bold",
  });
});

test("render ayarları: hücre ızgarası sıkı, renkler değiştirilmez", () => {
  expect(renderOptions).toEqual({
    customGlyphs: true, lineHeight: 1, letterSpacing: 0,
    drawBoldTextInBrightColors: false, minimumContrastRatio: 1, allowTransparency: false,
  });
});

test("xterm seçenekleri: 10.000 satır geçmiş, unicode 11 genişlikleri, arama eklentisi", async () => {
  const t = await openTerminal();
  const term = t.term as unknown as {
    opts: Record<string, unknown>; unicode: { activeVersion: string }; addons: { kind?: string }[];
  };
  expect(term.opts).toMatchObject({
    ...terminalFont, ...renderOptions, theme: ubuntuTheme, scrollback: 10000, allowProposedApi: true,
  });
  expect(term.unicode.activeVersion).toBe("11");
  expect(term.addons.map((a) => a.kind).filter(Boolean)).toEqual(["unicode11", "search"]);
  expect((t.search as unknown as { kind: string }).kind).toBe("search");
});

test("setFontSize: 8–32 aralığında uygular, yeniden sığdırır ve pty_resize gönderir", async () => {
  const t = await openTerminal();
  const opts = (t.term as unknown as { options: { fontSize: number } }).options;
  expect(setFontSize(t, 40)).toBe(32);
  expect(opts.fontSize).toBe(32);
  expect(invoke).toHaveBeenCalledWith("pty_resize", { id: 7, cols: 80, rows: 24 });
  expect(setFontSize(t, 2)).toBe(8);
  expect(opts.fontSize).toBe(8);
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

test("kapatma: içinde herhangi bir Claude oturumu varsa onay ister", async () => {
  await openTerminal();
  const ask = vi.spyOn(window, "confirm").mockReturnValue(false);
  expect(confirmClose([7], [])).toBe(true);
  expect(confirmClose([7], [card({ parentPid: 9999, status: "busy" })])).toBe(true);
  expect(ask).not.toHaveBeenCalled();
  expect(confirmClose([7], [card({ parentPid: 4242, status: "idle" })])).toBe(false);
  expect(confirmClose([7], [card({ parentPid: 4242, status: "busy" })])).toBe(false);
  expect(confirmClose([7], [card({ parentPid: 4242, status: "waiting" })])).toBe(false);
  expect(ask).toHaveBeenCalledTimes(3);
  ask.mockReturnValue(true);
  expect(confirmClose([7], [card({ parentPid: 4242, status: "busy" })])).toBe(true);
  expect(ask).toHaveBeenLastCalledWith("Terminalde açık 1 Claude oturumu var (demo). Yine de kapatılsın mı?");
  setLang("en");
  confirmClose([7], [card({ parentPid: 4242 }), card({ parentPid: 4242, name: "iki" })]);
  expect(ask).toHaveBeenLastCalledWith("The terminal has 2 open Claude sessions (demo, iki). Close it anyway?");
});

test("süreç çıkınca terminal menüden düşer", async () => {
  await openTerminal();
  expect(getTerminals()).toHaveLength(1);
  FakeChannel.all[1].onmessage(0 as never);
  expect(getTerminals()).toHaveLength(0);
  expect(terminalForPid(4242)).toBeUndefined();
});

test("spawn çözülmeden gelen klavye/yanıt verisi kaybolmaz, sırayla iletilir", async () => {
  invoke.mockImplementation(async (cmd: string) => {
    if (cmd !== "pty_spawn") return undefined;
    // ConPTY's ESC[6n is answered by xterm before pty_spawn resolves.
    dataCb("\x1b[1;1R");
    dataCb("a");
    return { id: 9, pid: 55 };
  });
  await openTerminal();
  dataCb("b");
  const w = invoke.mock.calls.filter((c) => c[0] === "pty_write").map((c) => c[1]);
  expect(w).toEqual([
    { id: 9, data: "\x1b[1;1R" }, { id: 9, data: "a" }, { id: 9, data: "b" },
  ]);
});

test("xterm: bağlantılar pencere açmaz, ConPTY arka ucu bildirilir", async () => {
  const open = vi.spyOn(window, "open").mockReturnValue(null);
  const t = await openTerminal();
  const opts = (t.term as unknown as { opts: Record<string, unknown> }).opts;
  expect(opts.windowsPty).toEqual({ backend: "conpty" });
  const lh = opts.linkHandler as { activate: (e: unknown, text: string) => void };
  expect(typeof lh.activate).toBe("function");
  lh.activate(new MouseEvent("click"), "https://example.invalid/");
  expect(open).not.toHaveBeenCalled();
  open.mockRestore();
});

test("yeniden yüklemede sahipsiz PTY'ler kapatılır, bilinenler kalır", async () => {
  await openTerminal(); // id 7 is ours
  invoke.mockImplementation(async (cmd: string) =>
    cmd === "pty_list" ? [{ id: 3, pid: 30 }, { id: 7, pid: 4242 }, { id: 5, pid: 50 }] : undefined);
  await killOrphans();
  const kills = invoke.mock.calls.filter((c) => c[0] === "pty_kill").map((c) => c[1]);
  expect(kills).toEqual([{ id: 3 }, { id: 5 }]);
});

test("exit_before_spawn_resolves_removes_terminal", async () => {
  invoke.mockImplementation(async (cmd: string) => {
    if (cmd !== "pty_spawn") return undefined;
    FakeChannel.all[1].onmessage(0 as never); // shell exits before spawn resolves
    return { id: 9, pid: 55 };
  });
  await openTerminal();
  expect(getTerminals()).toHaveLength(0);
  expect(terminalForPid(55)).toBeUndefined();
});

test("devam terminali: yalnız oturum kimliği gönderilir, ad oturum adı, aynı yaşam döngüsü", async () => {
  invoke.mockImplementation(async (cmd: string) => {
    if (cmd !== "pty_spawn_resume") return undefined;
    dataCb("\x1b[1;1R"); // answered before the spawn resolves: queued
    return { id: 11, pid: 99 };
  });
  const t = await openResumeTerminal("0f864996-1f75-464e-b101-a3ceed5d79d9", "projem");
  const call = invoke.mock.calls.find((c) => c[0] === "pty_spawn_resume")!;
  expect(Object.keys(call[1] as object).sort()).toEqual(["cols", "onExit", "onOutput", "rows", "sessionId"]);
  expect(call[1]).toMatchObject({ sessionId: "0f864996-1f75-464e-b101-a3ceed5d79d9", cols: 80, rows: 24 });
  expect(invoke).toHaveBeenCalledWith("pty_write", { id: 11, data: "\x1b[1;1R" });
  expect(getTerminals().map((x) => x.id)).toEqual([11]);
  expect(terminalName(t, [])).toBe("projem");
  // Claude inside reports its own name once it is live.
  expect(terminalName(t, [card({ parentPid: 99, name: "canli" })])).toBe("canli");
  const term = t.term as unknown as { opts: Record<string, unknown>; addons: { kind?: string }[] };
  expect(term.opts).toMatchObject({ theme: ubuntuTheme, scrollback: 10000 });
  expect(term.addons.map((a) => a.kind).filter(Boolean)).toEqual(["unicode11", "search"]);
  FakeChannel.all[1].onmessage(0 as never); // exit
  expect(getTerminals()).toHaveLength(0);
});

test("devam terminali: ad yoksa \"Devam: <kısa id>\"; hata atılır ve terminal eklenmez", async () => {
  invoke.mockImplementation(async (cmd: string) => (cmd === "pty_spawn_resume" ? { id: 12, pid: 5 } : undefined));
  const t = await openResumeTerminal("0f864996-1f75-464e-b101-a3ceed5d79d9");
  expect(terminalName(t, [])).toBe("Devam: 0f864996");
  invoke.mockImplementation(async (cmd: string) => {
    if (cmd === "pty_spawn_resume") throw "session_live";
    return undefined;
  });
  await expect(openResumeTerminal("0f864996-1f75-464e-b101-a3ceed5d79d9", "x")).rejects.toBe("session_live");
  expect(getTerminals().map((x) => x.id)).toEqual([12]);
});
