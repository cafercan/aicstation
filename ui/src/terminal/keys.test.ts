import { expect, test } from "vitest";
import {
  clampFontSize, DEFAULT_FONT_SIZE, fontSizeAfter, keyAction, wheelFontAction, type KeyLike,
} from "./keys";

function k(key: string, mods: Partial<KeyLike> = {}): KeyLike {
  return { type: "keydown", key, code: "", ctrlKey: true, shiftKey: false, altKey: false, metaKey: false, ...mods };
}

test("Ctrl+C: seçim varsa kopyalar, yoksa ^C kabuğa gider", () => {
  expect(keyAction(k("c"), true)).toBe("copy");
  expect(keyAction(k("c"), false)).toBe("pass");
});

test("Ctrl+Shift+C her zaman kopyalar; seçim yoksa hiçbir şey göndermez", () => {
  expect(keyAction(k("C", { shiftKey: true }), true)).toBe("copy");
  expect(keyAction(k("C", { shiftKey: true }), false)).toBe("swallow");
});

test("Ctrl+V ve Ctrl+Shift+V tarayıcının yerel paste olayına bırakılır", () => {
  expect(keyAction(k("v"), false)).toBe("paste");
  expect(keyAction(k("V", { shiftKey: true }), true)).toBe("paste");
});

test("Ctrl+F arama çubuğunu açar", () => {
  expect(keyAction(k("f"), false)).toBe("search");
  expect(keyAction(k("F", { shiftKey: true }), false)).toBe("search");
});

test("yazı boyutu kısayolları: Ctrl+= / Ctrl++ / Ctrl+- / Ctrl+0 ve numpad", () => {
  expect(keyAction(k("="), false)).toBe("fontUp");
  expect(keyAction(k("+", { shiftKey: true }), false)).toBe("fontUp");
  expect(keyAction(k("-"), false)).toBe("fontDown");
  expect(keyAction(k("0"), false)).toBe("fontReset");
  expect(keyAction(k("+", { code: "NumpadAdd" }), false)).toBe("fontUp");
  expect(keyAction(k("-", { code: "NumpadSubtract" }), false)).toBe("fontDown");
  expect(keyAction(k("Insert", { code: "Numpad0" }), false)).toBe("fontReset");
  // Turkish Q: Shift+0 produces "=" with code Digit0; the produced key wins.
  expect(keyAction(k("=", { shiftKey: true, code: "Digit0" }), false)).toBe("fontUp");
});

test("Ctrl olmadan, AltGr (Ctrl+Alt) ile ya da Meta ile hiçbir şey yakalanmaz", () => {
  expect(keyAction(k("c", { ctrlKey: false }), true)).toBe("pass");
  expect(keyAction(k("v", { ctrlKey: false }), false)).toBe("pass");
  expect(keyAction(k("=", { ctrlKey: false }), false)).toBe("pass");
  // AltGr+C / AltGr+V / AltGr+0 type characters on many layouts.
  expect(keyAction(k("c", { altKey: true }), true)).toBe("pass");
  expect(keyAction(k("v", { altKey: true }), false)).toBe("pass");
  expect(keyAction(k("0", { altKey: true }), false)).toBe("pass");
  expect(keyAction(k("c", { metaKey: true }), true)).toBe("pass");
  // Other Ctrl keys (Ctrl+D, Ctrl+R, Ctrl+L …) belong to the shell.
  expect(keyAction(k("d"), false)).toBe("pass");
  expect(keyAction(k("r"), true)).toBe("pass");
});

test("keydown dışındaki olaylar eylem tetiklemez ama yakalanan tuşlar xterm'e gitmez", () => {
  expect(keyAction(k("c", { type: "keypress" }), true)).toBe("swallow");
  expect(keyAction(k("v", { type: "keyup" }), false)).toBe("swallow");
  expect(keyAction(k("f", { type: "keypress" }), false)).toBe("swallow");
  // Ctrl+C without selection must still reach xterm on every event type.
  expect(keyAction(k("c", { type: "keypress" }), false)).toBe("pass");
  expect(keyAction(k("d", { type: "keyup" }), false)).toBe("pass");
});

test("yazı boyutu 8–32 aralığına sıkıştırılır", () => {
  expect(clampFontSize(7)).toBe(8);
  expect(clampFontSize(8)).toBe(8);
  expect(clampFontSize(20)).toBe(20);
  expect(clampFontSize(33)).toBe(32);
  expect(clampFontSize(Number.NaN)).toBe(DEFAULT_FONT_SIZE);
  expect(clampFontSize(12.6)).toBe(13);
});

test("fontSizeAfter: bir adım büyüt/küçült, sınırlarda durur, sıfırla 13", () => {
  expect(DEFAULT_FONT_SIZE).toBe(13);
  expect(fontSizeAfter("fontUp", 13)).toBe(14);
  expect(fontSizeAfter("fontDown", 13)).toBe(12);
  expect(fontSizeAfter("fontUp", 32)).toBe(32);
  expect(fontSizeAfter("fontDown", 8)).toBe(8);
  expect(fontSizeAfter("fontReset", 27)).toBe(13);
});

test("Ctrl+tekerlek: yukarı büyütür, aşağı küçültür, Ctrl yoksa kaydırma xterm'de kalır", () => {
  expect(wheelFontAction({ ctrlKey: true, deltaY: -100 })).toBe("fontUp");
  expect(wheelFontAction({ ctrlKey: true, deltaY: 100 })).toBe("fontDown");
  expect(wheelFontAction({ ctrlKey: true, deltaY: 0 })).toBe("swallow");
  expect(wheelFontAction({ ctrlKey: false, deltaY: -100 })).toBeNull();
});
