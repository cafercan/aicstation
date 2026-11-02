import { afterEach, expect, test, vi } from "vitest";
import {
  DICTS, LANGS, LANG_NAMES, errorText, getLang, loadLang, locale, setLang, t, type Dict, type Key, type Lang, type Msg,
} from "./index";
import deRaw from "./de.ts?raw";
import ruRaw from "./ru.ts?raw";
import esRaw from "./es.ts?raw";
import frRaw from "./fr.ts?raw";
import zhRaw from "./zh.ts?raw";
import { fmtElapsed, statusLabel } from "../format";
import { tr } from "./tr";
import errorsRs from "../../../src-tauri/src/errors.rs?raw";

afterEach(() => { vi.restoreAllMocks(); });

const placeholders = (m: Msg): string[] => {
  const s = typeof m === "string" ? m : `${m.one} ${m.other}`;
  return [...new Set([...s.matchAll(/\{(\w+)\}/g)].map((x) => x[1]))].sort();
};

test("her anahtar sekiz sözlükte de var, fazlası yok, hiçbiri boş değil", () => {
  const keys = Object.keys(tr).sort();
  expect(keys.length).toBeGreaterThan(50);
  for (const lang of LANGS) {
    const d: Dict = DICTS[lang];
    expect(Object.keys(d).sort(), lang).toEqual(keys);
    for (const k of keys) {
      const m = d[k as Key];
      const forms = typeof m === "string" ? [m] : [m.one, m.other];
      for (const f of forms) expect(f.trim(), `${lang}.${k}`).not.toBe("");
    }
  }
});

test("çeviriler Türkçedeki yer tutucuları aynen kullanır", () => {
  for (const lang of LANGS) {
    for (const k of Object.keys(tr) as Key[]) {
      const m = DICTS[lang][k];
      // EN singular may spell the number out ("1 agent running"): the plural form carries {n}.
      const own = typeof m === "string" ? placeholders(m) : placeholders({ one: m.other, other: m.other });
      expect(own, `${lang}.${k}`).toEqual(placeholders(tr[k]));
    }
  }
});

test("Rust hata kodlarının hepsinin çevirisi var", () => {
  const codes = [...errorsRs.matchAll(/pub const [A-Z_]+: &str = "([a-z_]+)";/g)].map((m) => m[1]);
  expect(codes.length).toBeGreaterThan(20);
  for (const c of codes) {
    for (const lang of LANGS) expect(DICTS[lang][`err.${c}` as Key], `${lang} ${c}`).toBeTruthy();
  }
});

test("dil seçimi kalıcı, anında uygulanır; varsayılan Türkçe", () => {
  expect(loadLang()).toBe("tr");
  setLang("en");
  expect(getLang()).toBe("en");
  expect(localStorage.getItem("pano.lang")).toBe("en");
  expect(document.documentElement.lang).toBe("en");
  expect(loadLang()).toBe("en");
  localStorage.setItem("pano.lang", "xx");
  expect(loadLang()).toBe("tr");
});

test("localStorage kullanılamazsa Türkçe ile devam eder, hata atmaz", () => {
  vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => { throw new Error("blocked"); });
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => { throw new Error("blocked"); });
  expect(loadLang()).toBe("tr");
  expect(() => setLang("az")).not.toThrow();
  expect(getLang()).toBe("az");
});

test("çoğul ve sayı biçimleri", () => {
  expect(t("card.agentsRunning", { n: 2 }, "tr")).toBe("2 ajan çalışıyor");
  expect(t("card.agentsRunning", { n: 2 }, "az")).toBe("2 agent işləyir");
  expect(t("card.agentsRunning", { n: 2 }, "en")).toBe("2 agents running");
  expect(t("card.agentsRunning", { n: 1 }, "en")).toBe("1 agent running");
  expect(t("card.agentsRunning", { n: 0 }, "en")).toBe("0 agents running");
});

test("t geçerli dili kullanır", () => {
  expect(t("nav.history")).toBe("Geçmiş");
  setLang("en");
  expect(t("nav.history")).toBe("History");
  setLang("az");
  expect(t("nav.history")).toBe("Tarixçə");
});

test("yerel ayarlar", () => {
  expect(locale("tr")).toBe("tr-TR");
  expect(locale("az")).toBe("az-Latn-AZ");
  expect(locale("en")).toBe("en-US");
  expect(locale("de")).toBe("de-DE");
  expect(locale("ru")).toBe("ru-RU");
  expect(locale("es")).toBe("es-ES");
  expect(locale("fr")).toBe("fr-FR");
  expect(locale("zh")).toBe("zh-CN");
});

test("hata kodları çevrilir; bilinmeyen kod ve düz metin olduğu gibi kalır", () => {
  expect(errorText("session_live", "tr")).toBe("Bu oturum zaten açık");
  expect(errorText("session_live", "en")).toBe("This session is already open");
  expect(errorText("resume_folder_mismatch:folder_network", "tr"))
    .toBe("Oturumun proje klasöründe devam edilemiyor: Ağ yolları desteklenmiyor");
  expect(errorText("pty_limit:32", "en")).toBe("At most 32 terminals can be open");
  expect(errorText("folder_open_failed:Erişim engellendi. (os error 5)", "tr"))
    .toBe("Klasör açılamadı: Erişim engellendi. (os error 5)");
  expect(errorText("tab_ambiguous", "az")).toBe(DICTS.az["err.tab_ambiguous"]);
  expect(errorText("weird_code", "tr")).toBe("weird_code");
  expect(errorText("command pty_spawn not found", "tr")).toBe("command pty_spawn not found");
  expect(errorText(new Error("session_unknown"), "en")).toBe(DICTS.en["err.session_unknown"]);
  expect(errorText("", "tr")).toBe("");
});

test("sekiz dil, sırayla, kendi adlarıyla", () => {
  expect([...LANGS]).toEqual(["tr", "az", "en", "de", "ru", "es", "fr", "zh"]);
  expect(Object.values(LANG_NAMES)).toEqual(
    ["Türkçe", "Azərbaycan", "English", "Deutsch", "Русский", "Español", "Français", "简体中文"]);
  const raws = { de: deRaw, ru: ruRaw, es: esRaw, fr: frRaw, zh: zhRaw };
  for (const [l, src] of Object.entries(raws)) {
    expect(src, l).toMatch(/^\/\*\*[\s\S]*AI-translated — native-speaker review welcome/);
  }
});

test("Rusça çoğul: 1, 2, 5, 21", () => {
  const f = (n: number) => t("card.agentsRunning", { n }, "ru");
  expect(f(1)).toBe("1 агент работает");
  expect(f(2)).toBe("2 агента работают");
  expect(f(5)).toBe("5 агентов работают");
  expect(f(21)).toBe("21 агент работает");
  expect(f(11)).toBe("11 агентов работают");
});

test("İngilizce, Almanca, İspanyolca, Fransızca çoğul; Çince çoğulsuz", () => {
  expect(t("sb.agents", { n: 1 }, "en")).toBe("1 agent");
  expect(t("sb.agents", { n: 5 }, "en")).toBe("5 agents");
  expect(t("sb.agents", { n: 1 }, "de")).toBe("1 Agent");
  expect(t("sb.agents", { n: 3 }, "de")).toBe("3 Agenten");
  expect(t("sb.agents", { n: 1 }, "es")).toBe("1 agente");
  expect(t("sb.agents", { n: 3 }, "es")).toBe("3 agentes");
  expect(t("sb.agents", { n: 1 }, "fr")).toBe("1 agent");
  expect(t("sb.agents", { n: 3 }, "fr")).toBe("3 agents");
  expect(t("sb.agents", { n: 1 }, "zh")).toBe("1 个智能体");
  expect(t("sb.agents", { n: 3 }, "zh")).toBe("3 个智能体");
  expect(typeof DICTS.zh["sb.agents"]).toBe("string");
});

test("süre birimleri dile göre", () => {
  const want: Record<string, string[]> = {
    de: ["5 Sek.", "5 Min.", "2 Std.", "2 Std. 5 Min.", "3 Tg."],
    ru: ["5 с", "5 мин", "2 ч", "2 ч 5 мин", "3 д"],
    es: ["5 s", "5 min", "2 h", "2 h 5 min", "3 d"],
    fr: ["5 s", "5 min", "2 h", "2 h 5 min", "3 j"],
    zh: ["5 秒", "5 分钟", "2 小时", "2 小时 5 分钟", "3 天"],
  };
  const ms = [5_000, 5 * 60_000, 2 * 3_600_000, 2 * 3_600_000 + 5 * 60_000, 3 * 86_400_000];
  for (const [lang, w] of Object.entries(want)) {
    expect(ms.map((m) => fmtElapsed(m, lang as Lang)), lang).toEqual(w);
  }
});

test("\"shell\" durum etiketi her dilde dolu ve farklı", () => {
  const labels = LANGS.map((l) => statusLabel("shell", undefined, l));
  expect(new Set(labels).size).toBe(8);
});
