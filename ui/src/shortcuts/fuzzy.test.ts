import { expect, test } from "vitest";
import { fuzzyRank, fuzzyScore } from "./fuzzy";

test("fuzzyScore: alt dizi eşleşmesi, harf/aksan duyarsız; eşleşmezse null", () => {
  expect(fuzzyScore("", "her şey")).toBe(0);
  expect(fuzzyScore("ynt", "Yeni terminal")).not.toBeNull();
  expect(fuzzyScore("gecmis", "Geçmişe git")).not.toBeNull();
  expect(fuzzyScore("ISIK", "ışık sensörü")).not.toBeNull();
  expect(fuzzyScore("xyz", "Yeni terminal")).toBeNull();
  expect(fuzzyScore("terminalx", "terminal")).toBeNull();
});

test("fuzzyScore: bitişik ve kelime başı eşleşme daha yüksek puan alır", () => {
  const prefix = fuzzyScore("ter", "terminal kapat")!;
  const scattered = fuzzyScore("ter", "the error")!;
  expect(prefix).toBeGreaterThan(scattered);
  const wordStart = fuzzyScore("tk", "terminal kapat")!;
  const mid = fuzzyScore("tk", "atkı")!;
  expect(wordStart).toBeGreaterThan(mid);
});

test("fuzzyRank: eşleşmeyenler düşer, en iyi önce; eşit puanda sıra korunur", () => {
  const items = ["Yeni terminal", "Terminali kapat", "Geçmiş", "Dashboard"];
  expect(fuzzyRank(items, "term", (s) => s)).toEqual(["Terminali kapat", "Yeni terminal"]);
  expect(fuzzyRank(items, "", (s) => s)).toEqual(items);
  expect(fuzzyRank(items, "qq", (s) => s)).toEqual([]);
});
