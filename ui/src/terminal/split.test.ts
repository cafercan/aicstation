import { expect, test } from "vitest";
import {
  MAX_RATIO, MIN_RATIO, clampRatio, inSplit, openSplit, paneLayout, ratioFromPointer, splitAfterClose,
} from "./split";

test("oran %20–%80 arasında; işaretçi konumundan hesaplanır", () => {
  expect(MIN_RATIO).toBe(0.2);
  expect(MAX_RATIO).toBe(0.8);
  expect(clampRatio(0.1)).toBe(0.2);
  expect(clampRatio(0.95)).toBe(0.8);
  expect(clampRatio(0.5)).toBe(0.5);
  expect(clampRatio(Number.NaN)).toBe(0.5);
  expect(ratioFromPointer(300, 100, 400)).toBe(0.5);
  expect(ratioFromPointer(0, 100, 400)).toBe(0.2);
  expect(ratioFromPointer(300, 100, 0)).toBe(0.5);
});

test("bölme açılır: sol = mevcut, sağ = seçilen; aynı terminal iki bölmeye konmaz", () => {
  expect(openSplit(3, 5)).toEqual({ left: 3, right: 5, ratio: 0.5 });
  expect(openSplit(3, 3)).toBeNull();
  expect(inSplit(openSplit(3, 5), 5)).toBe(true);
  expect(inSplit(openSplit(3, 5), 4)).toBe(false);
  expect(inSplit(null, 4)).toBe(false);
  expect(inSplit(openSplit(3, 5), null)).toBe(false);
});

test("yerleşim: tek terminalde tam genişlik; bölmede iki pencere, etkin olan geçerli terminal", () => {
  expect(paneLayout(null, 3)).toEqual([{ id: 3, left: 0, width: 100, active: true }]);
  expect(paneLayout(null, null)).toEqual([]);
  const s = { left: 3, right: 5, ratio: 0.3 };
  expect(paneLayout(s, 3)).toEqual([
    { id: 3, left: 0, width: 30, active: true },
    { id: 5, left: 30, width: 70, active: false },
  ]);
  expect(paneLayout(s, 5).map((p) => p.active)).toEqual([false, true]);
  // Viewing a terminal outside the split shows it alone.
  expect(paneLayout(s, 9)).toEqual([{ id: 9, left: 0, width: 100, active: true }]);
});

test("bölmedeki bir terminal kapanınca diğeri tek başına kalır", () => {
  const s = { left: 3, right: 5, ratio: 0.4 };
  expect(splitAfterClose(s, 5)).toEqual({ split: null, show: 3 });
  expect(splitAfterClose(s, 3)).toEqual({ split: null, show: 5 });
  expect(splitAfterClose(s, 8)).toEqual({ split: s, show: null });
  expect(splitAfterClose(null, 8)).toEqual({ split: null, show: null });
});
