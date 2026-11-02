import { afterEach, expect, test, vi } from "vitest";
import appCss from "../app.css?raw";
import {
  THEME_IDS, THEMES, TOKENS, applyTheme, cssVar, getTheme, loadTheme, setTheme, type ThemeId,
} from "./index";

afterEach(() => { vi.restoreAllMocks(); });

/** WCAG relative luminance of #rrggbb. */
function lum(hex: string): number {
  const c = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255)
    .map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
  return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
}
function contrast(a: string, b: string): number {
  const [x, y] = [lum(a), lum(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
}
/** `fg` at `alpha` over `bg` (badge backgrounds: status color at 15%). */
function blend(fg: string, bg: string, alpha: number): string {
  const ch = (h: string, i: number) => parseInt(h.slice(i, i + 2), 16);
  return "#" + [1, 3, 5].map((i) => Math.round(ch(fg, i) * alpha + ch(bg, i) * (1 - alpha))
    .toString(16).padStart(2, "0")).join("");
}
const HEX = /^#[0-9a-f]{6}$/;

test("altı tema, sırası ve varsayılanı", () => {
  expect(THEME_IDS).toEqual(["pano-dark", "slate-dark", "nord-dark", "minimal-light", "github-light", "sepia-light"]);
  expect(loadTheme()).toBe("pano-dark");
  expect(THEMES["slate-dark"].tokens.bg).toBe("#0f172a");
  expect(THEMES["slate-dark"].tokens.accent).toBe("#6366f1");
  expect(THEMES["nord-dark"].tokens.bg).toBe("#2e3440");
  expect(THEMES["nord-dark"].tokens.accent).toBe("#88c0d0");
  expect(THEMES["minimal-light"].tokens.bg).toBe("#f8fafc");
  expect(THEMES["minimal-light"].tokens.accent).toBe("#4f46e5");
  expect(THEMES["github-light"].tokens.bg).toBe("#ffffff");
  expect(THEMES["github-light"].tokens.accent).toBe("#0969da");
  expect(THEMES["sepia-light"].tokens.bg).toBe("#f4ecd8");
  expect(THEMES["sepia-light"].tokens.accent).toBe("#a15c2b");
});

test("her tema her token'ı tanımlar; renkler #rrggbb", () => {
  for (const id of THEME_IDS) {
    const tk = THEMES[id].tokens;
    expect(Object.keys(tk).sort(), id).toEqual([...TOKENS].sort());
    for (const name of TOKENS) {
      expect(tk[name], `${id}.${name}`).toBeTruthy();
      if (name !== "shadow") expect(tk[name], `${id}.${name}`).toMatch(HEX);
    }
  }
});

test("pano-dark, app.css :root yedek değerleriyle aynı", () => {
  const root = /:root\s*\{([^}]*)\}/.exec(appCss)![1];
  const vars = Object.fromEntries([...root.matchAll(/(--[\w-]+):\s*([^;]+);/g)].map((m) => [m[1], m[2].trim()]));
  for (const name of TOKENS) expect(vars[cssVar(name)], name).toBe(THEMES["pano-dark"].tokens[name]);
});

test("okunabilirlik: metin ≥ 4.5, soluk metin ≥ 3, rozet ve status bar durum metni ≥ 3", () => {
  for (const id of THEME_IDS) {
    const k = THEMES[id].tokens;
    expect(contrast(k.text, k.bg), `${id} text`).toBeGreaterThanOrEqual(4.5);
    expect(contrast(k.text, k.surfaceA), `${id} text/surface`).toBeGreaterThanOrEqual(4.5);
    expect(contrast(k.muted, k.bg), `${id} muted`).toBeGreaterThanOrEqual(3);
    expect(contrast(k.dim, k.surfaceB), `${id} dim`).toBeGreaterThanOrEqual(3);
    for (const st of ["busy", "idle", "waiting", "closed"] as const) {
      const fg = k[`${st}Text`];
      for (const under of [k.surfaceA, k.surfaceB, k.bg]) {
        expect(contrast(fg, blend(k[st], under, 0.15)), `${id} ${st} badge`).toBeGreaterThanOrEqual(3);
      }
      expect(contrast(fg, k.sbBg), `${id} ${st} status bar`).toBeGreaterThanOrEqual(3);
      // The card stripe, donut and context bar must stand out from the card.
      expect(contrast(k[st], k.surfaceA), `${id} ${st} mark`).toBeGreaterThanOrEqual(2);
    }
  }
});

test("tema uygulanınca dataset, CSS değişkenleri ve kalıcılık", () => {
  setTheme("sepia-light");
  const root = document.documentElement;
  expect(getTheme()).toBe("sepia-light");
  expect(root.dataset.theme).toBe("sepia-light");
  expect(root.style.getPropertyValue("--bg")).toBe("#f4ecd8");
  expect(root.style.getPropertyValue("--surface-a")).toBe(THEMES["sepia-light"].tokens.surfaceA);
  expect(root.style.colorScheme).toBe("light");
  expect(localStorage.getItem("pano.theme")).toBe("sepia-light");
  expect(loadTheme()).toBe("sepia-light");
  localStorage.setItem("pano.theme", "pembe");
  expect(loadTheme()).toBe("pano-dark");
});

test("applyTheme bilinmeyen kimliği yok sayar; localStorage hata atsa da çalışır", () => {
  applyTheme("yok" as ThemeId);
  expect(document.documentElement.dataset.theme).toBe("pano-dark");
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => { throw new Error("blocked"); });
  vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => { throw new Error("blocked"); });
  expect(() => setTheme("nord-dark")).not.toThrow();
  expect(document.documentElement.dataset.theme).toBe("nord-dark");
  expect(loadTheme()).toBe("pano-dark");
});
