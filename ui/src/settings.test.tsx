import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import type { Settings } from "./api";

const A = "0b2f6a40-1111-4c3e-9d2a-5f1e2d3c4b5a";

const DEFAULTS: Settings = {
  notifyWaiting: true, notifyFinished: true, notifySubagent: false, notifySound: false,
  closeToTray: true, trayNoticeShown: false, lang: "tr",
  shell: "auto", prompt: "own", promptPreset: "",
};

const h = vi.hoisted(() => ({
  stored: null as unknown as Settings,
  failSet: false,
  buta: { installed: true, version: "buta 0.4.0", presets: [
    { name: "carpet", description: "carpet colors" }, { name: "neon", description: "synthwave" },
  ] } as { installed: boolean; version: string | null; presets: { name: string; description: string }[] },
  listeners: new Map<string, (e: { payload: unknown }) => void>(),
  closeHandler: null as null | ((e: { preventDefault: () => void }) => void),
  destroy: vi.fn(async () => {}),
}));

const invoke = vi.hoisted(() => vi.fn(async (cmd: string, args?: Record<string, unknown>): Promise<unknown> => {
  if (cmd === "get_snapshot") return { sessions: [], runningAgents: 0, waitingCount: 0 };
  if (cmd === "pty_list") return [];
  if (cmd === "get_settings") return h.stored;
  if (cmd === "set_settings") {
    if (h.failSet) throw "settings_save_failed:disk full";
    h.stored = { ...(args!.settings as Settings), trayNoticeShown: h.stored.trayNoticeShown };
    return h.stored;
  }
  if (cmd === "list_shells") return ["auto", "bash", "fish"];
  if (cmd === "butaglyph_info") return h.buta;
  if (cmd === "open_butaglyph_editor") return undefined;
  if (cmd === "get_session_detail") return null;
  if (cmd === "get_history") return { rows: [], todayCost: 0, todayTokens: 0, todayComplete: true, indexing: false };
  return undefined;
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: class {} }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, cb: (e: { payload: unknown }) => void) => {
    h.listeners.set(name, cb);
    return () => { h.listeners.delete(name); };
  }),
}));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    onCloseRequested: async (cb: (e: { preventDefault: () => void }) => void) => { h.closeHandler = cb; return () => {}; },
    destroy: h.destroy,
  }),
}));
vi.mock("@xterm/xterm", () => ({ Terminal: class {} }));
vi.mock("@xterm/addon-fit", () => ({ FitAddon: class {} }));

import App from "./App";
import SettingsPage from "./pages/Settings";
import { loadSettings } from "./settings";
import { resetUiState } from "./testing/reset";
import { setLang } from "./i18n";

const calls = (cmd: string) => invoke.mock.calls.filter((c) => c[0] === cmd).map((c) => c[1]);

beforeEach(() => {
  h.stored = { ...DEFAULTS };
  h.failSet = false;
  h.buta = { installed: true, version: "buta 0.4.0", presets: [
    { name: "carpet", description: "carpet colors" }, { name: "neon", description: "synthwave" },
  ] };
  h.listeners.clear();
  h.closeHandler = null;
  invoke.mockClear();
  h.destroy.mockClear();
  setLang("tr");
  resetUiState();
});
afterEach(() => { setLang("tr"); });

test("dişli ayarlar sayfasını açar; anahtarlar kayıtlı değerleri gösterir", async () => {
  render(<App />);
  fireEvent.click(screen.getByRole("button", { name: "⚙ Ayarlar" }));
  expect(await screen.findByRole("heading", { name: "Ayarlar" })).toBeTruthy();
  const box = (name: string) => screen.getByRole("checkbox", { name }) as HTMLInputElement;
  expect(box("Bir oturum onay beklemeye başlayınca").checked).toBe(true);
  expect(box("Çalışan bir oturum işini bitirince (en az 10 sn)").checked).toBe(true);
  expect(box("Bir alt ajan bitince").checked).toBe(false);
  expect(box("Bildirim sesi (sistemin varsayılan sesi)").checked).toBe(false);
  expect(screen.getByRole("checkbox", { name: /Kapatınca tepsiye küçült/ }).getAttribute("type")).toBe("checkbox");
  expect((screen.getByRole("checkbox", { name: /Kapatınca tepsiye küçült/ }) as HTMLInputElement).checked).toBe(true);
});

test("anahtar değişince tüm ayarlar dil koduyla birlikte kaydedilir", async () => {
  await loadSettings();
  invoke.mockClear();
  render(<SettingsPage />);
  fireEvent.click(screen.getByRole("checkbox", { name: "Bildirim sesi (sistemin varsayılan sesi)" }));
  await waitFor(() => expect(calls("set_settings")).toEqual([{ settings: { ...DEFAULTS, notifySound: true } }]));
  expect((screen.getByRole("checkbox", { name: "Bildirim sesi (sistemin varsayılan sesi)" }) as HTMLInputElement).checked).toBe(true);
});

test("kabuk: kurulu kabuklar listelenir, seçim kaydedilir", async () => {
  await loadSettings();
  invoke.mockClear();
  render(<SettingsPage />);
  const select = screen.getByRole("combobox", { name: "Kabuk" }) as HTMLSelectElement;
  await waitFor(() => expect(select.options.length).toBe(3));
  expect([...select.options].map((o) => o.textContent)).toEqual(["Otomatik (Windows PowerShell)", "bash", "fish"]);
  fireEvent.change(select, { target: { value: "fish" } });
  await waitFor(() => expect(calls("set_settings")).toEqual([{ settings: { ...DEFAULTS, shell: "fish" } }]));
});

test("kabuk: artık kurulu olmayan kayıtlı kabuk yine listede görünür", async () => {
  h.stored = { ...DEFAULTS, shell: "zsh" };
  await loadSettings();
  render(<SettingsPage />);
  const select = screen.getByRole("combobox", { name: "Kabuk" }) as HTMLSelectElement;
  await waitFor(() => expect(select.options.length).toBe(4));
  expect(select.value).toBe("zsh");
});

test("prompt: butaglyph seçilince presetler gelir; preset seçimi kaydedilir", async () => {
  await loadSettings();
  render(<SettingsPage />);
  expect(screen.queryByRole("combobox", { name: "Preset" })).toBeNull();
  fireEvent.change(screen.getByRole("combobox", { name: "Prompt" }), { target: { value: "butaglyph" } });
  const preset = await screen.findByRole("combobox", { name: "Preset" }) as HTMLSelectElement;
  expect([...preset.options].map((o) => o.value)).toEqual(["", "carpet", "neon"]);
  // Own config: the editor button; a preset: its description instead.
  fireEvent.click(screen.getByRole("button", { name: "butaglyph düzenleyicisinde düzenle ↗" }));
  await waitFor(() => expect(calls("open_butaglyph_editor")).toHaveLength(1));
  fireEvent.change(preset, { target: { value: "neon" } });
  await waitFor(() => expect(h.stored).toEqual({ ...DEFAULTS, prompt: "butaglyph", promptPreset: "neon" }));
  expect(screen.getByText("synthwave")).toBeTruthy();
  expect(screen.queryByRole("button", { name: /düzenleyicisinde/ })).toBeNull();
});

test("prompt: butaglyph kurulu değilse uyarı gösterilir", async () => {
  h.buta = { installed: false, version: null, presets: [] };
  h.stored = { ...DEFAULTS, prompt: "butaglyph" };
  await loadSettings();
  render(<SettingsPage />);
  expect(await screen.findByText(/butaglyph kurulu değil/)).toBeTruthy();
  expect(screen.queryByRole("combobox", { name: "Preset" })).toBeNull();
});

test("kaydetme reddedilirse anahtar geri alınır ve hata gösterilir", async () => {
  await loadSettings();
  h.failSet = true;
  render(<SettingsPage />);
  const box = screen.getByRole("checkbox", { name: "Bir alt ajan bitince" }) as HTMLInputElement;
  fireEvent.click(box);
  expect((await screen.findByRole("alert")).textContent).toContain("Ayarlar kaydedilemedi");
  expect(box.checked).toBe(false);
});

test("arayüz dili değişince arka uca bildirilir", async () => {
  render(<App />);
  await waitFor(() => expect(calls("get_settings").length).toBe(1));
  act(() => setLang("en"));
  await waitFor(() => expect(calls("set_settings")).toContainEqual({ settings: { ...DEFAULTS, lang: "en" } }));
});

test("odak bağlamı: oturum detayı açılınca id, panoya dönünce boş liste gönderilir", async () => {
  render(<App />);
  await waitFor(() => expect(calls("set_focus_context")).toEqual([{ sessionIds: [] }]));
  // Toast / tray menu click: the backend asks the UI to open the session.
  await waitFor(() => expect(h.listeners.has("navigate")).toBe(true));
  act(() => h.listeners.get("navigate")!({ payload: { session: A } }));
  await waitFor(() => expect(calls("set_focus_context").slice(-1)[0]).toEqual({ sessionIds: [A] }));
  act(() => h.listeners.get("navigate")!({ payload: { session: null } }));
  await waitFor(() => expect(calls("set_focus_context").slice(-1)[0]).toEqual({ sessionIds: [] }));
});

test("ayarlar yüklenmeden pencere kapatılırsa varsayılan geçerli: tepsiye gizlenir", async () => {
  // get_settings never answers.
  const never = new Promise<never>(() => {});
  const base = invoke.getMockImplementation()!;
  invoke.mockImplementation(async (cmd: string, args?: Record<string, unknown>) =>
    cmd === "get_settings" ? never : base(cmd, args));
  render(<App />);
  await waitFor(() => expect(h.closeHandler).not.toBeNull());
  const prevent = vi.fn();
  act(() => h.closeHandler!({ preventDefault: prevent }));
  expect(prevent).toHaveBeenCalled();
  await waitFor(() => expect(calls("hide_to_tray").length).toBe(1));
  expect(h.destroy).not.toHaveBeenCalled();
  invoke.mockImplementation(base);
});

test("pencereyi kapatmak: tepsi açıksa gizler, kapalıysa uygulamadan çıkar", async () => {
  render(<App />);
  await waitFor(() => expect(h.closeHandler).not.toBeNull());
  await waitFor(() => expect(calls("get_settings").length).toBe(1));
  await act(async () => { await Promise.resolve(); });
  const prevent = vi.fn();
  act(() => h.closeHandler!({ preventDefault: prevent }));
  expect(prevent).toHaveBeenCalled();
  await waitFor(() => expect(calls("hide_to_tray").length).toBe(1));
  expect(h.destroy).not.toHaveBeenCalled();

  // Turn "minimize to tray" off: closing now exits.
  fireEvent.click(screen.getByRole("button", { name: "⚙ Ayarlar" }));
  fireEvent.click(await screen.findByRole("checkbox", { name: /Kapatınca tepsiye küçült/ }));
  await waitFor(() => expect(h.stored.closeToTray).toBe(false));
  act(() => h.closeHandler!({ preventDefault: () => {} }));
  await waitFor(() => expect(h.destroy).toHaveBeenCalledTimes(1));
  expect(calls("hide_to_tray").length).toBe(1);
});

test("tepsiden Çık: kapatma onayından geçip pencereyi kapatır", async () => {
  render(<App />);
  await waitFor(() => expect(h.listeners.has("tray-quit")).toBe(true));
  act(() => h.listeners.get("tray-quit")!({ payload: null }));
  await waitFor(() => expect(h.destroy).toHaveBeenCalledTimes(1));
  expect(calls("hide_to_tray")).toEqual([]);
});
