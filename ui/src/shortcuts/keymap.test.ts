import { expect, test } from "vitest";
import { keyAction, type KeyLike } from "../terminal/keys";
import { KEYMAP, cycle, displayChord, matchChord, parseChord, type KeyEventLike } from "./keymap";

/** A US-layout keydown. */
function ev(chord: string, over: Partial<KeyEventLike> = {}): KeyEventLike {
  const parts = chord.split("+");
  const k = parts[parts.length - 1];
  const has = (m: string) => parts.slice(0, -1).includes(m);
  let key = k;
  let code = "";
  if (/^[A-Z]$/i.test(k)) { key = has("Shift") ? k.toUpperCase() : k.toLowerCase(); code = `Key${k.toUpperCase()}`; }
  else if (/^\d$/.test(k)) { code = `Digit${k}`; }
  else if (k === "/") { key = has("Shift") ? "?" : "/"; code = "Slash"; }
  else if (k === "\\") { key = has("Shift") ? "|" : "\\"; code = "Backslash"; }
  else if (k === "Tab") { code = "Tab"; }
  else if (k === "Space") { key = " "; code = "Space"; }
  else code = k;
  return {
    type: "keydown", key, code,
    ctrlKey: has("Ctrl"), shiftKey: has("Shift"), altKey: has("Alt"), metaKey: false, altGraph: false, ...over,
  };
}

test("akor ayrıştırma", () => {
  expect(parseChord("Ctrl+Shift+T")).toEqual({ ctrl: true, shift: true, alt: false, key: "t" });
  expect(parseChord("Ctrl+Alt+3")).toEqual({ ctrl: true, shift: false, alt: true, key: "3" });
  expect(parseChord("Ctrl+Shift+\\")).toEqual({ ctrl: true, shift: true, alt: false, key: "\\" });
  expect(parseChord("Ctrl+Shift+/")).toEqual({ ctrl: true, shift: true, alt: false, key: "/" });
  expect(parseChord("Ctrl+Tab")).toEqual({ ctrl: true, shift: false, alt: false, key: "tab" });
  expect(parseChord("Shift+Ctrl+P")).toEqual({ ctrl: true, shift: true, alt: false, key: "p" });
  expect(parseChord("")).toBeNull();
  expect(parseChord("Ctrl+")).toBeNull();
  expect(parseChord("Hyper+X")).toBeNull();
});

test("tüm kısayollar tanınır; Ctrl+Alt+1..9 terminal numarası taşır", () => {
  const want: [string, string, number?][] = [
    ["Ctrl+Shift+T", "newTerminal"], ["Ctrl+Shift+W", "closeTerminal"], ["Ctrl+Shift+D", "dashboard"],
    ["Ctrl+Shift+H", "history"], ["Ctrl+Shift+F", "focusSearch"], ["Ctrl+Tab", "nextTerminal"],
    ["Ctrl+Shift+Tab", "prevTerminal"], ["Ctrl+Shift+P", "palette"], ["Ctrl+Shift+\\", "toggleSplit"],
    ["Ctrl+Shift+/", "shortcuts"], ["Ctrl+Alt+1", "switchTerminal", 1], ["Ctrl+Alt+9", "switchTerminal", 9],
  ];
  for (const [c, id, n] of want) {
    const b = matchChord(ev(c));
    expect(b?.id, c).toBe(id);
    if (n !== undefined) expect(b?.n, c).toBe(n);
  }
  expect(matchChord(ev("Ctrl+Alt+0"))).toBeNull();
  // Numpad digits too, and Caps Lock (lowercase key) does not matter.
  expect(matchChord(ev("Ctrl+Alt+4", { code: "Numpad4" }))?.n).toBe(4);
  expect(matchChord(ev("Ctrl+Shift+T", { key: "t" }))?.id).toBe("newTerminal");
});

test("yalnız keydown; başka akorlar ve kabuk tuşları geçer (null)", () => {
  expect(matchChord(ev("Ctrl+Shift+T", { type: "keyup" }))).toBeNull();
  for (const c of [
    "Ctrl+C", "Ctrl+V", "Ctrl+F", "Ctrl+Shift+C", "Ctrl+Shift+V", "Ctrl+R", "Ctrl+L", "Ctrl+A", "Ctrl+E", "Ctrl+K",
    "Ctrl+U", "Ctrl+W", "Ctrl+T", "Ctrl+D", "Ctrl+P", "Ctrl+Z", "Alt+F", "Alt+B", "Ctrl+Space", "Shift+Tab", "Tab",
    "Ctrl+Alt+/", "Ctrl+Shift+Z", "Escape", "Ctrl+1", "Alt+1", "T", "Shift+T",
  ]) {
    expect(matchChord(ev(c)), c).toBeNull();
  }
  expect(matchChord(ev("Ctrl+Shift+T", { metaKey: true }))).toBeNull();
  expect(matchChord(ev("Ctrl+Shift+T", { altKey: true }))).toBeNull();
});

test("AltGr ile yazılan karakterler çalınmaz (Türkçe Q: AltGr+7 = '{')", () => {
  // AltGr arrives as Ctrl+Alt; the key is the character it types.
  expect(matchChord(ev("Ctrl+Alt+7", { key: "{" }))).toBeNull();
  expect(matchChord(ev("Ctrl+Alt+2", { key: "£" }))).toBeNull();
  expect(matchChord(ev("Ctrl+Alt+1", { altGraph: true }))).toBeNull();
  expect(matchChord(ev("Ctrl+Shift+T", { altGraph: true }))).toBeNull();
});

test("terminalin kendi tuşlarıyla çakışma yok: uygulama akorlarını xterm işleyicisi 'pass' sayar", () => {
  for (const b of KEYMAP) {
    const e = ev(b.chord);
    for (const sel of [false, true]) {
      const a = keyAction(e as KeyLike, sel);
      // Ctrl+Shift+F means "search" in both: on a terminal page the app opens the terminal's search bar.
      if (b.id === "focusSearch") expect(a, b.chord).toBe("search");
      else expect(a, `${b.chord} sel=${sel}`).toBe("pass");
    }
  }
  // ...and none of the terminal's combos is an app chord.
  for (const c of ["Ctrl+C", "Ctrl+Shift+C", "Ctrl+V", "Ctrl+Shift+V", "Ctrl+F", "Ctrl+=", "Ctrl+-", "Ctrl+0"]) {
    expect(matchChord(ev(c)), c).toBeNull();
  }
});

test("harfler fiziksel tuşla eşleşir: Kiril ve Türkçe düzenlerde de çalışır", () => {
  // Russian ЙЦУКЕН: the T key types "е", W "ц", D "в", H "р", F "а", P "з".
  const ru: [string, string, string][] = [
    ["KeyT", "Е", "newTerminal"], ["KeyW", "Ц", "closeTerminal"], ["KeyD", "В", "dashboard"],
    ["KeyH", "Р", "history"], ["KeyF", "А", "focusSearch"], ["KeyP", "З", "palette"],
  ];
  for (const [code, key, id] of ru) {
    expect(matchChord(ev("Ctrl+Shift+T", { code, key }))?.id, code).toBe(id);
  }
  // Turkish F: the key at the T position types another letter; the position decides.
  expect(matchChord(ev("Ctrl+Shift+T", { code: "KeyT", key: "I" }))?.id).toBe("newTerminal");
  // Dvorak-like: the key that types "t" sits elsewhere; its code wins.
  expect(matchChord(ev("Ctrl+Shift+T", { code: "KeyK", key: "T" }))).toBeNull();
  // No usable code: the character is the fallback.
  expect(matchChord(ev("Ctrl+Shift+T", { code: "" }))?.id).toBe("newTerminal");
  expect(matchChord(ev("Ctrl+Shift+W", { code: "Unidentified" }))?.id).toBe("closeTerminal");
});

test("/ ve \\ fiziksel konumdan (Slash, Backslash, IntlBackslash)", () => {
  // Turkish Q: Slash is the "." key (Shift gives ":"), Backslash is "," (";"),
  // IntlBackslash is "<" (">").
  expect(matchChord(ev("Ctrl+Shift+/", { key: ":" }))?.id).toBe("shortcuts");
  expect(matchChord(ev("Ctrl+Shift+\\", { key: ";" }))?.id).toBe("toggleSplit");
  expect(matchChord(ev("Ctrl+Shift+\\", { code: "IntlBackslash", key: ">" }))?.id).toBe("toggleSplit");
  // "/" typed elsewhere (Turkish Q Shift+7) is not the chord key.
  expect(matchChord(ev("Ctrl+Shift+7", { key: "/" }))).toBeNull();
  // Without a code the character still works.
  expect(matchChord(ev("Ctrl+Shift+/", { code: "" }))?.id).toBe("shortcuts");
  expect(matchChord(ev("Ctrl+Shift+\\", { code: "" }))?.id).toBe("toggleSplit");
});

test("yardım tablosu tuş başlığını geçerli düzenden gösterir", () => {
  const trQ = new Map([["Slash", "."], ["Backslash", ","], ["IntlBackslash", "<"], ["KeyT", "t"]]);
  expect(displayChord("Ctrl+Shift+/", trQ)).toBe("Ctrl+Shift+.");
  expect(displayChord("Ctrl+Shift+\\", trQ)).toBe("Ctrl+Shift+,");
  expect(displayChord("Ctrl+Shift+T", trQ)).toBe("Ctrl+Shift+T");
  const us = new Map([["Slash", "/"], ["Backslash", "\\"]]);
  expect(displayChord("Ctrl+Shift+/", us)).toBe("Ctrl+Shift+/");
  expect(displayChord("Ctrl+Shift+\\", us)).toBe("Ctrl+Shift+\\");
  // ISO keyboard without a Backslash key in the map: the IntlBackslash cap.
  expect(displayChord("Ctrl+Shift+\\", new Map([["IntlBackslash", "<"]]))).toBe("Ctrl+Shift+<");
  expect(displayChord("Ctrl+Shift+/", null)).toBe("Ctrl+Shift+/");
  expect(displayChord("Ctrl+Shift+/", new Map())).toBe("Ctrl+Shift+/");
});

test("akorlar benzersiz", () => {
  const seen = new Set(KEYMAP.map((b) => JSON.stringify(parseChord(b.chord))));
  expect(seen.size).toBe(KEYMAP.length);
});

test("cycle: sonraki/önceki terminal, başa/sona sarar", () => {
  expect(cycle([1, 4, 7], 4, 1)).toBe(7);
  expect(cycle([1, 4, 7], 7, 1)).toBe(1);
  expect(cycle([1, 4, 7], 1, -1)).toBe(7);
  expect(cycle([1, 4, 7], null, 1)).toBe(1);
  expect(cycle([1, 4, 7], null, -1)).toBe(7);
  expect(cycle([1, 4, 7], 99, 1)).toBe(1);
  expect(cycle([], 1, 1)).toBeNull();
});
