import { beforeEach, expect, test } from "vitest";
import { CONV_FONT_DEFAULT, getConvFontSize, loadConvFontSize, setConvFontSize } from "./convAppearance";

beforeEach(() => { localStorage.clear(); setConvFontSize(CONV_FONT_DEFAULT); });

test("conversation font size: 13 px by default", () => {
  localStorage.clear();
  expect(loadConvFontSize()).toBe(13);
});

test("a set size is kept and stored", () => {
  setConvFontSize(17);
  expect(getConvFontSize()).toBe(17);
  expect(JSON.parse(localStorage.getItem("station.conversation")!).fontSize).toBe(17);
  expect(loadConvFontSize()).toBe(17);
});

test("sizes outside 10–24 px are clamped, odd stored values ignored", () => {
  setConvFontSize(99);
  expect(getConvFontSize()).toBe(24);
  setConvFontSize(3);
  expect(getConvFontSize()).toBe(10);
  localStorage.setItem("station.conversation", '{"fontSize":"big"}');
  expect(loadConvFontSize()).toBe(13);
  localStorage.setItem("station.conversation", "{not json");
  expect(loadConvFontSize()).toBe(13);
});
