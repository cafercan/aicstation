import { expect, test } from "vitest";
import { ANSI_KEYS, DEFAULT_SCHEME, SCHEMES, SCHEME_IDS, isScheme } from "./schemes";
import { ubuntuTheme } from "./theme";

const HEX = /^#[0-9A-Fa-f]{6}([0-9A-Fa-f]{2})?$/;

test("şemalar: her renk geçerli hex, 16 ANSI rengi eksiksiz", () => {
  expect(SCHEME_IDS.length).toBeGreaterThanOrEqual(15);
  for (const id of SCHEME_IDS) {
    const t = SCHEMES[id].theme;
    for (const [k, v] of Object.entries(t)) expect(v, `${id}.${k}`).toMatch(HEX);
    for (const k of ANSI_KEYS) expect(t[k], `${id}.${k}`).toBeTruthy();
  }
});

test("şemalar: varsayılan Ubuntu, eski sabit temayla aynı renkler", () => {
  expect(DEFAULT_SCHEME).toBe("ubuntu");
  const u = SCHEMES.ubuntu.theme;
  for (const [k, v] of Object.entries(ubuntuTheme)) expect(u[k as keyof typeof u], k).toBe(v);
});

test("şemalar: açık ve koyu şemalar var; bilinmeyen id reddedilir", () => {
  expect(SCHEME_IDS.some((id) => !SCHEMES[id].dark)).toBe(true);
  expect(isScheme("dracula")).toBe(true);
  expect(isScheme("toString")).toBe(false);
  expect(isScheme("yok")).toBe(false);
});
