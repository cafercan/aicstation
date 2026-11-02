import { expect, test } from "vitest";
import type { HistoryRow, HistorySummary } from "./api";
import { CHART_TOKENS, dailyCosts, localDateKey, modelSplit, rangeDays, topProjects, totals, withLive } from "./costs";
import { THEME_IDS, THEMES } from "./theme";

const sum = (over: Partial<HistorySummary>): HistorySummary => ({
  name: "x", cwd: null, model: null, firstAtMs: 0, lastAtMs: 0, agentsTotal: 0, costUsd: 0, costByDay: {}, ...over,
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
    row("a", { costByDay: { "2026-11-01": 1.5, "2026-11-02": 0.5, "2026-09-01": 9 } }),
    row("b", { costByDay: { "2026-11-01": 0.25 } }),
    row("c", null),
  ];
  const b = dailyCosts(rows, rangeDays(NOW, 7), NOW);
  expect(b).toHaveLength(7);
  expect(b.map((x) => x.today)).toEqual([false, false, false, false, false, false, true]);
  expect(b[5]).toEqual({ date: "2026-11-01", cost: 1.75, today: false });
  expect(b[6].cost).toBe(0.5);
  // The backend's today total (closed + live) replaces the indexed today slice.
  expect(dailyCosts(rows, rangeDays(NOW, 7), NOW, 4)[6].cost).toBe(4);
  expect(dailyCosts(rows, rangeDays(NOW, 7), NOW, null)[6].cost).toBe(0.5);
});

test("toplam ve günlük ortalama; boş veri sıfır", () => {
  const b = dailyCosts([row("a", { costByDay: { "2026-11-01": 3, "2026-11-02": 1 } })], rangeDays(NOW, 7), NOW);
  expect(totals(b)).toEqual({ total: 4, avg: 4 / 7 });
  expect(totals([])).toEqual({ total: 0, avg: 0 });
  const empty = dailyCosts([], rangeDays(NOW, 30), NOW);
  expect(empty.every((x) => x.cost === 0)).toBe(true);
  expect(totals(empty)).toEqual({ total: 0, avg: 0 });
});

test("en pahalı 5 proje: aralıktaki maliyete göre, klasör başına toplanır", () => {
  const days = new Set(rangeDays(NOW, 7));
  const rows = [
    row("a", { cwd: "C:\\p\\alfa", costByDay: { "2026-11-01": 1 } }),
    row("a2", { cwd: "C:\\p\\alfa", costByDay: { "2026-11-02": 2 } }),
    row("b", { cwd: "C:\\p\\beta", costByDay: { "2026-11-02": 2.5 } }),
    row("c", { cwd: "C:\\p\\gama", costByDay: { "2026-01-01": 100 } }),
    row("d", { costByDay: { "2026-11-02": 0.1 } }, "C--p-delta"),
    ...["e", "f", "g", "h"].map((id, i) => row(id, { cwd: `C:\\p\\${id}`, costByDay: { "2026-11-02": 0.01 * (i + 1) } })),
  ];
  const top = topProjects(rows, days);
  expect(top).toHaveLength(5);
  expect(top[0]).toEqual({ cwd: "C:\\p\\alfa", name: "alfa", cost: 3 });
  expect(top[1].name).toBe("beta");
  expect(top[2]).toEqual({ cwd: "C--p-delta", name: "C--p-delta", cost: 0.1 });
  expect(top.some((p) => p.name === "gama")).toBe(false);
  expect(topProjects([], days)).toEqual([]);
  expect(topProjects(rows, days, 2)).toHaveLength(2);
});

test("model dağılımı: paylar toplamı 1, sıralı; modelsiz '?' olarak gruplanır; fazlası 'diğer'", () => {
  const days = new Set(rangeDays(NOW, 7));
  const rows = [
    row("a", { model: "claude-opus-5-5", costByDay: { "2026-11-02": 3 } }),
    row("b", { model: "claude-sonnet-5-5", costByDay: { "2026-11-02": 1 } }),
    row("c", { model: null, costByDay: { "2026-11-02": 1 } }),
    row("d", { model: "claude-opus-5-5", costByDay: { "2026-01-01": 50 } }),
  ];
  const m = modelSplit(rows, days);
  expect(m.map((x) => x.model)).toEqual(["claude-opus-5-5", "claude-sonnet-5-5", null]);
  expect(m[0].share).toBeCloseTo(0.6);
  expect(m.reduce((a, x) => a + x.share, 0)).toBeCloseTo(1);
  expect(modelSplit([], days)).toEqual([]);
  const many = Array.from({ length: 8 }, (_, i) => row(`m${i}`, { model: `m${i}`, costByDay: { "2026-11-02": 8 - i } }));
  const capped = modelSplit(many, days, 4);
  expect(capped).toHaveLength(5);
  expect(capped[4]).toMatchObject({ model: null, other: true, cost: 4 + 3 + 2 + 1 });
});

test("canlı oturumlar üç grafikte de aynı şekilde sayılır", () => {
  const days = new Set(rangeDays(NOW, 7));
  const rows = [row("a", { cwd: "C:\\p\\alfa", model: "claude-opus-5-5", costByDay: { "2026-11-01": 1, "2026-11-02": 0.5 } })];
  const live = [{ sessionId: "L", cwd: "C:\\p\\canli", model: "claude-sonnet-5-5", costByDay: { "2026-11-01": 0.25, "2026-11-02": 2 } }];
  const all = withLive(rows, live);
  expect(all).toHaveLength(2);
  // Daily bars: yesterday includes the live session; today is the backend's
  // total (closed 0.5 + live 2), same as summing the merged rows.
  const b = dailyCosts(all, rangeDays(NOW, 7), NOW, 2.5);
  expect(b[5].cost).toBe(1.25);
  expect(b[6].cost).toBe(dailyCosts(all, rangeDays(NOW, 7), NOW)[6].cost);
  expect(topProjects(all, days).map((p) => [p.name, p.cost])).toEqual([["canli", 2.25], ["alfa", 1.5]]);
  const m = modelSplit(all, days);
  expect(m.map((x) => [x.model, x.cost])).toEqual([["claude-sonnet-5-5", 2.25], ["claude-opus-5-5", 1.5]]);
  // Totals agree across the charts.
  const sum = (xs: { cost: number }[]) => xs.reduce((a, x) => a + x.cost, 0);
  expect(sum(b)).toBeCloseTo(sum(topProjects(all, days)));
  expect(sum(b)).toBeCloseTo(sum(m));
  // No live data, or a live id that is also a row: rows unchanged / not doubled.
  expect(withLive(rows, undefined)).toBe(rows);
  expect(withLive(rows, [{ ...live[0], sessionId: "a" }])).toHaveLength(1);
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
