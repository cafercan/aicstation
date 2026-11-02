import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";
import type { HistoryRow, HistoryView, SearchOutcome, SessionCard, Snapshot } from "../api";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));
vi.mock("../terminal/store", () => ({ terminalForPid: () => undefined }));

import { setLang } from "../i18n";
import { focusSearch } from "../shortcuts/bus";
import Dashboard, { resetDashboardFilter } from "./Dashboard";
import History, { resetHistoryFilters } from "./History";

const card = (id: string, over: Partial<SessionCard>): SessionCard => ({
  sessionId: id, pid: 1, parentPid: null, name: id, cwd: `C:\\p\\${id}`, status: "busy", waitingFor: null,
  model: "Opus 5.5", contextPct: null, costUsd: 0, agentsTotal: 0, agentsFinished: 0, toolOk: 0, toolErr: 0,
  lastTool: null, startedAtMs: 0, lastActivityMs: 0, gitBranch: null, hostKind: "other", ...over,
});
const snap = (cards: SessionCard[]): Snapshot => ({ sessions: cards, runningAgents: 0, waitingCount: 0 });

const today = new Date();
const key = (d: Date) => `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
const daysAgo = (n: number) => new Date(today.getFullYear(), today.getMonth(), today.getDate() - n, 12);

const row = (id: string, cost: number, cwd: string, model: string, ago: number): HistoryRow => ({
  sessionId: id, modifiedMs: daysAgo(ago).getTime(), size: 1, name: id, cwd: null,
  summary: {
    name: `ad-${id}`, cwd, model, firstAtMs: 0, lastAtMs: 60_000, agentsTotal: 0, costUsd: cost,
    costByDay: { [key(daysAgo(ago))]: cost },
  },
});
const hview = (over: Partial<HistoryView> = {}): HistoryView => ({
  rows: [
    row("a", 3, "C:\\p\\İstanbul", "claude-opus-5-5", 1),
    row("b", 1, "C:\\p\\beta", "claude-sonnet-5-5", 10),
    row("c", 7, "C:\\p\\gama", "claude-opus-5-5", 40),
  ],
  todayCost: 0.5, todayComplete: true, indexing: false, ...over,
});

function mockBackend(view: HistoryView, search?: (q: string, gen: number) => Promise<SearchOutcome>) {
  invoke.mockImplementation((cmd: string, args?: { query: string; generation: number }) => {
    if (cmd === "get_history") return Promise.resolve(view);
    if (cmd === "search_history" && search) return search(args!.query, args!.generation);
    return Promise.reject(new Error(`unexpected ${cmd}`));
  });
}

beforeEach(() => {
  invoke.mockReset();
  resetDashboardFilter();
  resetHistoryFilters();
});

test("Dashboard: arama kutusu ad/klasör/model süzer (Türkçe İ/ı duyarsız), boşsa 'uyan yok'", () => {
  const cards = [card("Işık", {}), card("web", { cwd: "D:\\İş\\web" }), card("cli", { model: "Sonnet 5.5" })];
  render(<Dashboard snapshot={snap(cards)} history={null} now={0} onOpen={() => {}} />);
  const box = screen.getByLabelText("Ara: ad, klasör, model");
  fireEvent.change(box, { target: { value: "is" } });
  expect(screen.getByText("Işık")).toBeTruthy();
  expect(screen.getByText("web")).toBeTruthy();
  expect(screen.queryByText("cli")).toBeNull();
  fireEvent.change(box, { target: { value: "sonnet" } });
  expect(screen.getByText("cli")).toBeTruthy();
  expect(screen.queryByText("web")).toBeNull();
  fireEvent.change(box, { target: { value: "yok böyle" } });
  expect(screen.getByText("Filtreye uyan oturum yok.")).toBeTruthy();
});

test("Dashboard: durum çipleri bağımsız açılıp kapanır; süzgeç sayfa değişince bellekte kalır", () => {
  const cards = [
    card("a", { status: "busy" }), card("b", { status: "waiting" }), card("c", { status: "idle" }),
    card("d", { status: "shell" }),
  ];
  const r = render(<Dashboard snapshot={snap(cards)} history={null} now={0} onOpen={() => {}} />);
  const chips = within(screen.getByRole("group", { name: "Durum filtresi" }));
  const waiting = chips.getByRole("button", { name: "onay bekliyor" });
  fireEvent.click(waiting);
  fireEvent.click(chips.getByRole("button", { name: "komut çalıştırıyor" }));
  expect(waiting.getAttribute("aria-pressed")).toBe("true");
  expect(["a", "b", "c", "d"].filter((id) => screen.queryByText(id))).toEqual(["b", "d"]);
  r.unmount();
  render(<Dashboard snapshot={snap(cards)} history={null} now={0} onOpen={() => {}} />);
  expect(["a", "b", "c", "d"].filter((id) => screen.queryByText(id))).toEqual(["b", "d"]);
  fireEvent.click(screen.getByRole("button", { name: "onay bekliyor" }));
  expect(["a", "b", "c", "d"].filter((id) => screen.queryByText(id))).toEqual(["d"]);
});

test("Ctrl+Shift+F hedefi: sayfanın arama kutusuna odaklanır", () => {
  render(<Dashboard snapshot={snap([])} history={null} now={0} onOpen={() => {}} />);
  expect(focusSearch()).toBe(true);
  expect(document.activeElement).toBe(screen.getByLabelText("Ara: ad, klasör, model"));
});

test("Geçmiş: metin süzgeci ve maliyete göre sıralama", async () => {
  mockBackend(hview());
  render(<History onOpen={() => {}} />);
  await screen.findByText("ad-a");
  const names = () => [...document.querySelectorAll("table.hist tbody tr")].map((tr) => tr.children[1].textContent);
  expect(names()).toEqual(["ad-a", "ad-b", "ad-c"]);
  fireEvent.change(screen.getByLabelText("Sırala"), { target: { value: "cost" } });
  expect(names()).toEqual(["ad-c", "ad-a", "ad-b"]);
  fireEvent.change(screen.getByLabelText("Ara: ad, klasör, model"), { target: { value: "ISTANBUL" } });
  expect(names()).toEqual(["ad-a"]);
  fireEvent.change(screen.getByLabelText("Ara: ad, klasör, model"), { target: { value: "zzz" } });
  expect(screen.getByText("Filtreye uyan kayıt yok.")).toBeTruthy();
});

test("Geçmiş: tarih aralığı iki ucu dahil süzer", async () => {
  mockBackend(hview());
  render(<History onOpen={() => {}} />);
  await screen.findByText("ad-a");
  fireEvent.change(screen.getByLabelText("Başlangıç"), { target: { value: key(daysAgo(20)) } });
  fireEvent.change(screen.getByLabelText("Bitiş"), { target: { value: key(daysAgo(1)) } });
  expect(screen.getByText("ad-a")).toBeTruthy();
  expect(screen.getByText("ad-b")).toBeTruthy();
  expect(screen.queryByText("ad-c")).toBeNull();
});

const hit = (sessionId: string, before: string, h: string, after: string) =>
  ({ sessionId, atMs: 0, role: "assistant" as const, snippet: { before, hit: h, after } });

test("İçerikte ara: kısa sorgu backend'e gitmez; sonuç parçası vurgulu, tıklayınca oturum açılır", async () => {
  const search = vi.fn(async () => ({ hits: [hit("a", "önce ", "kalibrasyon", " sonra")], truncated: false, cancelled: false }));
  mockBackend(hview(), search);
  const onOpen = vi.fn();
  render(<History onOpen={onOpen} />);
  await screen.findByText("ad-a");
  const box = screen.getByLabelText("İçerikte ara");
  fireEvent.change(box, { target: { value: " k " } });
  fireEvent.submit(box.closest("form")!);
  expect(await screen.findByText("Arama metni 2–200 karakter olmalı")).toBeTruthy();
  expect(search).not.toHaveBeenCalled();
  fireEvent.change(box, { target: { value: "kalibrasyon" } });
  fireEvent.submit(box.closest("form")!);
  const mark = await screen.findByText("kalibrasyon", { selector: "mark" });
  expect(search).toHaveBeenCalledWith("kalibrasyon", expect.any(Number));
  expect(mark.parentElement!.textContent).toBe("önce kalibrasyon sonra");
  expect(screen.getByText("1 sonuç")).toBeTruthy();
  fireEvent.click(mark.closest("button")!);
  expect(onOpen).toHaveBeenCalledWith("a");
});

test("İçerikte ara: yeni arama eskisinin geç gelen yanıtını ezer; her aramanın kuşağı farklı", async () => {
  const pending: ((o: SearchOutcome) => void)[] = [];
  const gens: number[] = [];
  mockBackend(hview(), (_q, gen) => { gens.push(gen); return new Promise((res) => pending.push(res)); });
  render(<History onOpen={() => {}} />);
  await screen.findByText("ad-a");
  const box = screen.getByLabelText("İçerikte ara");
  fireEvent.change(box, { target: { value: "eski" } });
  fireEvent.submit(box.closest("form")!);
  fireEvent.change(box, { target: { value: "yeni" } });
  fireEvent.submit(box.closest("form")!);
  await waitFor(() => expect(gens).toHaveLength(2));
  expect(gens[1]).toBeGreaterThan(gens[0]);
  await act(async () => { pending[1]({ hits: [hit("b", "", "yeni", "")], truncated: true, cancelled: false }); });
  await act(async () => { pending[0]({ hits: [hit("a", "", "eski", "")], truncated: false, cancelled: false }); });
  expect(screen.getByText("yeni", { selector: "mark" })).toBeTruthy();
  expect(screen.queryByText("eski", { selector: "mark" })).toBeNull();
  expect(screen.getByText(/ilk 1 sonuç gösteriliyor/)).toBeTruthy();
});

test("Maliyet: 30 gün varsayılan, bugün vurgulu; 7/90 aralığı; tahmini toplam ve ortalama", async () => {
  mockBackend(hview());
  render(<History onOpen={() => {}} />);
  await screen.findByText("ad-a");
  const bars = () => document.querySelectorAll(".cost-daily rect[data-date]");
  expect(bars()).toHaveLength(30);
  const todayBar = document.querySelector(`.cost-daily rect[data-date="${key(today)}"]`)!;
  expect(todayBar.getAttribute("data-today")).toBe("true");
  expect((todayBar as SVGElement).style.fill).toBe("var(--yellow)");
  // 30 days: a (3) + b (1) + today's live slice (0.5); c is 40 days old.
  expect(screen.getByText("$4.50")).toBeTruthy();
  expect(screen.getByText("aralık toplamı (tahmini)")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "90 gün" }));
  expect(bars()).toHaveLength(90);
  expect(screen.getByText("$11.50")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "7 gün" }));
  expect(bars()).toHaveLength(7);
  expect(screen.getByRole("button", { name: "7 gün" }).getAttribute("aria-pressed")).toBe("true");
});

test("Maliyet: en pahalı projeler tam yol ipucuyla; model dağılımı; boş aralıkta not", async () => {
  mockBackend(hview());
  render(<History onOpen={() => {}} />);
  await screen.findByText("ad-a");
  const top = document.querySelector(".cost-top")!;
  const items = [...top.querySelectorAll("li")];
  expect(items.map((li) => li.querySelector(".lbl")!.textContent)).toEqual(["İstanbul", "beta"]);
  expect(items[0].getAttribute("title")).toBe("C:\\p\\İstanbul");
  const legend = [...document.querySelectorAll(".cost-models .legend li")].map((li) => li.textContent);
  expect(legend).toEqual(["claude-opus-5-575%", "claude-sonnet-5-525%"]);
  fireEvent.click(screen.getByRole("button", { name: "7 gün" }));
  expect([...document.querySelectorAll(".cost-top .lbl")].map((e) => e.textContent)).toEqual(["İstanbul"]);
});

test("Maliyet: veri yoksa sıfır çubuklar ve 'maliyet yok' notları", async () => {
  mockBackend(hview({ rows: [], todayCost: 0 }));
  render(<History onOpen={() => {}} />);
  await screen.findByText("Kayıt yok.");
  expect(document.querySelectorAll(".cost-daily rect[data-date]")).toHaveLength(30);
  expect(screen.getAllByText("Bu aralıkta maliyet yok.")).toHaveLength(2);
  expect(screen.getAllByText("$0.00").length).toBeGreaterThanOrEqual(2);
});

test("Maliyet: klavye odağında ipucu tarih ve $ gösterir, oklarla gezilir", async () => {
  mockBackend(hview());
  render(<History onOpen={() => {}} />);
  await screen.findByText("ad-a");
  const svg = document.querySelector(".cost-daily svg")!;
  fireEvent.focus(svg);
  expect(document.querySelector(".cost-tip")!.textContent).toMatch(/\(bugün\): \$0\.50$/);
  fireEvent.keyDown(svg, { key: "ArrowLeft" });
  expect(document.querySelector(".cost-tip")!.textContent).toMatch(/: \$3\.00$/);
  fireEvent.blur(svg);
  expect(document.querySelector(".cost-tip")).toBeNull();
});

test("Maliyet: dizinleme sürerken 'hesaplanıyor' notu", async () => {
  mockBackend(hview({ indexing: true, todayComplete: false }));
  render(<History onOpen={() => {}} />);
  expect(await screen.findByText("Geçmiş dizinleniyor; grafikler hesaplanıyor…")).toBeTruthy();
});

test("Maliyet İngilizce: aralık düğmeleri çoğul", async () => {
  setLang("en");
  mockBackend(hview());
  render(<History onOpen={() => {}} />);
  await screen.findByText("ad-a");
  expect(screen.getByRole("button", { name: "30 days" })).toBeTruthy();
  expect(screen.getByText("COST BY MODEL")).toBeTruthy();
});
