import { afterEach, expect, test, vi } from "vitest";
import {
  DICTS, LANGS, errorText, getLang, loadLang, locale, setLang, t, type Dict, type Key, type Msg,
} from "./index";
import { tr } from "./tr";
import errorsRs from "../../../src-tauri/src/errors.rs?raw";

afterEach(() => { vi.restoreAllMocks(); });

const placeholders = (m: Msg): string[] => {
  const s = typeof m === "string" ? m : `${m.one} ${m.other}`;
  return [...new Set([...s.matchAll(/\{(\w+)\}/g)].map((x) => x[1]))].sort();
};

test("her anahtar üç sözlükte de var, fazlası yok, hiçbiri boş değil", () => {
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
  localStorage.setItem("pano.lang", "de");
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
});

test("hata kodları çevrilir; bilinmeyen kod ve düz metin olduğu gibi kalır", () => {
  expect(errorText("session_live", "tr")).toBe("Bu oturum zaten açık");
  expect(errorText("session_live", "en")).toBe("This session is already open");
  expect(errorText("session_folder:folder_network", "tr"))
    .toBe("Oturumun klasörü kullanılamıyor: Ağ yolları desteklenmiyor");
  expect(errorText("pty_limit:32", "en")).toBe("At most 32 terminals can be open");
  expect(errorText("folder_open_failed:Erişim engellendi. (os error 5)", "tr"))
    .toBe("Klasör açılamadı: Erişim engellendi. (os error 5)");
  expect(errorText("tab_ambiguous", "az")).toBe(DICTS.az["err.tab_ambiguous"]);
  expect(errorText("weird_code", "tr")).toBe("weird_code");
  expect(errorText("command pty_spawn not found", "tr")).toBe("command pty_spawn not found");
  expect(errorText(new Error("session_unknown"), "en")).toBe(DICTS.en["err.session_unknown"]);
  expect(errorText("", "tr")).toBe("");
});
