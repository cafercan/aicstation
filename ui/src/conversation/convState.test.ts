import { expect, test } from "vitest";
import type { ConvItem, ConvPage } from "../api";
import { applyPage, hasOlder, lastIndex, type ConvState } from "./convState";

const a = (text: string): ConvItem => ({ kind: "assistant", text, truncated: false, atMs: 0 });
const page = (epoch: number, start: number, total: number, ...texts: string[]): ConvPage =>
  ({ epoch, start, total, items: texts.map(a) });
const texts = (s: ConvState) => s.items.map((i) => (i.kind === "assistant" ? i.text : "?"));

test("null state: the page replaces", () => {
  const s = applyPage(null, page(1, 5, 8, "e", "f", "g"));
  expect(s).toMatchObject({ epoch: 1, start: 5, total: 8 });
  expect(texts(s)).toEqual(["e", "f", "g"]);
});

test("older page is prepended", () => {
  const s0 = applyPage(null, page(1, 3, 5, "d", "e"));
  const s = applyPage(s0, page(1, 0, 5, "a", "b", "c"));
  expect(s.start).toBe(0);
  expect(texts(s)).toEqual(["a", "b", "c", "d", "e"]);
});

test("older page overlapping s.start only contributes the part before it", () => {
  const s0 = applyPage(null, page(1, 3, 5, "d", "e"));
  const s = applyPage(s0, page(1, 1, 5, "b", "c", "D"));
  expect(s.start).toBe(1);
  expect(texts(s)).toEqual(["b", "c", "d", "e"]);
});

test("since page replaces the last item and appends", () => {
  const s0 = applyPage(null, page(1, 0, 3, "a", "b", "c"));
  const s = applyPage(s0, page(1, 2, 5, "C", "d", "e"));
  expect(s.total).toBe(5);
  expect(texts(s)).toEqual(["a", "b", "C", "d", "e"]);
});

test("epoch change resets", () => {
  const s0 = applyPage(null, page(1, 0, 3, "a", "b", "c"));
  const s = applyPage(s0, page(2, 7, 9, "x", "y"));
  expect(s).toMatchObject({ epoch: 2, start: 7, total: 9 });
  expect(texts(s)).toEqual(["x", "y"]);
});

test("hasOlder and lastIndex", () => {
  const s = applyPage(null, page(1, 4, 6, "e", "f"));
  expect(hasOlder(s)).toBe(true);
  expect(lastIndex(s)).toBe(5);
  const z = applyPage(null, page(1, 0, 0));
  expect(hasOlder(z)).toBe(false);
  expect(lastIndex(z)).toBe(0);
});
