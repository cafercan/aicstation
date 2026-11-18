import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";
import type { SessionCard } from "./api";

/** Sessions `get_snapshot` answers with; `push` sends a snapshot event. */
const snap = vi.hoisted(() => ({ sessions: [] as unknown[] }));
/** Listeners by event name (unlisten is a no-op here). */
const events = vi.hoisted(() => new Map<string, ((e: { payload: unknown }) => void)[]>());
const emitEvent = (name: string, payload: unknown) => (events.get(name) ?? []).forEach((cb) => cb({ payload }));
const invoke = vi.hoisted(() => vi.fn(async (cmd: string, _args?: unknown): Promise<unknown> => {
  if (cmd === "get_snapshot") return { sessions: snap.sessions, runningAgents: 0, waitingCount: 0 };
  if (cmd === "pty_list") return [{ id: 2, pid: 20 }];
  if (cmd === "pty_kill") return undefined;
  if (cmd === "get_session_detail") return null;
  if (cmd === "set_focus_context") return null;
  if (cmd === "get_conversation_page") return { epoch: 1, total: 1, start: 0, items: [{ kind: "user", text: "sohbet metni", truncated: false, images: [], atMs: 1 }] };
  return { rows: [], todayCost: 0, todayTokens: 0, todayComplete: true, indexing: false };
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: class {} }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, cb: (e: { payload: unknown }) => void) => {
    events.set(name, [...(events.get(name) ?? []), cb]);
    return () => {};
  }),
}));

vi.mock("@xterm/xterm", () => ({ Terminal: class {} }));
vi.mock("@xterm/addon-fit", () => ({ FitAddon: class {} }));

/** In-memory terminals (no xterm, no PTY); the rest of the store is real. */
const fake = vi.hoisted(() => {
  type E = { id: number; pid: number; n: number };
  let list: E[] = [];
  const ls = new Set<() => void>();
  const emit = () => ls.forEach((l) => l());
  return {
    get: () => list,
    subscribe: (l: () => void) => { ls.add(l); return () => { ls.delete(l); }; },
    reset: () => { list = []; emit(); },
    add: (id: number, pid: number) => { const e = { id, pid, n: id }; list = [...list, e]; emit(); return e; },
  };
});
const openResume = vi.hoisted(() => vi.fn());
const openAdopt = vi.hoisted(() => vi.fn());
vi.mock("./terminal/store", async (orig) => {
  const real = await orig<typeof import("./terminal/store")>();
  const React = await import("react");
  return {
    ...real,
    useTerminals: () => React.useSyncExternalStore(fake.subscribe, fake.get),
    terminalForPid: (p: number | null | undefined) => (p == null ? undefined : fake.get().find((e) => e.pid === p)?.id),
    allTerminalIds: () => fake.get().map((e) => e.id),
    openResumeTerminal: (...a: unknown[]) => openResume(...a),
    openAdoptTerminal: (...a: unknown[]) => openAdopt(...a),
  };
});
vi.mock("./pages/TerminalView", () => ({
  default: (p: { entry: { id: number }; visible: boolean }) => (
    <div data-testid={`term-${p.entry.id}`} data-visible={String(p.visible)} />
  ),
}));

import App from "./App";
import { resetUiState } from "./testing/reset";

function card(over: Partial<SessionCard>): SessionCard {
  return {
    sessionId: "s1", pid: 7, parentPid: null, name: "demo", cwd: "C:/x/demo", status: "idle", waitingFor: null,
    model: null, contextPct: null, costUsd: 0, tokens: { input: 0, output: 0, cacheRead: 0, cacheCreation: 0, cacheCreation1h: 0 },
    agentsTotal: 0, agentsFinished: 0, toolOk: 0, toolErr: 0, lastTool: null, startedAtMs: 0, lastActivityMs: 0,
    gitBranch: null, hostKind: "other", movable: false, agentsRunning: 0, remoteControl: false, ...over,
  };
}
/** Sends a snapshot event with these sessions. */
const push = (sessions: SessionCard[]) => act(async () => {
  snap.sessions = sessions;
  emitEvent("snapshot", { sessions, runningAgents: 0, waitingCount: 0 });
});
const navigate = (session: string) => act(async () => { emitEvent("navigate", { session }); });
const tab = (name: string) => screen.getByRole("tab", { name });
const termVisible = (id: number) => screen.getByTestId(`term-${id}`).getAttribute("data-visible");

beforeEach(() => {
  resetUiState();
  fake.reset();
  events.clear();
  snap.sessions = [];
  openResume.mockReset();
  openAdopt.mockReset();
});

test("iskelet: menü ve status bar görünür", async () => {
  render(<App />);
  expect(screen.getByText("▦ Dashboard")).toBeTruthy();
  await waitFor(() => expect(screen.getByText("0 açık oturum")).toBeTruthy());
});

test("açılışta önceki sayfadan kalan PTY'ler kapatılır", async () => {
  render(<App />);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("pty_kill", { id: 2 }));
});

test("dashboard kartı: uygulama terminalindeki oturum Terminal sekmesinde açılır", async () => {
  fake.add(1, 50);
  snap.sessions = [card({ sessionId: "s-app", name: "uygulama", parentPid: 50 })];
  const { container } = render(<App />);
  await waitFor(() => expect(container.querySelector(".grid .card")).toBeTruthy());
  expect(screen.queryByRole("tablist")).toBeNull();
  fireEvent.click(container.querySelector(".grid .card")!);
  expect(tab("Terminal").getAttribute("aria-selected")).toBe("true");
  expect(tab("Bilgi").getAttribute("aria-selected")).toBe("false");
  expect(termVisible(1)).toBe("true");
});

test("dashboard kartı: uygulama dışındaki oturum Bilgi sekmesinde açılır", async () => {
  snap.sessions = [card({ sessionId: "s-ext", hostKind: "windowsTerminal" })];
  const { container } = render(<App />);
  await waitFor(() => expect(container.querySelector(".grid .card")).toBeTruthy());
  fireEvent.click(container.querySelector(".grid .card")!);
  expect(tab("Bilgi").getAttribute("aria-selected")).toBe("true");
  expect(await screen.findByText("Oturum bulunamadı.")).toBeTruthy();
});

test("sekmeye tıklamak Terminal ile Bilgi arasında geçer; Bilgi'de terminal satırı seçili kalır", async () => {
  fake.add(1, 50);
  snap.sessions = [card({ sessionId: "s-app", name: "uygulama", parentPid: 50 })];
  const { container } = render(<App />);
  await waitFor(() => expect(container.querySelector(".grid .card")).toBeTruthy());
  fireEvent.click(container.querySelector(".grid .card")!);
  fireEvent.click(tab("Bilgi"));
  expect(tab("Bilgi").getAttribute("aria-selected")).toBe("true");
  expect(termVisible(1)).toBe("false");
  expect(await screen.findByText("Oturum bulunamadı.")).toBeTruthy();
  expect(container.querySelector(".side .item.sub.on")).toBeTruthy();
  fireEvent.click(tab("Terminal"));
  expect(tab("Terminal").getAttribute("aria-selected")).toBe("true");
  expect(termVisible(1)).toBe("true");
  expect(screen.queryByText("Oturum bulunamadı.")).toBeNull();
});

test("Sohbet sekmesi konuşmayı gösterir, terminal satırı seçili kalır; Onay notu Terminal sekmesine döner", async () => {
  fake.add(1, 50);
  snap.sessions = [card({ sessionId: "s-app", name: "uygulama", parentPid: 50, status: "waiting" })];
  const { container } = render(<App />);
  await waitFor(() => expect(container.querySelector(".grid .card")).toBeTruthy());
  fireEvent.click(container.querySelector(".grid .card")!);
  fireEvent.click(tab("Sohbet"));
  expect(tab("Sohbet").getAttribute("aria-selected")).toBe("true");
  expect(termVisible(1)).toBe("false");
  expect(await screen.findByText("sohbet metni")).toBeTruthy();
  expect(container.querySelector(".side .item.sub.on")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Terminal sekmesi" }));
  expect(tab("Terminal").getAttribute("aria-selected")).toBe("true");
  expect(termVisible(1)).toBe("true");
});

test("Böl düğmesi oturumun sekme satırında durur; ayrı bir satır kalmaz", async () => {
  fake.add(1, 50);
  snap.sessions = [card({ sessionId: "s-app", name: "uygulama", parentPid: 50 })];
  const { container } = render(<App />);
  await waitFor(() => expect(container.querySelector(".grid .card")).toBeTruthy());
  fireEvent.click(container.querySelector(".grid .card")!);
  const row = screen.getByRole("tablist");
  expect(row.querySelector(".term-bar.inline button")).toBeTruthy();
  expect(container.querySelectorAll(".term-bar").length).toBe(1);
  expect(container.querySelector(".term-area")!.className).toBe("term-area no-bar");
  fireEvent.click(tab("Bilgi"));
  expect(container.querySelector(".term-bar")).toBeNull();
});

test("Claude'suz kabuk terminalinde Böl kendi satırında kalır", async () => {
  fake.add(1, 50);
  const { container } = render(<App />);
  fireEvent.click(container.querySelector(".side .item.sub")!);
  expect(container.querySelector(".term-bar:not(.inline) button")).toBeTruthy();
  expect(container.querySelector(".term-area")!.className).toBe("term-area");
});

test("kenar çubuğu gizlenir, ince şerit kalır; göster ve Ctrl+Shift+B geri getirir; tercih saklanır", async () => {
  render(<App />);
  fireEvent.click(screen.getByRole("button", { name: "Kenar çubuğunu gizle" }));
  expect(screen.queryByText("▦ Dashboard")).toBeNull();
  expect(localStorage.getItem("station.sidebarOpen")).toBe("0");
  fireEvent.click(screen.getByRole("button", { name: "Kenar çubuğunu göster" }));
  expect(screen.getByText("▦ Dashboard")).toBeTruthy();
  expect(localStorage.getItem("station.sidebarOpen")).toBe("1");
  fireEvent.keyDown(window, { key: "B", code: "KeyB", ctrlKey: true, shiftKey: true });
  expect(screen.queryByText("▦ Dashboard")).toBeNull();
  fireEvent.keyDown(window, { key: "B", code: "KeyB", ctrlKey: true, shiftKey: true });
  expect(screen.getByText("▦ Dashboard")).toBeTruthy();
});

test("Claude'suz kabuk terminalinde sekme çubuğu yok", async () => {
  fake.add(1, 50);
  const { container } = render(<App />);
  fireEvent.click(container.querySelector(".side .item.sub")!);
  expect(termVisible(1)).toBe("true");
  expect(screen.queryByRole("tablist")).toBeNull();
});

test("uygulama dışındaki oturumun Terminal sekmesi: yer tutucu; taşınınca yeni terminal gösterilir", async () => {
  const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);
  snap.sessions = [card({ sessionId: "s-ext", hostKind: "windowsTerminal", movable: true })];
  openAdopt.mockImplementation(async () => fake.add(3, 70));
  const { container } = render(<App />);
  await waitFor(() => expect(container.querySelector(".grid .card")).toBeTruthy());
  fireEvent.click(container.querySelector(".grid .card")!);
  fireEvent.click(tab("Terminal"));
  expect(screen.getByText("Bu oturum uygulamanın dışında, Windows Terminal'de çalışıyor.")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Uygulamaya taşı" }));
  await waitFor(() => expect(termVisible(3)).toBe("true"));
  expect(openAdopt).toHaveBeenCalledWith("s-ext", "demo");
  // Once the registry lists the session in the new terminal, the tab bar is back.
  await push([card({ sessionId: "s-ext", parentPid: 70, hostKind: "embedded" })]);
  expect(tab("Terminal").getAttribute("aria-selected")).toBe("true");
  confirm.mockRestore();
});

test("kapalı oturumun Terminal sekmesi Devam et sunar; kayıt gelmeden dönülürse yeni terminal açılır", async () => {
  openResume.mockImplementation(async () => fake.add(2, 60));
  render(<App />);
  await waitFor(() => expect(events.has("navigate")).toBe(true));
  await navigate("s-gone");
  fireEvent.click(tab("Terminal"));
  expect(screen.getByText("Bu oturum kapalı. Bir uygulama terminalinde devam ettirebilirsiniz.")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "▶ Devam et" }));
  await waitFor(() => expect(termVisible(2)).toBe("true"));
  // Back to the session view before the registry lists it: no second Resume offered.
  await navigate("s-gone");
  fireEvent.click(tab("Terminal"));
  expect(termVisible(2)).toBe("true");
  expect(screen.queryByRole("button", { name: "▶ Devam et" })).toBeNull();
  await push([card({ sessionId: "s-gone", parentPid: 60 })]);
  expect(tab("Terminal").getAttribute("aria-selected")).toBe("true");
});

test("Remote Control açık oturumda sekme satırı simgeyi gösterir ve tıklama oturum kimliğini iletir", async () => {
  fake.add(1, 50);
  snap.sessions = [card({ sessionId: "s-rc", name: "uzak", parentPid: 50, remoteControl: true }),
    card({ sessionId: "s-no", name: "yerel", pid: 8, parentPid: 51 })];
  const { container } = render(<App />);
  await waitFor(() => expect(container.querySelector(".grid .card")).toBeTruthy());
  fireEvent.click(Array.from(container.querySelectorAll(".grid .card")).find((c) => c.textContent!.includes("uzak"))!);
  const btn = screen.getByRole("tablist").querySelector("button.remote")!;
  expect(btn).toBeTruthy();
  fireEvent.click(btn);
  expect(invoke).toHaveBeenCalledWith("open_remote_control", { sessionId: "s-rc" });
});
