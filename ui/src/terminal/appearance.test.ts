import { expect, test } from "vitest";
import {
  DEFAULT_APPEARANCE, fontFamily, getAppearance, loadAppearance, resetAppearance, sanitize, updateAppearance,
  validFontName, xtermOptions,
} from "./appearance";
import { SCHEMES } from "./schemes";
import { MATCH } from "./match";
import { THEME_IDS } from "../theme";
import { activeScheme } from "./appearance";
import { renderOptions, terminalFont } from "./theme";

test("görünüm: varsayılanlar eski sabit görünümle aynı xterm seçeneklerini verir", () => {
  const o = xtermOptions(DEFAULT_APPEARANCE);
  expect(o).toMatchObject({
    fontFamily: terminalFont.fontFamily, fontSize: terminalFont.fontSize, fontWeight: terminalFont.fontWeight,
    lineHeight: renderOptions.lineHeight, letterSpacing: renderOptions.letterSpacing,
    drawBoldTextInBrightColors: renderOptions.drawBoldTextInBrightColors,
    minimumContrastRatio: renderOptions.minimumContrastRatio, cursorBlink: true, scrollback: 10000,
  });
  expect(o.theme).toEqual(SCHEMES.ubuntu.theme);
});

test("görünüm: her alan tek tek doğrulanır, geçersiz olan kendi varsayılanını alır", () => {
  const s = sanitize({
    scheme: "dracula", font: "Fira Code", fontSize: 99, fontWeight: 450, lineHeight: 1.234, letterSpacing: -1,
    cursorStyle: "bar", cursorBlink: "evet", scrollback: 5000.4, padding: 12, minContrast: 4.4, extra: 1,
  });
  expect(s).toEqual({
    ...DEFAULT_APPEARANCE, scheme: "dracula", font: "Fira Code", lineHeight: 1.25, cursorStyle: "bar",
    scrollback: 5000, padding: 12, minContrast: 4.5,
  });
  expect(sanitize(null)).toEqual(DEFAULT_APPEARANCE);
  expect(sanitize({ scheme: "__proto__", font: "Comic Sans" })).toEqual(DEFAULT_APPEARANCE);
});

test("görünüm: özel font adı CSS'e kaçamaz", () => {
  expect(validFontName("Maple Mono NF")).toBe(true);
  for (const bad of ['x", serif', "a;b", "url(x)", "a".repeat(65)]) expect(validFontName(bad), bad).toBe(false);
  expect(sanitize({ font: "custom", customFont: '"; color: red' }).customFont).toBe("");
  expect(fontFamily({ ...DEFAULT_APPEARANCE, font: "custom", customFont: "Maple Mono" }))
    .toBe(`"Maple Mono", ${terminalFont.fontFamily}`);
  expect(fontFamily({ ...DEFAULT_APPEARANCE, font: "custom", customFont: "  " })).toBe(terminalFont.fontFamily);
});

test("görünüm: değişiklik saklanır, yeniden yüklenir ve sıfırlanabilir", () => {
  updateAppearance({ scheme: "nord", fontSize: 16 });
  expect(getAppearance()).toMatchObject({ scheme: "nord", fontSize: 16 });
  expect(loadAppearance()).toMatchObject({ scheme: "nord", fontSize: 16 });
  localStorage.setItem("station.terminal", "{bozuk");
  expect(loadAppearance()).toEqual(DEFAULT_APPEARANCE);
  resetAppearance();
  expect(getAppearance()).toEqual(DEFAULT_APPEARANCE);
});

test("eşleştirme: her uygulama temasının geçerli bir şeması var; izleme açıkken o kullanılır", () => {
  for (const id of THEME_IDS) expect(SCHEMES[MATCH[id].scheme], id).toBeTruthy();
  const a = { ...DEFAULT_APPEARANCE, scheme: "dracula" as const };
  expect(activeScheme(a, "nord-dark")).toBe("dracula");
  expect(activeScheme({ ...a, followApp: true }, "nord-dark")).toBe("nord");
  expect(xtermOptions({ ...a, followApp: true }, "sepia-light").theme).toEqual(SCHEMES["gruvbox-light"].theme);
});
