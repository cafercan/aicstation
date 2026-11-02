import { render, screen, fireEvent } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import SessionCardView from "./components/SessionCard";
import KpiRow from "./components/KpiRow";
import StatusBar from "./components/StatusBar";
import Sidebar from "./components/Sidebar";
import type { SessionCard } from "./api";

function card(over: Partial<SessionCard> = {}): SessionCard {
  return {
    sessionId: "s1", pid: 1, parentPid: null, name: "demo", cwd: "C:/x/demo",
    status: "busy", waitingFor: null, model: "Opus 5.5", contextPct: 62,
    costUsd: 1.2, agentsTotal: 0, agentsFinished: 0, toolOk: 0, toolErr: 0, lastTool: null,
    startedAtMs: 0, lastActivityMs: 0, gitBranch: "main", ...over,
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

test('durum yoksa rozet "?" gösterir', () => {
  render(<SessionCardView card={card({ status: "" })} now={0} onOpen={() => {}} />);
  expect(screen.getByText("?", { selector: ".badge" })).toBeTruthy();
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

test("kart rengi ve Claude işareti durumu izler; kapanmış oturum gri", () => {
  const { container, unmount } = render(<SessionCardView card={card({ status: "idle" })} now={0} onOpen={() => {}} />);
  expect(container.querySelector(".card")?.classList.contains("idle")).toBe(true);
  expect(container.querySelector(".cmark circle")?.getAttribute("fill")).toBe("#ff9f43");
  unmount();
  const closed = render(<SessionCardView card={card({ status: "busy", pid: 0 })} now={0} onOpen={() => {}} />);
  expect(screen.getByText("kapandı")).toBeTruthy();
  expect(closed.container.querySelector(".cmark circle")?.getAttribute("fill")).toBe("#7d8696");
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

test('KPI bugünün maliyeti tamamlanmadıysa "hesaplanıyor"', () => {
  const snap = { sessions: [card()], runningAgents: 2, waitingCount: 1 };
  const { unmount } = render(<KpiRow snapshot={snap} history={{ rows: [], todayCost: 3.4216, todayComplete: false, indexing: true }} />);
  expect(screen.getByText("hesaplanıyor")).toBeTruthy();
  unmount();
  render(<KpiRow snapshot={snap} history={{ rows: [], todayCost: 3.4216, todayComplete: true, indexing: false }} />);
  expect(screen.getByText("$3.42")).toBeTruthy();
});

test('status bar seçili oturum yokken "N açık oturum"', () => {
  render(<StatusBar sessions={[card(), card({ sessionId: "s2" })]} selectedId={null} />);
  expect(screen.getByText("2 açık oturum")).toBeTruthy();
});

test("status bar durum etiketini renkli ve bağlamı sınırlı gösterir", () => {
  render(<StatusBar sessions={[card({ status: "waiting", contextPct: 250 })]} selectedId="s1" />);
  const st = screen.getByText("onay bekliyor");
  expect(st.getAttribute("style")).toContain("rgb(255, 93, 115)");
  expect(screen.getByRole("contentinfo").textContent).toContain("ctx 100%");
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

test("kenar çubuğu: Windows Terminal'de aç; hata satır içinde gösterilir", async () => {
  const onOpenWt = vi.fn()
    .mockRejectedValueOnce("Windows Terminal bulunamadı")
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
