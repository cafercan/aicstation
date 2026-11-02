import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn(async (cmd: string): Promise<unknown> => {
  if (cmd === "get_snapshot") {
    return {
      sessions: [{
        sessionId: "s-1", pid: 5, parentPid: null, name: "kalibrasyon", cwd: "C:\\p\\adc", status: "busy",
        waitingFor: null, model: null, contextPct: null, costUsd: 0, agentsTotal: 0, agentsFinished: 0, toolOk: 0,
        toolErr: 0, lastTool: null, startedAtMs: 0, lastActivityMs: 0, gitBranch: null, hostKind: "other",
      }],
      runningAgents: 0, waitingCount: 0,
    };
  }
  if (cmd === "get_session_detail") return null;
  if (cmd === "get_settings" || cmd === "set_settings") {
    return {
      notifyWaiting: true, notifyFinished: true, notifySubagent: false, notifySound: true, closeToTray: false,
      trayNoticeShown: false, lang: "tr",
    };
  }
  if (cmd === "set_focus_context") return null;
  return { rows: [], todayCost: 0, todayComplete: true, indexing: false };
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: class {} }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => { throw new Error("no tauri"); } }));

/** In-memory terminal store: no xterm, no PTY. */
const fake = vi.hoisted(() => {
  type E = { id: number; pid: number; n: number };
  let list: E[] = [];
  let next = 1;
  /** Claude session inside each terminal (by terminal id). */
  const inside = new Map<number, unknown>();
  const ls = new Set<() => void>();
  const emit = () => ls.forEach((l) => l());
  return {
    get: () => list,
    subscribe: (l: () => void) => { ls.add(l); return () => { ls.delete(l); }; },
    inside,
    reset: () => { list = []; next = 1; inside.clear(); emit(); },
    open: async () => { const e = { id: next, pid: 100 + next, n: next }; next++; list = [...list, e]; emit(); return e; },
    close: async (id: number) => { list = list.filter((e) => e.id !== id); emit(); },
  };
});
vi.mock("./terminal/store", async () => {
  const React = await import("react");
  return {
    useTerminals: () => React.useSyncExternalStore(fake.subscribe, fake.get),
    openTerminal: vi.fn(() => fake.open()),
    closeTerminal: vi.fn((id: number) => fake.close(id)),
    openResumeTerminal: vi.fn(),
    confirmClose: () => true,
    allTerminalIds: () => fake.get().map((e) => e.id),
    killOrphans: async () => {},
    sessionsIn: (t: { id: number }) => (fake.inside.has(t.id) ? [fake.inside.get(t.id)] : []),
    terminalName: (t: { n: number }) => `PowerShell ${t.n}`,
    terminalForPid: () => undefined,
  };
});
vi.mock("./pages/TerminalView", () => ({
  default: (p: { entry: { id: number }; visible: boolean; active: boolean; pane?: { width: number }; onActivate?: () => void }) => (
    <div data-testid={`term-${p.entry.id}`} data-visible={String(p.visible)} data-active={String(p.active)}
      data-width={p.pane ? String(p.pane.width) : "full"} onMouseDown={p.onActivate} />
  ),
}));

import App from "./App";

beforeEach(() => { fake.reset(); });

/** Fires a keydown on the focused element (or body); returns the event. */
function key(init: KeyboardEventInit, target: Element = document.activeElement ?? document.body): KeyboardEvent {
  const e = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init });
  act(() => { target.dispatchEvent(e); });
  return e;
}
const chord = (k: string, code: string, extra: KeyboardEventInit = {}) =>
  key({ key: k, code, ctrlKey: true, shiftKey: true, ...extra });
const vis = (id: number) => screen.getByTestId(`term-${id}`).dataset;

async function boot() {
  render(<App />);
  await screen.findByText("kalibrasyon");
}

test("Ctrl+Shift+H / D sayfa değiştirir; olay tüketilir ve hedefe ulaşmaz", async () => {
  await boot();
  const inner = vi.fn();
  document.body.addEventListener("keydown", inner);
  const e = chord("H", "KeyH");
  expect(e.defaultPrevented).toBe(true);
  expect(inner).not.toHaveBeenCalled();
  expect(await screen.findByText("Kayıt yok.")).toBeTruthy();
  chord("D", "KeyD");
  expect(await screen.findByText("kalibrasyon")).toBeTruthy();
  document.body.removeEventListener("keydown", inner);
});

test("akor olmayan tuşlar geçer: Ctrl+C, Ctrl+F, AltGr+7 dokunulmaz", async () => {
  await boot();
  const inner = vi.fn();
  document.body.addEventListener("keydown", inner);
  for (const init of [
    { key: "c", code: "KeyC", ctrlKey: true }, { key: "f", code: "KeyF", ctrlKey: true },
    { key: "{", code: "Digit7", ctrlKey: true, altKey: true }, { key: "a", code: "KeyA" },
  ]) {
    expect(key(init).defaultPrevented).toBe(false);
  }
  expect(inner).toHaveBeenCalledTimes(4);
  document.body.removeEventListener("keydown", inner);
});

test("Ctrl+Shift+T yeni terminal; Ctrl+Alt+N ve Ctrl+Tab geçiş; Ctrl+Shift+W kapatır", async () => {
  await boot();
  chord("T", "KeyT");
  await waitFor(() => expect(vis(1).visible).toBe("true"));
  chord("T", "KeyT");
  await waitFor(() => expect(vis(2).visible).toBe("true"));
  expect(vis(1).visible).toBe("false");
  key({ key: "1", code: "Digit1", ctrlKey: true, altKey: true });
  expect(vis(1).visible).toBe("true");
  // Ctrl+Alt+9 with no ninth terminal is not consumed.
  expect(key({ key: "9", code: "Digit9", ctrlKey: true, altKey: true }).defaultPrevented).toBe(false);
  key({ key: "Tab", code: "Tab", ctrlKey: true });
  expect(vis(2).visible).toBe("true");
  key({ key: "Tab", code: "Tab", ctrlKey: true });
  expect(vis(1).visible).toBe("true");
  key({ key: "Tab", code: "Tab", ctrlKey: true, shiftKey: true });
  expect(vis(2).visible).toBe("true");
  chord("W", "KeyW");
  await waitFor(() => expect(screen.queryByTestId("term-2")).toBeNull());
  expect(await screen.findByText("kalibrasyon")).toBeTruthy();
});

test("Böl: sağ bölme seçilir, iki bölme görünür, kenar çubuğu işaretler; Birleştir tek bölmeye döner", async () => {
  await boot();
  chord("T", "KeyT");
  chord("T", "KeyT");
  await waitFor(() => expect(vis(2).visible).toBe("true"));
  key({ key: "1", code: "Digit1", ctrlKey: true, altKey: true });
  fireEvent.click(screen.getByRole("button", { name: "◫ Böl" }));
  const pick = screen.getByLabelText("Sağ bölmede ne açılsın?");
  expect(within(pick).getAllByRole("option").map((o) => o.textContent))
    .toEqual(["Sağ bölmede ne açılsın?", "PowerShell 2", "＋ Yeni terminal"]);
  fireEvent.change(pick, { target: { value: "2" } });
  expect(vis(1)).toMatchObject({ visible: "true", active: "false", width: "50" });
  expect(vis(2)).toMatchObject({ visible: "true", active: "true", width: "50" });
  expect(screen.getByLabelText("sol bölmede görünüyor")).toBeTruthy();
  expect(screen.getByLabelText("sağ bölmede görünüyor")).toBeTruthy();
  // Clicking the left pane focuses it.
  fireEvent.mouseDown(screen.getByTestId("term-1"));
  expect(vis(1).active).toBe("true");
  // Divider: arrow keys move it, at most to 80%.
  const sep = screen.getByRole("separator");
  for (let i = 0; i < 10; i++) fireEvent.keyDown(sep, { key: "ArrowRight" });
  expect(vis(1).width).toBe("80");
  expect(sep.getAttribute("aria-valuenow")).toBe("80");
  fireEvent.click(screen.getByRole("button", { name: "▣ Birleştir" }));
  expect(vis(1)).toMatchObject({ visible: "true", width: "full" });
  expect(vis(2).visible).toBe("false");
  expect(screen.queryByRole("separator")).toBeNull();
});

test("Ctrl+Shift+\\ bölmeyi açar/kapatır; tek terminalde yeni terminal açar; bölmedeki terminal kapanınca diğeri kalır", async () => {
  await boot();
  chord("T", "KeyT");
  await waitFor(() => expect(vis(1).visible).toBe("true"));
  chord("|", "Backslash");
  await waitFor(() => expect(screen.queryByTestId("term-2")).toBeTruthy());
  expect(vis(1).width).toBe("50");
  expect(vis(2).active).toBe("true");
  chord("|", "Backslash");
  expect(vis(2).width).toBe("full");
  // Split again from terminal 2: it stays left, terminal 1 opens right and gets focus.
  chord("|", "Backslash");
  expect(vis(2)).toMatchObject({ width: "50", active: "false" });
  expect(vis(1).active).toBe("true");
  // Closing the focused pane shows the other one alone.
  chord("W", "KeyW");
  await waitFor(() => expect(screen.queryByTestId("term-1")).toBeNull());
  expect(vis(2)).toMatchObject({ visible: "true", width: "full" });
});

test("komut paleti: Ctrl+Shift+P açar, bulanık arar, Enter çalıştırır, Esc kapatır", async () => {
  await boot();
  chord("P", "KeyP");
  const input = screen.getByRole("combobox", { name: "Komut paleti" });
  expect(document.activeElement).toBe(input);
  fireEvent.change(input, { target: { value: "gcms" } });
  expect(within(screen.getByRole("listbox")).getAllByRole("option")[0].textContent).toContain("Geçmiş'e git");
  fireEvent.keyDown(input, { key: "Enter" });
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(await screen.findByText("Kayıt yok.")).toBeTruthy();

  chord("P", "KeyP");
  fireEvent.change(screen.getByRole("combobox", { name: "Komut paleti" }), { target: { value: "kalib" } });
  const opt = within(screen.getByRole("listbox")).getAllByRole("option")[0];
  expect(opt.textContent).toContain("oturum");
  fireEvent.keyDown(screen.getByRole("combobox", { name: "Komut paleti" }), { key: "Escape" });
  expect(screen.queryByRole("dialog")).toBeNull();
});

test("kısayol yardımı: Ctrl+Shift+/ açar, çevrili liste, Esc kapatır; paletten de açılır", async () => {
  await boot();
  chord("?", "Slash");
  const dlg = screen.getByRole("dialog", { name: "Kısayollar" });
  expect(within(dlg).getByText("Ctrl+Shift+T")).toBeTruthy();
  expect(within(dlg).getByText("Ctrl+Alt+1…9")).toBeTruthy();
  expect(within(dlg).getByText("Bölmeyi aç / birleştir")).toBeTruthy();
  fireEvent.keyDown(dlg, { key: "Escape" });
  expect(screen.queryByRole("dialog")).toBeNull();
  chord("P", "KeyP");
  fireEvent.change(screen.getByRole("combobox", { name: "Komut paleti" }), { target: { value: "kısayol" } });
  fireEvent.keyDown(screen.getByRole("combobox", { name: "Komut paleti" }), { key: "Enter" });
  expect(screen.getByRole("dialog", { name: "Kısayollar" })).toBeTruthy();
});

test("Ctrl+Shift+F geçerli sayfanın arama kutusuna odaklanır", async () => {
  await boot();
  chord("F", "KeyF");
  expect(document.activeElement).toBe(screen.getByLabelText("Ara: ad, klasör, model"));
});

test("komut paleti Ayarlar'ı da açar", async () => {
  await boot();
  chord("P", "KeyP");
  const input = screen.getByRole("combobox", { name: "Komut paleti" });
  fireEvent.change(input, { target: { value: "ayarlar" } });
  expect(within(screen.getByRole("listbox")).getAllByRole("option")[0].textContent).toContain("Ayarlar");
  fireEvent.keyDown(input, { key: "Enter" });
  expect(await screen.findByRole("heading", { name: "Ayarlar" })).toBeTruthy();
});

test("kısayol katmanı Ayarlar panelinin girişlerine dokunmaz", async () => {
  await boot();
  fireEvent.click(screen.getByRole("button", { name: /Ayarlar/ }));
  const box = (await screen.findAllByRole("checkbox"))[0] as HTMLInputElement;
  box.focus();
  const inner = vi.fn();
  box.addEventListener("keydown", inner);
  for (const init of [
    { key: " ", code: "Space" }, { key: "Enter", code: "Enter" }, { key: "Tab", code: "Tab" },
    { key: "Tab", code: "Tab", shiftKey: true }, { key: "F", code: "KeyF", ctrlKey: true, shiftKey: true },
  ]) {
    expect(key(init, box).defaultPrevented).toBe(false);
  }
  expect(inner).toHaveBeenCalledTimes(5);
  expect(screen.getByRole("heading", { name: "Ayarlar" })).toBeTruthy();
});

test("bölünmüş terminalde odak bağlamı odaktaki bölmenin oturumunu izler", async () => {
  await boot();
  const card = { sessionId: "s-1", status: "busy", pid: 5 };
  chord("T", "KeyT");
  chord("T", "KeyT");
  await waitFor(() => expect(vis(2).visible).toBe("true"));
  act(() => { fake.inside.set(1, card); });
  key({ key: "1", code: "Digit1", ctrlKey: true, altKey: true });
  chord("|", "Backslash");
  expect(vis(2).active).toBe("true");
  const focus = () => invoke.mock.calls.filter(([c]) => c === "set_focus_context").map((c) => (c as unknown[])[1]);
  await waitFor(() => expect(focus().slice(-1)[0]).toEqual({ sessionId: null }));
  fireEvent.mouseDown(screen.getByTestId("term-1"));
  expect(vis(1).active).toBe("true");
  await waitFor(() => expect(focus().slice(-1)[0]).toEqual({ sessionId: "s-1" }));
});
