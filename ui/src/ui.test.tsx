import { render, screen, fireEvent } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import SessionCardView from "./components/SessionCard";
import KpiRow from "./components/KpiRow";
import StatusBar from "./components/StatusBar";
import type { SessionCard } from "./api";

function card(over: Partial<SessionCard> = {}): SessionCard {
  return {
    sessionId: "s1", pid: 1, parentPid: null, name: "demo", cwd: "C:/x/demo",
    status: "busy", waitingFor: null, model: "Opus 5.5", contextPct: 62,
    costUsd: 1.2, agentsTotal: 0, agentsFinished: 0, lastTool: null,
    startedAtMs: 0, lastActivityMs: 0, gitBranch: "main", ...over,
  };
}

test("kart durum rozetleri", () => {
  const cases: [string, string][] = [
    ["busy", "çalışıyor"], ["waiting", "onay bekliyor"], ["idle", "boşta"], ["x", "x"],
  ];
  for (const [status, label] of cases) {
    const { unmount } = render(<SessionCardView card={card({ status })} now={0} onOpen={() => {}} />);
    expect(screen.getByText(label)).toBeTruthy();
    unmount();
  }
});

test("ajan metni", () => {
  const { unmount } = render(<SessionCardView card={card({ agentsTotal: 3, agentsFinished: 1 })} now={0} onOpen={() => {}} />);
  expect(screen.getByText("3 ajan · 1 bitti")).toBeTruthy();
  unmount();
  render(<SessionCardView card={card({ agentsTotal: 0 })} now={0} onOpen={() => {}} />);
  expect(screen.getByText("ajan yok")).toBeTruthy();
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

test("status bar seçili oturumu gösterir", () => {
  render(<StatusBar sessions={[card()]} selectedId="s1" />);
  expect(screen.getByText(/Opus 5.5/)).toBeTruthy();
  expect(screen.getByText(/demo/)).toBeTruthy();
});
