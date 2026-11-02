import { expect, test } from "vitest";
import type { HistoryRow, SessionCard } from "./api";
import { filterHistory, filterSessions, fold, matchesQuery } from "./filter";

test("fold: büyük/küçük harf ve aksan duyarsız; Türkçe İ/ı/I hepsi i", () => {
  expect(fold("İSTANBUL")).toBe("istanbul");
  expect(fold("Işık")).toBe("isik");
  expect(fold("ışık")).toBe("isik");
  expect(fold("ĞÜŞÖÇ")).toBe("gusoc");
  expect(fold("Café Ə Øre Łódź")).toBe("cafe e ore lodz");
  expect(fold("")).toBe("");
});

test("matchesQuery: her kelime alanlardan birinde geçmeli; boş sorgu her şeyi eşler", () => {
  expect(matchesQuery("", ["x"])).toBe(true);
  expect(matchesQuery("   ", [])).toBe(true);
  expect(matchesQuery("istanbul", ["İSTANBUL projesi"])).toBe(true);
  expect(matchesQuery("ışık opus", ["Işık Sensörü", null, "Opus 5.5"])).toBe(true);
  expect(matchesQuery("ışık sonnet", ["Işık Sensörü", "Opus 5.5"])).toBe(false);
  expect(matchesQuery("kalib", [undefined, null])).toBe(false);
});

const card = (over: Partial<SessionCard>): SessionCard => ({
  sessionId: "s", pid: 1, parentPid: null, name: "demo", cwd: "C:\\x\\demo", status: "busy", waitingFor: null,
  model: "Opus 5.5", contextPct: null, costUsd: 0, agentsTotal: 0, agentsFinished: 0, toolOk: 0, toolErr: 0,
  lastTool: null, startedAtMs: 0, lastActivityMs: 0, gitBranch: null, hostKind: "other", ...over,
});

test("filterSessions: metin + bağımsız durum çipleri (hiçbiri seçili değilse hepsi)", () => {
  const cards = [
    card({ sessionId: "a", name: "Işıl ADC", status: "busy" }),
    card({ sessionId: "b", name: "web", cwd: "D:\\İş\\web", status: "waiting" }),
    card({ sessionId: "c", name: "cli", status: "idle", model: "Sonnet 5.5" }),
    card({ sessionId: "d", name: "build", status: "shell" }),
    card({ sessionId: "e", name: "eski", status: "busy", pid: 0 }),
  ];
  const ids = (f: ReturnType<typeof filterSessions>) => f.map((c) => c.sessionId);
  expect(ids(filterSessions(cards, "", new Set()))).toEqual(["a", "b", "c", "d", "e"]);
  expect(ids(filterSessions(cards, "isil", new Set()))).toEqual(["a"]);
  expect(ids(filterSessions(cards, "is", new Set()))).toEqual(["a", "b"]);
  expect(ids(filterSessions(cards, "sonnet", new Set()))).toEqual(["c"]);
  expect(ids(filterSessions(cards, "", new Set(["waiting", "shell"])))).toEqual(["b", "d"]);
  // A closed session (pid 0) is not "busy".
  expect(ids(filterSessions(cards, "", new Set(["busy"])))).toEqual(["a"]);
  expect(ids(filterSessions(cards, "web", new Set(["idle"])))).toEqual([]);
});

const hrow = (id: string, modifiedMs: number, cost: number | null, over: Partial<HistoryRow> = {}): HistoryRow => ({
  sessionId: id, modifiedMs, size: 0, name: id, cwd: "C--x",
  summary: cost === null ? null : {
    name: id, cwd: `C:\\p\\${id}`, model: id === "b" ? "Sonnet 5.5" : "Opus 5.5", firstAtMs: 0, lastAtMs: 0,
    agentsTotal: 0, costUsd: cost, costByDay: {},
  },
  ...over,
});

test("filterHistory: metin, tarih aralığı (yerel, iki uç dahil), tarihe/maliyete göre sıralama", () => {
  const d = (day: number, h = 12) => new Date(2026, 10, day, h).getTime();
  const rows = [hrow("a", d(1), 2), hrow("b", d(3, 0), 5), hrow("c", d(5, 23), null), hrow("d", d(10), 1)];
  const ids = (r: HistoryRow[]) => r.map((x) => x.sessionId);
  const base = { query: "", from: "", to: "", sort: "date" as const };
  expect(ids(filterHistory(rows, base))).toEqual(["d", "c", "b", "a"]);
  expect(ids(filterHistory(rows, { ...base, sort: "cost" }))).toEqual(["b", "a", "d", "c"]);
  expect(ids(filterHistory(rows, { ...base, from: "2026-11-03", to: "2026-11-05" }))).toEqual(["c", "b"]);
  expect(ids(filterHistory(rows, { ...base, from: "2026-11-04" }))).toEqual(["d", "c"]);
  expect(ids(filterHistory(rows, { ...base, to: "2026-11-01" }))).toEqual(["a"]);
  expect(ids(filterHistory(rows, { ...base, query: "sonnet" }))).toEqual(["b"]);
  // Unindexed rows match on their name / project folder.
  expect(ids(filterHistory(rows, { ...base, query: "c--x" }))).toEqual(["c"]);
  // A reversed range matches nothing rather than everything.
  expect(ids(filterHistory(rows, { ...base, from: "2026-11-09", to: "2026-11-02" }))).toEqual([]);
  // The input is not mutated.
  expect(ids(rows)).toEqual(["a", "b", "c", "d"]);
});
