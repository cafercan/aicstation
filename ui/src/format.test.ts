import { expect, test } from "vitest";
import {
  clampPct, donutSegments, fmtBreakdown, fmtContext, fmtCost, fmtEstCost, fmtTokens, usageSum, fmtElapsed, folderName, statusColor, statusKind,
  statusLabel, statusTextColor, toolSuccess,
} from "./format";
import { setLang } from "./i18n";
import { applyTheme } from "./theme";

test("format: token sayısı kompakt ve yerel ayara göre (en çok bir ondalık)", () => {
  expect(fmtTokens(950, "en")).toBe("950");
  expect(fmtTokens(0, "en")).toBe("0");
  expect(fmtTokens(12_345, "en")).toBe("12.3K");
  expect(fmtTokens(182_400_000, "en")).toBe("182.4M");
  expect(fmtTokens(1_234_567_890, "en")).toBe("1.2B");
  expect(fmtTokens(1_000, "en")).toBe("1K");
  expect(fmtTokens(1234, "tr")).toBe("1,2K");
  expect(fmtTokens(182_400_000, "de")).toBe("182,4M");
  expect(fmtTokens(NaN, "en")).toBe("0");
});

test("format: maliyet ve ikincil ~$ tahmini (fiyatsız modelde yok)", () => {
  expect(fmtCost(3.4216)).toBe("$3.42");
  expect(fmtEstCost(3.4216, "en")).toBe("~$3.42 est.");
  expect(fmtEstCost(456.4, "en")).toBe("~$456 est.");
  expect(fmtEstCost(3.4216, "tr")).toBe("~$3.42 tah.");
  expect(fmtEstCost(0, "en")).toBeNull();
  expect(fmtEstCost(NaN, "en")).toBeNull();
  expect(fmtEstCost(null, "en")).toBeNull();
  expect(fmtEstCost(undefined, "en")).toBeNull();
});

test("format: dört token türünün toplamı ve ipucu dökümü", () => {
  const u = { input: 1000, output: 200, cacheRead: 30000, cacheCreation: 4000, cacheCreation1h: 1000 };
  // cacheCreation1h is part of cacheCreation: never added twice.
  expect(usageSum(u)).toBe(35_200);
  expect(usageSum(null)).toBe(0);
  expect(fmtBreakdown(u, "en")).toBe("input 1,000 · output 200 · cache read 30,000 · cache write 4,000");
  expect(fmtBreakdown(undefined, "en")).toBe("input 0 · output 0 · cache read 0 · cache write 0");
});

test("format: geçen süre", () => {
  expect(fmtElapsed(30_000)).toBe("30 sn");
  expect(fmtElapsed(12_999)).toBe("12 sn");
  expect(fmtElapsed(0)).toBe("0 sn");
  expect(fmtElapsed(59_999)).toBe("59 sn");
  expect(fmtElapsed(60_000)).toBe("1 dk");
  expect(fmtElapsed(12 * 60_000)).toBe("12 dk");
  expect(fmtElapsed(3 * 3_600_000 + 5 * 60_000)).toBe("3 sa 5 dk");
  expect(fmtElapsed(3 * 86_400_000)).toBe("3 gün");
  expect(fmtElapsed(-5)).toBe("0 sn");
});

test("durum etiketi: türler hep Türkçe, ham durum metni olduğu gibi", () => {
  expect(statusLabel("busy", 1)).toBe("çalışıyor");
  expect(statusLabel("waiting", 1)).toBe("onay bekliyor");
  expect(statusLabel("idle", 1)).toBe("hazır");
  expect(statusLabel("busy", 0)).toBe("kapandı");
  expect(statusLabel("", 1)).toBe("bilinmiyor");
  expect(statusLabel("")).toBe("bilinmiyor");
  // A raw status that spells a kind name is never shown in English.
  expect(statusLabel("unknown", 1)).toBe("bilinmiyor");
  expect(statusLabel("closed", 1)).toBe("kapandı");
  expect(statusLabel("compacting", 1)).toBe("compacting");
});

test("format: durum etiketi ve klasör adı", () => {
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

test("geçen süre her dilin birimleriyle", () => {
  const cases: [number, string, string, string][] = [
    [30_000, "30 sn", "30 san", "30 s"],
    [12 * 60_000, "12 dk", "12 dəq", "12 min"],
    [3 * 3_600_000, "3 sa", "3 saat", "3 h"],
    [3 * 3_600_000 + 5 * 60_000, "3 sa 5 dk", "3 saat 5 dəq", "3 h 5 min"],
    [3 * 86_400_000, "3 gün", "3 gün", "3 d"],
  ];
  for (const [ms, trS, azS, enS] of cases) {
    expect(fmtElapsed(ms, "tr")).toBe(trS);
    expect(fmtElapsed(ms, "az")).toBe(azS);
    expect(fmtElapsed(ms, "en")).toBe(enS);
  }
  setLang("en");
  expect(fmtElapsed(60_000)).toBe("1 min");
});

test("durum etiketi her dilde; ham \"shell\" komut çalıştırıyor, meşgul yeşili", () => {
  expect(statusKind("shell", 1)).toBe("shell");
  expect(statusKind("shell", 0)).toBe("closed");
  expect(statusLabel("shell", 1, "tr")).toBe("komut çalıştırıyor");
  expect(statusLabel("shell", 1, "az")).toBe("əmr icra edir");
  expect(statusLabel("shell", 1, "en")).toBe("running command");
  expect(statusLabel("shell", 0, "en")).toBe("closed");
  expect(statusColor("shell")).toBe(statusColor("busy"));
  expect(statusLabel("busy", 1, "az")).toBe("işləyir");
  expect(statusLabel("waiting", 1, "en")).toBe("awaiting approval");
  expect(statusLabel("idle", 1, "en")).toBe("ready");
  expect(statusLabel("", 1, "az")).toBe("naməlum");
  expect(statusLabel("compacting", 1, "en")).toBe("compacting");
  setLang("az");
  expect(statusLabel("idle")).toBe("hazırdır");
});

test("durum ve halka renkleri etkin temayı izler", () => {
  applyTheme("github-light");
  expect(statusColor("busy")).toBe("#1a7f37");
  expect(statusTextColor("waiting")).toBe("#cf222e");
  expect(donutSegments(1, 1).map((s) => s.color)).toEqual(["#1a7f37", "#0969da"]);
  applyTheme("station-dark");
  expect(statusTextColor("waiting")).toBe("#ff8193");
  expect(donutSegments(1, 1).map((s) => s.color)).toEqual(["#48c774", "#5aa0ff"]);
});
