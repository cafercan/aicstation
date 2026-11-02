import { expect, test } from "vitest";
import {
  clampPct, donutSegments, fmtContext, fmtCost, fmtTokens, fmtElapsed, folderName, statusColor, statusKind,
  statusLabel, toolSuccess,
} from "./format";

test("format: sayılar ve maliyet", () => {
  expect(fmtTokens(1234)).toBe("1.2k");
  expect(fmtTokens(1_500_000)).toBe("1.50M");
  expect(fmtTokens(12)).toBe("12");
  expect(fmtCost(3.4216)).toBe("$3.42");
});

test("format: geçen süre", () => {
  expect(fmtElapsed(30_000)).toBe("şimdi");
  expect(fmtElapsed(12 * 60_000)).toBe("12 dk");
  expect(fmtElapsed(3 * 3_600_000 + 5 * 60_000)).toBe("3 sa 5 dk");
  expect(fmtElapsed(-5)).toBe("şimdi");
});

test("format: durum etiketi ve klasör adı", () => {
  expect(statusLabel("")).toBe("?");
  expect(statusLabel("busy")).toBe("çalışıyor");
  expect(folderName("C:\\isler\\pusula\\")).toBe("pusula");
  expect(folderName("/home/x/proje")).toBe("proje");
  expect(folderName("")).toBe("");
});

test("durum türü, etiketi ve rengi", () => {
  expect(statusKind("busy", 1)).toBe("busy");
  expect(statusKind("idle", 1)).toBe("idle");
  expect(statusKind("waiting", 1)).toBe("waiting");
  expect(statusKind("busy", 0)).toBe("closed");
  expect(statusKind("garip", 1)).toBe("unknown");
  expect(statusKind("", 1)).toBe("unknown");
  expect(statusKind("idle")).toBe("idle");

  expect(statusLabel("idle")).toBe("hazır");
  expect(statusLabel("waiting")).toBe("onay bekliyor");
  expect(statusLabel("busy", 0)).toBe("kapandı");
  expect(statusLabel("garip")).toBe("garip");

  expect(statusColor("busy")).toBe("#48c774");
  expect(statusColor("idle")).toBe("#ff9f43");
  expect(statusColor("waiting")).toBe("#ff5d73");
  expect(statusColor("closed")).toBe("#7d8696");
  expect(statusColor("unknown")).toBe("#7d8696");
});

test("halka segmentleri: 100 birimlik çevre, saat 12'den başlar", () => {
  expect(donutSegments(0, 0)).toEqual([]);
  expect(donutSegments(2, 3)).toEqual([
    { kind: "finished", color: "#48c774", length: 60, offset: 25 },
    { kind: "running", color: "#5aa0ff", length: 40, offset: -35 },
  ]);
  expect(donutSegments(0, 4)).toEqual([{ kind: "finished", color: "#48c774", length: 100, offset: 25 }]);
  expect(donutSegments(1, 0)).toEqual([{ kind: "running", color: "#5aa0ff", length: 100, offset: 25 }]);
  // negatif girdiler sıfır sayılır
  expect(donutSegments(-1, 1)).toEqual([{ kind: "finished", color: "#48c774", length: 100, offset: 25 }]);
});

test("tool başarı oranı ve bağlam metni", () => {
  expect(toolSuccess(0, 0)).toBe("—");
  expect(toolSuccess(47, 3)).toBe("94%");
  expect(toolSuccess(0, 2)).toBe("0%");
  expect(fmtContext(null)).toBe("?");
  expect(fmtContext(50.7)).toBe("51%");
  expect(fmtContext(130)).toBe("100%");
  expect(fmtContext(-4)).toBe("0%");
  expect(clampPct(null)).toBe(0);
  expect(clampPct(130)).toBe(100);
});
