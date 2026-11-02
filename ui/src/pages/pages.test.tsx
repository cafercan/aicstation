import { render, screen, waitFor, fireEvent, within, act } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";
import type { AgentNode, HistoryRow, SessionDetail, ToolCall } from "../api";

const invoke = vi.fn();
let snapshotCb: (() => void) | null = null;
/** Session ids in the snapshot events the mock emits. */
let snapIds: string[] = [];
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (_n: string, cb: (e: { payload: unknown }) => void) => {
    snapshotCb = () => cb({
      payload: { sessions: snapIds.map((sessionId) => ({ sessionId })), runningAgents: 0, waitingCount: 0 },
    });
    return () => {};
  }),
}));
const terminalForPid = vi.fn((_p: number | null | undefined): number | undefined => undefined);
vi.mock("../terminal/store", () => ({ terminalForPid: (p: number | null) => terminalForPid(p) }));

import { setLang } from "../i18n";
import SessionDetailPage from "./SessionDetail";
import History from "./History";
import { resetUiState } from "../testing/reset";

const usage = { input: 1000, output: 500, cacheRead: 0, cacheCreation: 0, cacheCreation1h: 0 };
const call = (id: string, name: string, target: string, state: ToolCall["state"]): ToolCall =>
  ({ id, name, target, at: "2026-11-02T10:31:04Z", state });
const stats = (calls: ToolCall[]) => ({
  tokens: usage, costUsd: 0, toolCalls: calls, toolCallCount: calls.length, firstAt: null, lastAt: null,
});
const node = (id: string, kind: AgentNode["kind"], desc: string, children: AgentNode[] = [],
  calls: ToolCall[] = []): AgentNode => ({
  id, kind, agentType: kind.kind === "agent" ? "Explore" : null, description: desc,
  state: "running", stats: stats(calls), children,
});

function detail(over: Partial<SessionDetail> = {}): SessionDetail {
  return {
    card: {
      sessionId: "s1", pid: 1, parentPid: 77, name: "demo", cwd: "C:/x/demo", status: "busy",
      waitingFor: null, model: "Opus 5.5", contextPct: 62, costUsd: 1.2, agentsTotal: 1,
      agentsFinished: 0, toolOk: 0, toolErr: 0, lastTool: null, startedAtMs: 0, lastActivityMs: 0, gitBranch: "main",
      hostKind: "other",
    },
    tokens: usage, toolCallCount: 3,
    tree: {
      roots: [node("wf", { kind: "workflow", id: "wf_x" }, "akış", [
        node("ph", { kind: "phase", name: "Review" }, "faz", [
          node("ag", { kind: "agent" }, "ADC bul", [], [call("c9", "Grep", "ADC_CAL", "Ok")]),
        ]),
      ])],
      total: 1, finished: 0,
    },
    mainToolCalls: [
      call("c1", "Read", "main.c", "Ok"),
      call("c2", "Bash", "make", "Error"),
      call("c3", "Bash", "make all", "Running"),
    ],
    ...over,
  };
}

beforeEach(() => {
  resetUiState();
  invoke.mockReset();
  terminalForPid.mockReset();
  terminalForPid.mockReturnValue(undefined);
  snapshotCb = null;
  snapIds = [];
});

test("ağaç: workflow → aşama → ajan iç içe çizilir", async () => {
  invoke.mockResolvedValue(detail());
  render(<SessionDetailPage sessionId="s1" onBack={() => {}} />);
  const wf = await screen.findByTestId("node-wf");
  const ph = screen.getByTestId("node-ph");
  const ag = screen.getByTestId("node-ag");
  expect(wf.textContent).toContain("Workflow wf_x");
  expect(ph.textContent).toContain("Aşama Review");
  expect(ag.textContent).toContain("Explore");
  expect(wf.nextElementSibling?.contains(ph)).toBe(true);
  expect(ph.nextElementSibling?.contains(ag)).toBe(true);
});

test("ağaçta ajan seçilince sağdaki liste o ajanın tool çağrılarını gösterir", async () => {
  invoke.mockResolvedValue(detail());
  render(<SessionDetailPage sessionId="s1" onBack={() => {}} />);
  await screen.findByTestId("node-ag");
  expect(screen.getAllByTestId("tool-row")).toHaveLength(3);
  fireEvent.click(screen.getByTestId("node-ag"));
  const rows = screen.getAllByTestId("tool-row");
  expect(rows).toHaveLength(1);
  expect(rows[0].textContent).toContain("Grep");
  expect(rows[0].textContent).toContain("ADC_CAL");
});

test('tool satırı: Ok "✓", Error "✗", Running "… çalışıyor"', async () => {
  invoke.mockResolvedValue(detail());
  render(<SessionDetailPage sessionId="s1" onBack={() => {}} />);
  const rows = await screen.findAllByTestId("tool-row");
  expect(rows[0].textContent).toContain("✓");
  expect(rows[1].textContent).toContain("✗");
  expect(rows[2].textContent).toContain("… çalışıyor");
});

test('"Sekmeye git" gömülü terminalde o terminale geçer', async () => {
  invoke.mockResolvedValue(detail());
  const { unmount } = render(<SessionDetailPage sessionId="s1" onBack={() => {}} />);
  await screen.findByText("Klasörü aç");
  expect(screen.queryByText("Sekmeye git")).toBeNull();
  unmount();
  terminalForPid.mockReturnValue(4);
  const onTerminal = vi.fn();
  render(<SessionDetailPage sessionId="s1" onBack={() => {}} onTerminal={onTerminal} />);
  fireEvent.click(await screen.findByText("Sekmeye git"));
  expect(onTerminal).toHaveBeenCalledWith(4);
  expect(invoke.mock.calls.some((c) => c[0] === "focus_session_tab")).toBe(false);
});

test('"Sekmeye git" Windows Terminal oturumunda yalnızca kimlikle focus_session_tab çağırır, notu gösterir', async () => {
  const d = detail();
  d.card.hostKind = "windowsTerminal";
  invoke.mockImplementation(async (cmd: string) =>
    cmd === "focus_session_tab" ? "tab_ambiguous" : d);
  render(<SessionDetailPage sessionId="s1" onBack={() => {}} />);
  fireEvent.click(await screen.findByText("Sekmeye git"));
  await screen.findByText("Pencere öne getirildi; aynı adlı birden çok sekme var");
  expect(invoke).toHaveBeenCalledWith("focus_session_tab", { sessionId: "s1" });
});

test('"Sekmeye git" hatası satır içinde gösterilir; başka host ve kapalı oturumda düğme yok', async () => {
  const d = detail();
  d.card.hostKind = "windowsTerminal";
  invoke.mockImplementation(async (cmd: string) => {
    if (cmd === "focus_session_tab") throw "tab_not_found";
    return d;
  });
  const first = render(<SessionDetailPage sessionId="s1" onBack={() => {}} />);
  fireEvent.click(await screen.findByText("Sekmeye git"));
  expect((await screen.findByRole("alert")).textContent).toBe("Sekmeye gidilemedi: Sekme bulunamadı");
  first.unmount();
  for (const c of [{ ...d.card, hostKind: "other" as const }, { ...d.card, pid: 0, parentPid: null }]) {
    invoke.mockResolvedValue({ ...d, card: c });
    terminalForPid.mockReturnValue(c.pid ? undefined : 4);
    const r = render(<SessionDetailPage sessionId="s1" onBack={() => {}} />);
    await screen.findByText("Klasörü aç");
    expect(screen.queryByText("Sekmeye git")).toBeNull();
    r.unmount();
  }
});

test("snapshot olayında ayrıntı yeniden çekilir; Klasörü aç open_folder çağırır", async () => {
  invoke.mockResolvedValue(detail());
  render(<SessionDetailPage sessionId="s1" onBack={() => {}} />);
  await screen.findByText("Klasörü aç");
  const before = invoke.mock.calls.filter((c) => c[0] === "get_session_detail").length;
  await waitFor(() => expect(snapshotCb).not.toBeNull());
  await act(async () => { snapshotCb!(); });
  await waitFor(() =>
    expect(invoke.mock.calls.filter((c) => c[0] === "get_session_detail").length).toBe(before + 1));
  fireEvent.click(screen.getByText("Klasörü aç"));
  expect(invoke).toHaveBeenCalledWith("open_folder", { path: "C:/x/demo" });
});

const detailCalls = () => invoke.mock.calls.filter((c) => c[0] === "get_session_detail").length;
const closed = (over: Partial<SessionDetail["card"]> = {}) => {
  const d = detail();
  return { ...d, card: { ...d.card, pid: 0, parentPid: null, status: "idle", ...over } };
};
function deferred<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((r, j) => { resolve = r; reject = j; });
  return { promise, resolve, reject };
}

test("kapanmış oturum bir kez çekilir, snapshot olaylarında yeniden çekilmez; \"kapandı\" gösterilir", async () => {
  invoke.mockResolvedValue(closed());
  render(<SessionDetailPage sessionId="s1" onBack={() => {}} />);
  await screen.findByText("Klasörü aç");
  expect(screen.getByText(/kapandı/)).toBeTruthy();
  expect(screen.queryByText(/hazır/)).toBeNull();
  await waitFor(() => expect(snapshotCb).not.toBeNull());
  for (let i = 0; i < 3; i++) await act(async () => { snapshotCb!(); });
  await act(async () => { await new Promise((r) => setTimeout(r, 20)); });
  expect(detailCalls()).toBe(1);
});

test("pid 0 gelse de oturum snapshot'ta canlıysa ayrıntı yenilenmeye devam eder", async () => {
  // Backend glitch: a live session answered as closed with empty stats.
  invoke.mockResolvedValue(closed({ model: null }));
  snapIds = ["s1"];
  render(<SessionDetailPage sessionId="s1" live onBack={() => {}} />);
  await screen.findByText("Klasörü aç");
  await waitFor(() => expect(snapshotCb).not.toBeNull());
  invoke.mockResolvedValue(detail());
  await act(async () => { snapshotCb!(); });
  await waitFor(() => expect(detailCalls()).toBe(2));
  expect(await screen.findByText("çalışıyor")).toBeTruthy();
  expect(screen.queryByText(/kapandı/)).toBeNull();
});

test("canlı prop'u olmadan da: snapshot olayında oturum listedeyse pid 0 son kabul edilmez", async () => {
  invoke.mockResolvedValue(closed());
  render(<SessionDetailPage sessionId="s1" onBack={() => {}} />);
  await screen.findByText("Klasörü aç");
  await waitFor(() => expect(snapshotCb).not.toBeNull());
  snapIds = ["s1"];
  await act(async () => { snapshotCb!(); });
  await waitFor(() => expect(detailCalls()).toBe(2));
  await act(async () => { snapshotCb!(); });
  await waitFor(() => expect(detailCalls()).toBe(3));
});

test("kapanmış oturumda ▶ Devam et: onResume(id, ad); hata satır içinde", async () => {
  invoke.mockResolvedValue(closed());
  const onResume = vi.fn().mockRejectedValueOnce("session_live").mockResolvedValue(undefined);
  render(<SessionDetailPage sessionId="s1" onBack={() => {}} onResume={onResume} />);
  fireEvent.click(await screen.findByText("▶ Devam et"));
  expect(onResume).toHaveBeenCalledWith("s1", "demo");
  expect(await screen.findByText("Devam edilemedi: Bu oturum zaten açık")).toBeTruthy();
});

test("canlı oturumda ▶ Devam et yok", async () => {
  invoke.mockResolvedValue(detail());
  render(<SessionDetailPage sessionId="s1" onBack={() => {}} onResume={vi.fn()} />);
  await screen.findByText("Klasörü aç");
  expect(screen.queryByText("▶ Devam et")).toBeNull();
});

test("aynı anda en fazla bir ayrıntı isteği; bekleyen olay sonra tek istekle karşılanır", async () => {
  const reqs: ReturnType<typeof deferred<SessionDetail>>[] = [];
  invoke.mockImplementation((cmd: string) => {
    if (cmd !== "get_session_detail") return Promise.resolve(undefined);
    const d = deferred<SessionDetail>();
    reqs.push(d);
    return d.promise;
  });
  render(<SessionDetailPage sessionId="s1" onBack={() => {}} />);
  await waitFor(() => expect(snapshotCb).not.toBeNull());
  await act(async () => { snapshotCb!(); snapshotCb!(); });
  expect(detailCalls()).toBe(1);
  await act(async () => { reqs[0].resolve(detail({ card: { ...detail().card, name: "eski" } })); });
  await screen.findByText(/eski/);
  await waitFor(() => expect(detailCalls()).toBe(2));
  await act(async () => { reqs[1].resolve(detail({ card: { ...detail().card, name: "yeni" } })); });
  await screen.findByText(/yeni/);
  expect(detailCalls()).toBe(2);
});

test("eski oturumun geç gelen yanıtı yenisinin üzerine yazmaz", async () => {
  const byId: Record<string, ReturnType<typeof deferred<SessionDetail>>> = {};
  invoke.mockImplementation((cmd: string, a: { sessionId: string }) => {
    if (cmd !== "get_session_detail") return Promise.resolve(undefined);
    byId[a.sessionId] = deferred<SessionDetail>();
    return byId[a.sessionId].promise;
  });
  const { rerender } = render(<SessionDetailPage sessionId="s1" onBack={() => {}} />);
  rerender(<SessionDetailPage sessionId="s2" onBack={() => {}} />);
  await act(async () => { byId.s2.resolve(detail({ card: { ...detail().card, name: "ikinci" } })); });
  await screen.findByText(/ikinci/);
  await act(async () => { byId.s1.resolve(detail({ card: { ...detail().card, name: "birinci" } })); });
  expect(screen.queryByText(/birinci/)).toBeNull();
});

test("Klasörü aç hatası kodu satır içinde Türkçe gösterilir", async () => {
  invoke.mockImplementation((cmd: string) =>
    cmd === "open_folder" ? Promise.reject("folder_unknown") : Promise.resolve(detail()));
  render(<SessionDetailPage sessionId="s1" onBack={() => {}} />);
  fireEvent.click(await screen.findByText("Klasörü aç"));
  expect(await screen.findByText("Klasör açılamadı: Bilinmeyen klasör")).toBeTruthy();
});

const row = (id: string, cwd: string, withSummary: boolean): HistoryRow => ({
  sessionId: id, modifiedMs: 0, size: 1, name: `ad-${id}`, cwd,
  summary: withSummary ? {
    name: `ad-${id}`, cwd, model: "Opus 5.5", firstAtMs: 0, lastAtMs: 600_000,
    agentsTotal: 2, costUsd: 1.5, costByDay: {},
  } : null,
});

test('Geçmiş: summary null satırda model/maliyet/ajan "?"', async () => {
  invoke.mockResolvedValue({ rows: [row("a", "C:/a", false)], todayCost: 0, todayComplete: false, indexing: false });
  render(<History onOpen={() => {}} />);
  const tr = (await screen.findByText("ad-a")).closest("tr")!;
  expect(within(tr).getAllByText("?").length).toBeGreaterThanOrEqual(4);
  expect(within(tr).queryByText("$1.50")).toBeNull();
});

test("Geçmiş: klasör filtresi satırları süzer", async () => {
  invoke.mockResolvedValue({
    rows: [row("a", "C:/a", true), row("b", "C:/b", true)], todayCost: 0, todayComplete: true, indexing: false,
  });
  const onOpen = vi.fn();
  render(<History onOpen={onOpen} />);
  await screen.findByText("ad-a");
  expect(screen.getByText("ad-b")).toBeTruthy();
  fireEvent.change(screen.getByLabelText("Klasör filtresi"), { target: { value: "C:/b" } });
  expect(screen.queryByText("ad-a")).toBeNull();
  fireEvent.click(screen.getByText("ad-b"));
  expect(onOpen).toHaveBeenCalledWith("b");
});

test("Geçmiş: indexing true iken 2 saniyede bir yenilenir", async () => {
  vi.useFakeTimers({ shouldAdvanceTime: true });
  try {
    invoke.mockResolvedValue({ rows: [], todayCost: 0, todayComplete: false, indexing: true });
    render(<History onOpen={() => {}} />);
    await screen.findByText("Dizinleniyor…");
    const n = invoke.mock.calls.length;
    await act(async () => { await vi.advanceTimersByTimeAsync(2100); });
    expect(invoke.mock.calls.length).toBeGreaterThan(n);
  } finally {
    vi.useRealTimers();
  }
});

test("Geçmiş: ▶ Devam et satırı açmadan onResume çağırır; hata o satırda gösterilir", async () => {
  invoke.mockResolvedValue({
    rows: [row("a", "C:/a", true), row("b", "C:/b", false)], todayCost: 0, todayComplete: true, indexing: false,
  });
  const onOpen = vi.fn();
  const onResume = vi.fn().mockRejectedValueOnce("session_unknown").mockResolvedValue(undefined);
  render(<History onOpen={onOpen} onResume={onResume} />);
  const trB = (await screen.findByText("ad-b")).closest("tr")!;
  fireEvent.click(within(trB).getByText("▶ Devam et"));
  expect(onResume).toHaveBeenCalledWith("b", "ad-b");
  expect(onOpen).not.toHaveBeenCalled();
  expect(await within(trB).findByText("Bilinmeyen oturum ya da klasörü yok")).toBeTruthy();
  const trA = screen.getByText("ad-a").closest("tr")!;
  expect(within(trA).queryByText("Bilinmeyen oturum ya da klasörü yok")).toBeNull();
  fireEvent.click(within(trA).getByText("▶ Devam et"));
  expect(onResume).toHaveBeenLastCalledWith("a", "ad-a");
});

test("▶ Devam et istek sürerken devre dışı (ayrıntı ve geçmiş)", async () => {
  const d1 = deferred<void>();
  invoke.mockResolvedValue(closed());
  const onResume = vi.fn(() => d1.promise);
  const r = render(<SessionDetailPage sessionId="s1" onBack={() => {}} onResume={onResume} />);
  const btn = (await screen.findByText("▶ Devam et")) as HTMLButtonElement;
  fireEvent.click(btn);
  expect(btn.disabled).toBe(true);
  fireEvent.click(btn);
  expect(onResume).toHaveBeenCalledTimes(1);
  await act(async () => { d1.resolve(); });
  await waitFor(() => expect(btn.disabled).toBe(false));
  r.unmount();

  const d2 = deferred<void>();
  invoke.mockResolvedValue({
    rows: [row("a", "C:/a", true), row("b", "C:/b", false)], todayCost: 0, todayComplete: true, indexing: false,
  });
  const onResume2 = vi.fn(() => d2.promise);
  render(<History onOpen={() => {}} onResume={onResume2} />);
  const trA = (await screen.findByText("ad-a")).closest("tr")!;
  const a = within(trA).getByText("▶ Devam et") as HTMLButtonElement;
  fireEvent.click(a);
  expect(a.disabled).toBe(true);
  fireEvent.click(a);
  expect(onResume2).toHaveBeenCalledTimes(1);
  await act(async () => { d2.reject("x"); });
  await waitFor(() => expect(a.disabled).toBe(false));
});

test("ayrıntı sayfası İngilizce: etiketler, ağaç, çalışan çağrı, hata kodu", async () => {
  setLang("en");
  invoke.mockImplementation((cmd: string) =>
    cmd === "open_folder" ? Promise.reject("folder_network") : Promise.resolve(detail()));
  render(<SessionDetailPage sessionId="s1" onBack={() => {}} />);
  fireEvent.click(await screen.findByText("Open folder"));
  expect(await screen.findByText("Could not open folder: Network paths are not supported")).toBeTruthy();
  expect(screen.getByText("AGENT TREE")).toBeTruthy();
  expect(screen.getByText("RECENT TOOL CALLS · main session")).toBeTruthy();
  expect(screen.getByTestId("node-ph").textContent).toContain("Phase Review");
  expect(screen.getAllByTestId("tool-row")[2].textContent).toContain("… running");
  expect(screen.getByText("agents / finished")).toBeTruthy();
  expect(screen.getByText("working")).toBeTruthy();
});

test("Geçmiş Azerbaycanca: başlıklar ve tarih yerel ayarla", async () => {
  setLang("az");
  invoke.mockResolvedValue({ rows: [row("a", "C:/a", true)], todayCost: 0, todayComplete: true, indexing: false });
  render(<History onOpen={() => {}} onResume={vi.fn()} />);
  const trA = (await screen.findByText("ad-a")).closest("tr")!;
  const heads = [...document.querySelectorAll("th")].map((th) => th.textContent).filter(Boolean);
  expect(heads).toEqual(["Tarix", "Ad", "Qovluq", "Model", "Müddət", "Agent", "Xərc"]);
  expect(within(trA).getByText("10 dəq")).toBeTruthy();
  expect(within(trA).getByText("▶ Davam et")).toBeTruthy();
  expect(within(trA).getByText(new Date(0).toLocaleString("az-Latn-AZ"))).toBeTruthy();
  expect(screen.getByLabelText("Qovluq filtri")).toBeTruthy();
});

test("devam ettiren terminal açıksa: ayrıntıda uyarı ve Sekmeye git o terminale geçer", async () => {
  invoke.mockResolvedValue(closed());
  const onTerminal = vi.fn();
  const onResume = vi.fn().mockRejectedValueOnce("session_resume_terminal_open:5");
  render(<SessionDetailPage sessionId="s1" onBack={() => {}} onResume={onResume} onTerminal={onTerminal} />);
  fireEvent.click(await screen.findByText("▶ Devam et"));
  const alert = await screen.findByRole("alert");
  expect(alert.textContent).toContain("Devam edilemedi: Bu oturum, onu devam ettiren terminalde hâlâ açık");
  fireEvent.click(within(alert).getByRole("button", { name: "Sekmeye git" }));
  expect(onTerminal).toHaveBeenCalledWith(5);
});

test("devam ettiren terminal açıksa: geçmiş satırında Sekmeye git o terminale geçer, satırı açmaz", async () => {
  invoke.mockResolvedValue({ rows: [row("a", "C:/a", true)], todayCost: 0, todayComplete: true, indexing: false });
  const onOpen = vi.fn();
  const onTerminal = vi.fn();
  const onResume = vi.fn().mockRejectedValueOnce("session_resume_terminal_open:9");
  render(<History onOpen={onOpen} onResume={onResume} onTerminal={onTerminal} />);
  const tr = (await screen.findByText("ad-a")).closest("tr")!;
  fireEvent.click(within(tr).getByText("▶ Devam et"));
  const alert = await within(tr).findByRole("alert");
  expect(alert.textContent).toContain("Bu oturum, onu devam ettiren terminalde hâlâ açık");
  expect(alert.textContent).not.toContain(":9");
  fireEvent.click(within(alert).getByRole("button", { name: "Sekmeye git" }));
  expect(onTerminal).toHaveBeenCalledWith(9);
  expect(onOpen).not.toHaveBeenCalled();
});
