import { act, render, screen, fireEvent } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import SessionCardView from "./components/SessionCard";
import KpiRow from "./components/KpiRow";
import StatusBar from "./components/StatusBar";
import Sidebar from "./components/Sidebar";
import GeneralPrefs from "./components/GeneralPrefs";
import type { SessionCard } from "./api";
import { getLang, setLang } from "./i18n";
import { getTheme } from "./theme";
import { setOsForTests } from "./platform";

function card(over: Partial<SessionCard> = {}): SessionCard {
  return {
    sessionId: "s1", pid: 1, parentPid: null, name: "demo", cwd: "C:/x/demo",
    status: "busy", waitingFor: null, model: "Opus 5.5", contextPct: 62,
    costUsd: 1.2, tokens: { input: 1_000_000, output: 200_000, cacheRead: 0, cacheCreation: 0, cacheCreation1h: 0 }, agentsTotal: 0, agentsFinished: 0, toolOk: 0, toolErr: 0, lastTool: null,
    startedAtMs: 0, lastActivityMs: 0, gitBranch: "main", hostKind: "other", ...over,
  };
}

test("kart durum rozetleri", () => {
  const cases: [string, string][] = [
    ["busy", "çalışıyor"], ["waiting", "onay bekliyor"], ["idle", "hazır"], ["x", "x"],
  ];
  for (const [status, label] of cases) {
    const { unmount } = render(<SessionCardView card={card({ status })} now={0} onOpen={() => {}} />);
    expect(screen.getByText(label)).toBeTruthy();
    unmount();
  }
});

test("onay bekleyen kartta beklenen şey rozetin yanında görünür", () => {
  const { unmount } = render(<SessionCardView card={card({ status: "waiting", waitingFor: "Bash izni" })} now={0} onOpen={() => {}} />);
  const badge = screen.getByText("onay bekliyor");
  expect(badge.parentElement?.textContent).toContain("Bash izni");
  unmount();
  render(<SessionCardView card={card({ status: "busy", waitingFor: null })} now={0} onOpen={() => {}} />);
  expect(screen.getByText("çalışıyor").parentElement?.textContent).not.toContain("izni");
});

test('durum yoksa rozet "bilinmiyor" gösterir', () => {
  render(<SessionCardView card={card({ status: "" })} now={0} onOpen={() => {}} />);
  expect(screen.getByText("bilinmiyor", { selector: ".badge" })).toBeTruthy();
});

test("halka: çalışan/biten ajanlar, ortada toplam", () => {
  const { container, unmount } = render(
    <SessionCardView card={card({ agentsTotal: 5, agentsFinished: 3 })} now={0} onOpen={() => {}} />);
  expect(screen.getByText("2 ajan çalışıyor")).toBeTruthy();
  expect(screen.getByText("3 ajan tamamlandı")).toBeTruthy();
  expect(screen.queryByText("alt ajan yok")).toBeNull();
  expect(screen.getByText("5")).toBeTruthy();
  expect(screen.getByText("ajan")).toBeTruthy();
  const segs = container.querySelectorAll("circle[data-seg]");
  expect([...segs].map((c) => [c.getAttribute("data-seg"), c.getAttribute("stroke"), c.getAttribute("stroke-dasharray")]))
    .toEqual([["finished", "#48c774", "60 40"], ["running", "#5aa0ff", "40 60"]]);
  unmount();
  const empty = render(<SessionCardView card={card({ agentsTotal: 0 })} now={0} onOpen={() => {}} />);
  expect(screen.getByText("0")).toBeTruthy();
  expect(empty.container.querySelectorAll("circle[data-seg]")).toHaveLength(0);
  // No subagents at all: one muted line instead of two zero lines.
  const none = screen.getByText("alt ajan yok");
  expect(none.closest(".muted")).toBeTruthy();
  expect(screen.queryByText(/ajan çalışıyor/)).toBeNull();
  expect(screen.queryByText(/ajan tamamlandı/)).toBeNull();
});

test("tool başarı oranı yalnız biten çağrılardan", () => {
  const { unmount } = render(<SessionCardView card={card({ toolOk: 47, toolErr: 3 })} now={0} onOpen={() => {}} />);
  expect(screen.getByText("94%")).toBeTruthy();
  unmount();
  render(<SessionCardView card={card({ toolOk: 0, toolErr: 0 })} now={0} onOpen={() => {}} />);
  expect(screen.getByText("—")).toBeTruthy();
});

test("kart rengi ve oturum işareti durumu izler; kapanmış oturum gri", () => {
  const { container, unmount } = render(<SessionCardView card={card({ status: "idle" })} now={0} onOpen={() => {}} />);
  expect(container.querySelector(".card")?.classList.contains("idle")).toBe(true);
  expect(container.querySelector(".smark circle")?.getAttribute("fill")).toBe("#ff9f43");
  unmount();
  const closed = render(<SessionCardView card={card({ status: "busy", pid: 0 })} now={0} onOpen={() => {}} />);
  expect(screen.getByText("kapandı")).toBeTruthy();
  expect(closed.container.querySelector(".smark circle")?.getAttribute("fill")).toBe("#7d8696");
});

test("bağlam yüzdesi 100'ü aşmaz", () => {
  render(<SessionCardView card={card({ contextPct: 130 })} now={0} onOpen={() => {}} />);
  expect(screen.getByText("bağlam 100%")).toBeTruthy();
});

test("kart klavyeden Enter ve Boşluk ile açılır", () => {
  const onOpen = vi.fn();
  render(<SessionCardView card={card()} now={0} onOpen={onOpen} />);
  const el = screen.getByRole("button");
  fireEvent.keyDown(el, { key: "Enter" });
  fireEvent.keyDown(el, { key: " " });
  expect(onOpen).toHaveBeenCalledTimes(2);
});

test("kenar çubuğu: Claude oturumu olan terminalde durum renkli işaret", () => {
  render(<Sidebar view="dashboard" onSelect={() => {}} onNew={() => {}} onClose={() => {}}
    onOpenWt={async () => {}}
    terminals={[{ id: 1, label: "projem", status: "idle" }, { id: 2, label: "PowerShell 2", status: null }]} />);
  const mark = screen.getByRole("img", { name: "Claude: hazır" });
  expect(mark.querySelector("circle")?.getAttribute("fill")).toBe("#ff9f43");
  expect(screen.getAllByRole("img")).toHaveLength(1);
});

test("bilinmeyen değerler ? gösterir", () => {
  render(<SessionCardView card={card({ contextPct: null, model: null })} now={0} onOpen={() => {}} />);
  expect(screen.getByText("bağlam ?")).toBeTruthy();
  expect(screen.getByText("?")).toBeTruthy();
});

test("kart tıklanınca onOpen çağrılır", () => {
  const onOpen = vi.fn();
  render(<SessionCardView card={card()} now={0} onOpen={onOpen} />);
  fireEvent.click(screen.getByText("demo"));
  expect(onOpen).toHaveBeenCalledWith("s1");
});

test('KPI bugünün token sayısı birincil, $ tahmini ikincil; tamamlanmadıysa "hesaplanıyor"', () => {
  const snap = { sessions: [card()], runningAgents: 2, waitingCount: 1 };
  const { unmount } = render(<KpiRow snapshot={snap} history={{ rows: [], todayCost: 3.4216, todayTokens: 5, todayComplete: false, indexing: true }} />);
  expect(screen.getByText("hesaplanıyor")).toBeTruthy();
  expect(screen.queryByText(/\$3/)).toBeNull();
  unmount();
  const { unmount: u2 } = render(<KpiRow snapshot={snap} history={{ rows: [], todayCost: 3.4216, todayTokens: 182_400_000, todayComplete: true, indexing: false }} />);
  const tile = screen.getByText("bugünkü token").closest(".kpi")!;
  expect(tile.querySelector("b")!.textContent).toBe("182,4M");
  const est = tile.querySelector(".est")!;
  expect(est.textContent).toBe("~$3.42 tah.");
  u2();
  // Unknown model: tokens but no cost, so no dollar part at all (no "$0").
  render(<KpiRow snapshot={snap} history={{ rows: [], todayCost: 0, todayTokens: 950, todayComplete: true, indexing: false }} />);
  const t3 = screen.getByText("bugünkü token").closest(".kpi")!;
  expect(t3.querySelector("b")!.textContent).toBe("950");
  expect(t3.querySelector(".est")).toBeNull();
});

test("kart altbilgisi: token sayısı birincil (ipucunda dört tür), ~$ soluk; bilinmeyen modelde $ yok", () => {
  const tokens = { input: 1_000_000, output: 200_000, cacheRead: 300, cacheCreation: 40, cacheCreation1h: 0 };
  const { container, rerender } = render(<SessionCardView card={card({ tokens })} now={0} onOpen={() => {}} />);
  const row = container.querySelector(".card .row")!;
  expect(row.textContent).toContain("1,2M token");
  expect(row.querySelector(".est")!.textContent).toBe("~$1.20 tah.");
  expect(row.querySelector("[title]")!.getAttribute("title"))
    .toBe("girdi 1.000.000 · çıktı 200.000 · önbellek okuma 300 · önbellek yazma 40");
  rerender(<SessionCardView card={card({ tokens, costUsd: 0 })} now={0} onOpen={() => {}} />);
  expect(container.querySelector(".card .row .est")).toBeNull();
  expect(container.querySelector(".card .row")!.textContent).not.toContain("$");
});

test('status bar seçili oturum yokken "N açık oturum"', () => {
  render(<StatusBar sessions={[card(), card({ sessionId: "s2" })]} selectedId={null} />);
  expect(screen.getByText("2 açık oturum")).toBeTruthy();
});

test("status bar durum etiketini renkli ve bağlamı sınırlı gösterir", () => {
  render(<StatusBar sessions={[card({ status: "waiting", contextPct: 250 })]} selectedId="s1" />);
  const st = screen.getByText("onay bekliyor");
  // Text tone of the status (readable on the status bar in every theme).
  expect(st.getAttribute("style")).toContain("rgb(255, 129, 147)");
  expect(screen.getByRole("contentinfo").textContent).toContain("bağlam 100%");
});

test("status bar bağlam etiketi sözlükten gelir ve dil değişince güncellenir", () => {
  render(<StatusBar sessions={[card({ contextPct: 40 })]} selectedId="s1" />);
  expect(screen.getByRole("contentinfo").textContent).toContain("bağlam 40%");
  act(() => setLang("en"));
  expect(screen.getByRole("contentinfo").textContent).toContain("ctx 40%");
  act(() => setLang("az"));
  expect(screen.getByRole("contentinfo").textContent).toContain("kontekst 40%");
  act(() => setLang("tr"));
});

test("status bar seçili oturumu gösterir", () => {
  render(<StatusBar sessions={[card()]} selectedId="s1" />);
  expect(screen.getByText(/Opus 5.5/)).toBeTruthy();
  expect(screen.getByText(/demo/)).toBeTruthy();
});

test("status bar oturum adı yerine klasör adını gösterir", () => {
  render(<StatusBar sessions={[card({ name: "oturum-adi", cwd: "C:\\isler\\pusula\\" })]} selectedId="s1" />);
  const sb = screen.getByRole("contentinfo");
  expect(sb.textContent).toContain("pusula");
  expect(sb.textContent).not.toContain("oturum-adi");
});

test("kenar çubuğu (Linux): sistem terminalinde aç; hata kodu çevrilir", async () => {
  setOsForTests("linux");
  const onOpenWt = vi.fn().mockRejectedValueOnce("terminal_not_found");
  render(<Sidebar view="dashboard" onSelect={() => {}} onNew={() => {}} onClose={() => {}}
    onOpenWt={onOpenWt} terminals={[]} />);
  expect(screen.queryByText("↗ Windows Terminal'de aç")).toBeNull();
  fireEvent.click(screen.getByText("↗ Sistem terminalinde aç"));
  expect((await screen.findByRole("alert")).textContent).toBe("Terminal uygulaması bulunamadı ($TERMINAL ayarlayın)");
});

test("kenar çubuğu: Windows Terminal'de aç; hata satır içinde gösterilir", async () => {
  const onOpenWt = vi.fn()
    .mockRejectedValueOnce("wt_not_found")
    .mockResolvedValueOnce(undefined);
  render(<Sidebar view="dashboard" onSelect={() => {}} onNew={() => {}} onClose={() => {}}
    onOpenWt={onOpenWt} terminals={[]} />);
  const btn = screen.getByText("↗ Windows Terminal'de aç");
  fireEvent.click(btn);
  expect(onOpenWt).toHaveBeenCalledTimes(1);
  expect((await screen.findByRole("alert")).textContent).toBe("Windows Terminal bulunamadı");
  fireEvent.click(btn);
  await vi.waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
});

test('kart: "Sekmeye git" simgesi yalnızca onGoto verildiğinde görünür ve kartı açmaz', async () => {
  const onOpen = vi.fn();
  const onGoto = vi.fn(async () => null);
  const { unmount } = render(<SessionCardView card={card()} now={0} onOpen={onOpen} />);
  expect(screen.queryByRole("button", { name: "Sekmeye git" })).toBeNull();
  unmount();
  render(<SessionCardView card={card()} now={0} onOpen={onOpen} onGoto={onGoto} />);
  fireEvent.click(screen.getByRole("button", { name: "Sekmeye git" }));
  expect(onGoto).toHaveBeenCalledTimes(1);
  expect(onOpen).not.toHaveBeenCalled();
});

test("kart: Sekmeye git notu kartta gösterilir", async () => {
  const onGoto = vi.fn(async () => "tab_ambiguous");
  render(<SessionCardView card={card()} now={0} onOpen={() => {}} onGoto={onGoto} />);
  fireEvent.click(screen.getByRole("button", { name: "Sekmeye git" }));
  expect(await screen.findByText("Pencere öne getirildi; aynı adlı birden çok sekme var")).toBeTruthy();
});

test("kart: Sekmeye git notu role=alert, ~6 sn sonra kaybolur", async () => {
  vi.useFakeTimers();
  try {
    const onGoto = vi.fn(async () => "tab_ambiguous");
    render(<SessionCardView card={card()} now={0} onOpen={() => {}} onGoto={onGoto} />);
    await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Sekmeye git" })); });
    expect(screen.getByRole("alert").textContent).toBe("Pencere öne getirildi; aynı adlı birden çok sekme var");
    await act(async () => { await vi.advanceTimersByTimeAsync(5900); });
    expect(screen.queryByRole("alert")).not.toBeNull();
    await act(async () => { await vi.advanceTimersByTimeAsync(200); });
    expect(screen.queryByRole("alert")).toBeNull();
  } finally {
    vi.useRealTimers();
  }
});

test("kart: hata notu bir sonraki başarılı Sekmeye git ile temizlenir", async () => {
  const onGoto = vi.fn<() => Promise<string | null>>()
    .mockRejectedValueOnce(new Error("Sekmeye gidilemedi: Sekme bulunamadı"))
    .mockResolvedValueOnce(null);
  render(<SessionCardView card={card()} now={0} onOpen={() => {}} onGoto={onGoto} />);
  fireEvent.click(screen.getByRole("button", { name: "Sekmeye git" }));
  expect((await screen.findByRole("alert")).textContent).toBe("Sekmeye gidilemedi: Sekme bulunamadı");
  await act(async () => { fireEvent.click(screen.getByRole("button", { name: "Sekmeye git" })); });
  expect(onGoto).toHaveBeenCalledTimes(2);
  expect(screen.queryByRole("alert")).toBeNull();
});

const sidebar = (over: Partial<Parameters<typeof Sidebar>[0]> = {}) => (
  <Sidebar view="dashboard" onSelect={() => {}} onNew={() => {}} onClose={() => {}} onOpenWt={async () => {}}
    terminals={[{ id: 1, label: "projem", status: "shell" }]} {...over} />
);

test("dil seçici: sekiz dil, seçim anında uygulanır ve kalıcıdır", () => {
  render(<>{sidebar()}<GeneralPrefs /></>);
  expect(screen.getByText("Geçmiş")).toBeTruthy();
  const sel = screen.getByRole("combobox", { name: "Dil" }) as HTMLSelectElement;
  expect(sel.value).toBe("tr");
  expect([...sel.options].map((o) => o.textContent)).toEqual(
    ["Türkçe", "Azərbaycan", "English", "Deutsch", "Русский", "Español", "Français", "简体中文"]);
  fireEvent.change(sel, { target: { value: "en" } });
  expect(getLang()).toBe("en");
  expect(localStorage.getItem("station.lang")).toBe("en");
  expect(document.documentElement.lang).toBe("en");
  expect(screen.getByText("History")).toBeTruthy();
  expect(screen.getByText("＋ New terminal")).toBeTruthy();
  expect(screen.getByRole("img", { name: "Claude: running command" })).toBeTruthy();
  expect((screen.getByRole("combobox", { name: "Language" }) as HTMLSelectElement).value).toBe("en");
  fireEvent.change(screen.getByRole("combobox", { name: "Language" }), { target: { value: "az" } });
  expect(screen.getByText("Tarixçə")).toBeTruthy();
  expect(screen.getByTitle("Terminalı bağla")).toBeTruthy();
  fireEvent.change(screen.getByRole("combobox", { name: "Dil" }), { target: { value: "zh" } });
  expect(localStorage.getItem("station.lang")).toBe("zh");
  expect(screen.getByRole("img", { name: "Claude：正在执行命令" })).toBeTruthy();
});

test("dil seçici: kayıtlı dil açılışta seçili gelir", () => {
  setLang("ru");
  render(<GeneralPrefs />);
  expect((screen.getByRole("combobox") as HTMLSelectElement).value).toBe("ru");
});

test("tema kalıpları: altı yuvarlak düğme, ipucu çevrilir, tıklanınca tema uygulanır", () => {
  render(<GeneralPrefs />);
  const group = screen.getByRole("group", { name: "Tema" });
  const sw = group.querySelectorAll("button.theme-swatch");
  expect(sw).toHaveLength(6);
  const sepia = screen.getByRole("button", { name: "Sepya" });
  expect(sepia.getAttribute("title")).toBe("Sepya");
  expect(sepia.getAttribute("style")).toContain("--sw-bg: #f4ecd8");
  expect(sepia.getAttribute("style")).toContain("--sw-ac: #a15c2b");
  expect(screen.getByRole("button", { name: "Station Koyu" }).classList.contains("on")).toBe(true);
  fireEvent.click(sepia);
  expect(getTheme()).toBe("sepia-light");
  expect(document.documentElement.dataset.theme).toBe("sepia-light");
  expect(localStorage.getItem("station.theme")).toBe("sepia-light");
  expect(sepia.classList.contains("on")).toBe(true);
  expect(sepia.getAttribute("aria-pressed")).toBe("true");
  act(() => setLang("en"));
  expect(screen.getByRole("button", { name: "Sepia" })).toBeTruthy();
  expect(screen.getByRole("group", { name: "Theme" })).toBeTruthy();
});

test("dil değişince kart, KPI ve status bar yeniden çizilir", () => {
  const snap = { sessions: [card({ status: "shell", agentsTotal: 3, agentsFinished: 2, startedAtMs: 1_000 })], runningAgents: 1, waitingCount: 0 };
  render(<>
    <SessionCardView card={snap.sessions[0]} now={91_000} onOpen={() => {}} />
    <KpiRow snapshot={snap} history={null} />
    <StatusBar sessions={snap.sessions} selectedId={null} />
  </>);
  expect(screen.getByText("komut çalıştırıyor")).toBeTruthy();
  expect(screen.getByText("1 ajan çalışıyor")).toBeTruthy();
  expect(screen.getByText("çalışan ajan")).toBeTruthy();
  expect(screen.getByText("1 açık oturum")).toBeTruthy();
  act(() => setLang("en"));
  expect(screen.getByText("running command")).toBeTruthy();
  expect(screen.getByText("1 agent running")).toBeTruthy();
  expect(screen.getByText("2 agents finished")).toBeTruthy();
  expect(screen.getByText("running agents")).toBeTruthy();
  expect(screen.getByText("1 open session")).toBeTruthy();
  expect(document.querySelector(".card .row")!.textContent).toContain("1.2M tokens");
  expect(document.querySelector(".card .row .est")!.textContent).toBe("~$1.20 est.");
  act(() => setLang("az"));
  expect(screen.getByText("əmr icra edir")).toBeTruthy();
  expect(screen.getByText("1 agent işləyir")).toBeTruthy();
  expect(screen.getByRole("img", { name: "3 agent: 1 işləyir, 2 bitib" })).toBeTruthy();
});

test('kart: "shell" durumunda yeşil şerit ve rozet', () => {
  const { container } = render(<SessionCardView card={card({ status: "shell" })} now={0} onOpen={() => {}} />);
  expect(container.querySelector(".card")?.classList.contains("shell")).toBe(true);
  expect(container.querySelector(".smark circle")?.getAttribute("fill")).toBe("#48c774");
});

test("Windows Terminal hata kodu seçili dilde gösterilir", async () => {
  setLang("en");
  render(sidebar({ onOpenWt: () => Promise.reject("wt_not_found") }));
  fireEvent.click(screen.getByText("↗ Open in Windows Terminal"));
  expect((await screen.findByRole("alert")).textContent).toBe("Windows Terminal not found");
});

test("oturum işareti: durum renkli daire ve nötr bir üçgen; yıldız glifi yok", () => {
  const { container } = render(<SessionCardView card={card({ status: "idle" })} now={0} onOpen={() => {}} />);
  const mark = container.querySelector("svg.smark")!;
  expect(mark.querySelector("circle")?.getAttribute("fill")).toBe("#ff9f43");
  expect(mark.querySelector("text")).toBeNull();
  expect(mark.querySelector("path")).toBeTruthy();
  expect(mark.textContent ?? "").not.toMatch(/[\u2731-\u274B]/);
});

test("hakkında: ürün adı, slogan ve bağımsızlık notu seçili dilde", () => {
  setLang("en");
  render(sidebar());
  const about = screen.getByRole("contentinfo");
  expect(about.textContent).toContain("AI C-Station");
  expect(about.textContent).toContain("Sessions, agents and terminals for Claude Code");
  const note = "Unofficial; not affiliated with or endorsed by Anthropic. Claude and Claude Code are trademarks of Anthropic.";
  expect(about.textContent).toContain(note);
  expect(about.getAttribute("title")).toBe(note);
  act(() => setLang("tr"));
  expect(screen.getByRole("contentinfo").textContent).toContain("Anthropic");
  expect(screen.getByRole("contentinfo").textContent).toContain("Claude Code için");
});

test("status bar: token sayısı birincil, ~$ soluk; fiyatsız modelde $ yok", () => {
  const c = card({ tokens: { input: 1_000_000, output: 200_000, cacheRead: 0, cacheCreation: 0, cacheCreation1h: 0 } });
  const { container, rerender } = render(<StatusBar sessions={[c]} selectedId="s1" />);
  expect(container.textContent).toContain("1,2M token");
  expect(container.querySelector(".est")!.textContent).toBe("~$1.20 tah.");
  rerender(<StatusBar sessions={[{ ...c, costUsd: 0 }]} selectedId="s1" />);
  expect(container.querySelector(".est")).toBeNull();
  expect(container.textContent).not.toContain("$");
});

test("kenar çubuğu alt bloğu: ok düğmesiyle kapanır/açılır, aria-expanded güncellenir, tercih saklanır", () => {
  const { unmount } = render(sidebar());
  const toggle = screen.getByRole("button", { name: /Ayarlar ve hakkında/ });
  expect(toggle.getAttribute("aria-expanded")).toBe("true");
  expect(toggle.getAttribute("aria-controls")).toBe("side-foot-body");
  expect(document.getElementById("side-foot-body")!.hidden).toBe(false);
  expect(screen.getByRole("button", { name: "⚙ Ayarlar" })).toBeTruthy();
  expect(screen.queryByRole("combobox", { name: "Dil" })).toBeNull();
  expect(screen.getByText("AI C-Station")).toBeTruthy();
  fireEvent.click(toggle);
  expect(toggle.getAttribute("aria-expanded")).toBe("false");
  expect(document.getElementById("side-foot-body")!.hidden).toBe(true);
  expect(screen.queryByRole("button", { name: /^⚙/ })).toBeNull();
  expect(localStorage.getItem("station.footerOpen")).toBe("0");
  // Remounting keeps it closed (the choice lives in the store and in localStorage).
  unmount();
  render(sidebar());
  expect(screen.getByRole("button", { name: /Ayarlar ve hakkında/ }).getAttribute("aria-expanded")).toBe("false");
  fireEvent.click(screen.getByRole("button", { name: /Ayarlar ve hakkında/ }));
  expect(screen.getByRole("button", { name: "⚙ Ayarlar" })).toBeTruthy();
  expect(localStorage.getItem("station.footerOpen")).toBe("1");
});
