import { expect, test } from "vitest";
import type { HistoryRow, HistorySummary, Usage } from "./api";
import { CHART_TOKENS, dailyUsage, localDateKey, modelSplit, rangeDays, topProjects, totals, withLive } from "./costs";
import { THEME_IDS, THEMES } from "./theme";

const ZERO: Usage = { input: 0, output: 0, cacheRead: 0, cacheCreation: 0, cacheCreation1h: 0 };
const sum = (over: Partial<HistorySummary>): HistorySummary => ({
  name: "x", cwd: null, model: null, firstAtMs: 0, lastAtMs: 0, agentsTotal: 0, costUsd: 0, costByDay: {},
  tokens: ZERO, tokensByDay: {}, ...over,
});
const row = (id: string, s: Partial<HistorySummary> | null, cwd: string | null = null): HistoryRow => ({
  sessionId: id, modifiedMs: 0, size: 0, name: id, cwd, summary: s ? sum(s) : null,
});

/** 2026-11-02 15:00 local. */
const NOW = new Date(2026, 10, 2, 15, 0, 0).getTime();

test("yerel tarih anahtarı ve aralık günleri (eskiden yeniye, son gün bugün)", () => {
  expect(localDateKey(NOW)).toBe("2026-11-02");
  expect(localDateKey(new Date(2026, 0, 5, 0, 1).getTime())).toBe("2026-01-05");
  expect(rangeDays(NOW, 3)).toEqual(["2026-10-31", "2026-11-01", "2026-11-02"]);
  const d30 = rangeDays(NOW, 30);
  expect(d30).toHaveLength(30);
  expect(d30[0]).toBe("2026-10-04");
  expect(new Set(d30).size).toBe(30);
  // Month/year boundaries.
  expect(rangeDays(new Date(2026, 0, 1, 12).getTime(), 2)).toEqual(["2025-12-31", "2026-01-01"]);
});

test("günlük kovalar: aralık dışı günler düşer, bugün işaretli, canlı bugünkü dilim kullanılır", () => {
  const rows = [
    row("a", {
      costByDay: { "2026-11-01": 1.5, "2026-11-02": 0.5, "2026-09-01": 9 },
      tokensByDay: { "2026-11-01": 1500, "2026-11-02": 500, "2026-09-01": 9000 },
    }),
    row("b", { costByDay: { "2026-11-01": 0.25 }, tokensByDay: { "2026-11-01": 250 } }),
    row("c", null),
  ];
  const b = dailyUsage(rows, rangeDays(NOW, 7), NOW);
  expect(b).toHaveLength(7);
  expect(b.map((x) => x.today)).toEqual([false, false, false, false, false, false, true]);
  expect(b[5]).toEqual({ date: "2026-11-01", tokens: 1750, cost: 1.75, today: false });
  expect(b[6]).toMatchObject({ tokens: 500, cost: 0.5 });
  // The backend's today totals (closed + live) replace the indexed today slice, each on its own.
  const t = dailyUsage(rows, rangeDays(NOW, 7), NOW, { tokens: 4000, cost: 4 })[6];
  expect([t.tokens, t.cost]).toEqual([4000, 4]);
  const only = dailyUsage(rows, rangeDays(NOW, 7), NOW, { tokens: 4000 })[6];
  expect([only.tokens, only.cost]).toEqual([4000, 0.5]);
  const none = dailyUsage(rows, rangeDays(NOW, 7), NOW, { tokens: null, cost: null })[6];
  expect([none.tokens, none.cost]).toEqual([500, 0.5]);
});

test("token sayısı fiyatsız (bilinmeyen) modelde de var; maliyet 0 kalır", () => {
  const rows = [row("u", { model: "mystery", costByDay: {}, tokensByDay: { "2026-11-01": 700 } })];
  const b = dailyUsage(rows, rangeDays(NOW, 3), NOW);
  expect(b[1]).toEqual({ date: "2026-11-01", tokens: 700, cost: 0, today: false });
});

test("toplam ve günlük ortalama; boş veri sıfır", () => {
  const b = dailyUsage(
    [row("a", { costByDay: { "2026-11-01": 3, "2026-11-02": 1 }, tokensByDay: { "2026-11-01": 3000, "2026-11-02": 1000 } })],
    rangeDays(NOW, 7), NOW,
  );
  expect(totals(b)).toEqual({ tokens: 4000, cost: 4, avgTokens: 4000 / 7, avgCost: 4 / 7 });
  const zero = { tokens: 0, cost: 0, avgTokens: 0, avgCost: 0 };
  expect(totals([])).toEqual(zero);
  const empty = dailyUsage([], rangeDays(NOW, 30), NOW);
  expect(empty.every((x) => x.tokens === 0 && x.cost === 0)).toBe(true);
  expect(totals(empty)).toEqual(zero);
});

test("en çok token kullanan 5 proje: aralıktaki token'a göre, klasör başına toplanır, $ ikincil", () => {
  const days = new Set(rangeDays(NOW, 7));
  const rows = [
    row("a", { cwd: "C:\\p\\alfa", costByDay: { "2026-11-01": 1 }, tokensByDay: { "2026-11-01": 10 } }),
    row("a2", { cwd: "C:\\p\\alfa", costByDay: { "2026-11-02": 2 }, tokensByDay: { "2026-11-02": 20 } }),
    // Cheap per token but the most tokens: ranked by tokens, not by dollars.
    row("b", { cwd: "C:\\p\\beta", costByDay: { "2026-11-02": 0.5 }, tokensByDay: { "2026-11-02": 500 } }),
    row("c", { cwd: "C:\\p\\gama", costByDay: { "2026-01-01": 100 }, tokensByDay: { "2026-01-01": 100_000 } }),
    row("d", { costByDay: {}, tokensByDay: { "2026-11-02": 5 } }, "C--p-delta"),
    ...["e", "f", "g", "h"].map((id, i) => row(id, { cwd: `C:\\p\\${id}`, tokensByDay: { "2026-11-02": i + 1 } })),
  ];
  const top = topProjects(rows, days);
  expect(top).toHaveLength(5);
  expect(top[0]).toEqual({ cwd: "C:\\p\\beta", name: "beta", tokens: 500, cost: 0.5 });
  expect(top[1]).toEqual({ cwd: "C:\\p\\alfa", name: "alfa", tokens: 30, cost: 3 });
  expect(top[2]).toEqual({ cwd: "C--p-delta", name: "C--p-delta", tokens: 5, cost: 0 });
  expect(top.some((p) => p.name === "gama")).toBe(false);
  expect(topProjects([], days)).toEqual([]);
  expect(topProjects(rows, days, 2)).toHaveLength(2);
});

test("model dağılımı: paylar toplamı 1, sıralı; modelsiz '?' olarak gruplanır; fazlası 'diğer'", () => {
  const days = new Set(rangeDays(NOW, 7));
  const d = "2026-11-02";
  const rows = [
    row("a", { model: "claude-opus-5-5", costByDay: { [d]: 3 }, tokensByDay: { [d]: 600 } }),
    row("b", { model: "claude-sonnet-5-5", costByDay: { [d]: 1 }, tokensByDay: { [d]: 300 } }),
    row("c", { model: null, tokensByDay: { [d]: 100 } }),
    row("d", { model: "claude-opus-5-5", costByDay: { "2026-01-01": 50 }, tokensByDay: { "2026-01-01": 5000 } }),
  ];
  const m = modelSplit(rows, days);
  expect(m.map((x) => x.model)).toEqual(["claude-opus-5-5", "claude-sonnet-5-5", null]);
  expect(m[0].share).toBeCloseTo(0.6);
  expect(m[2]).toMatchObject({ tokens: 100, cost: 0 });
  expect(m.reduce((a, x) => a + x.share, 0)).toBeCloseTo(1);
  expect(modelSplit([], days)).toEqual([]);
  const many = Array.from({ length: 8 }, (_, i) => row(`m${i}`, { model: `m${i}`, tokensByDay: { [d]: 8 - i } }));
  const capped = modelSplit(many, days, 4);
  expect(capped).toHaveLength(5);
  expect(capped[4]).toMatchObject({ model: null, other: true, tokens: 4 + 3 + 2 + 1 });
});

test("canlı oturumlar üç grafikte de aynı şekilde sayılır", () => {
  const days = new Set(rangeDays(NOW, 7));
  const rows = [row("a", {
    cwd: "C:\\p\\alfa", model: "claude-opus-5-5", costByDay: { "2026-11-01": 1, "2026-11-02": 0.5 },
    tokensByDay: { "2026-11-01": 100, "2026-11-02": 50 },
  })];
  const live = [{
    sessionId: "L", cwd: "C:\\p\\canli", model: "claude-sonnet-5-5",
    costByDay: { "2026-11-01": 0.25, "2026-11-02": 2 }, tokensByDay: { "2026-11-01": 25, "2026-11-02": 200 },
  }];
  const all = withLive(rows, live);
  expect(all).toHaveLength(2);
  // Daily bars: yesterday includes the live session; today is the backend's
  // total (closed 50 + live 200), same as summing the merged rows.
  const b = dailyUsage(all, rangeDays(NOW, 7), NOW, { tokens: 250, cost: 2.5 });
  expect(b[5]).toMatchObject({ tokens: 125, cost: 1.25 });
  expect(b[6].tokens).toBe(dailyUsage(all, rangeDays(NOW, 7), NOW)[6].tokens);
  expect(topProjects(all, days).map((p) => [p.name, p.tokens, p.cost])).toEqual([["canli", 225, 2.25], ["alfa", 150, 1.5]]);
  const m = modelSplit(all, days);
  expect(m.map((x) => [x.model, x.tokens])).toEqual([["claude-sonnet-5-5", 225], ["claude-opus-5-5", 150]]);
  // Totals agree across the charts.
  const total = (xs: { tokens: number }[]) => xs.reduce((a, x) => a + x.tokens, 0);
  expect(total(b)).toBeCloseTo(total(topProjects(all, days)));
  expect(total(b)).toBeCloseTo(total(m));
  // No live data, or a live id that is also a row: rows unchanged / not doubled.
  expect(withLive(rows, undefined)).toBe(rows);
  expect(withLive(rows, [{ ...live[0], sessionId: "a" }])).toHaveLength(1);
  // An old fixture without tokensByDay still merges (zero tokens).
  const old = { sessionId: "O", cwd: "C:\\p\\eski", model: null, costByDay: { "2026-11-02": 1 } };
  expect(withLive([], [old])[0].summary!.tokensByDay).toEqual({});
});

test("grafik renkleri: her temada yüzeylerde ≥ 3 kontrast ve seriler birbirinden farklı", () => {
  const lum = (hex: string) => {
    const c = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255)
      .map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
    return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
  };
  const contrast = (a: string, b: string) => {
    const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p);
    return (x + 0.05) / (y + 0.05);
  };
  for (const id of THEME_IDS) {
    const k = THEMES[id].tokens;
    const all = [CHART_TOKENS.bar, CHART_TOKENS.today, ...CHART_TOKENS.series];
    for (const tok of all) {
      for (const under of [k.bg, k.surfaceA, k.surfaceB]) {
        expect(contrast(k[tok], under), `${id} ${tok}`).toBeGreaterThanOrEqual(3);
      }
    }
    expect(new Set(CHART_TOKENS.series.map((t) => k[t])).size, id).toBe(CHART_TOKENS.series.length);
    expect(k[CHART_TOKENS.bar], id).not.toBe(k[CHART_TOKENS.today]);
  }
});
