/**
 * UI strings: one small dictionary per language, no i18n library. `tr` is the
 * reference: its keys are the `Key` type, so a key missing from another
 * dictionary is a compile error (and i18n.test.ts checks it at runtime).
 *
 * The chosen language is a per-device preference (localStorage "pano.lang",
 * default Turkish) and applies without a reload: components read it through
 * `useT()` / `useLang()`, plain functions through `getLang()`.
 */
import { createStore, readPref, writePref } from "../prefs";
import { az } from "./az";
import { en } from "./en";
import { tr } from "./tr";
import { de } from "./de";
import { ru } from "./ru";
import { es } from "./es";
import { fr } from "./fr";
import { zh } from "./zh";

export type Lang = "tr" | "az" | "en" | "de" | "ru" | "es" | "fr" | "zh";
export const LANGS: readonly Lang[] = ["tr", "az", "en", "de", "ru", "es", "fr", "zh"];

/**
 * A message. A language that inflects for a count gives `one`/`other` (plus
 * `few`/`many` for Russian); the form is picked with Intl.PluralRules from `n`.
 * Languages without plurals (zh) use a plain string.
 */
export type Msg = string | { one: string; other: string; few?: string; many?: string };
export type Key = keyof typeof tr;
export type Dict = Record<Key, Msg>;
export type Params = Record<string, string | number>;

export const DICTS: Record<Lang, Dict> = { tr, az, en, de, ru, es, fr, zh };

const LOCALES: Record<Lang, string> = {
  tr: "tr-TR", az: "az-Latn-AZ", en: "en-US", de: "de-DE", ru: "ru-RU", es: "es-ES", fr: "fr-FR", zh: "zh-CN",
};
/** Names on the language switch, each in its own language. */
export const LANG_NAMES: Record<Lang, string> = {
  tr: "Türkçe", az: "Azərbaycan", en: "English", de: "Deutsch", ru: "Русский", es: "Español", fr: "Français", zh: "简体中文",
};

const STORAGE_KEY = "pano.lang";
const isLang = (v: unknown): v is Lang => typeof v === "string" && (LANGS as readonly string[]).includes(v);

/** Stored language, or Turkish. */
export function loadLang(): Lang {
  const v = readPref(STORAGE_KEY);
  return isLang(v) ? v : "tr";
}

const store = createStore<Lang>(loadLang());

export const getLang = store.get;
export const useLang = store.use;

export function setLang(lang: Lang): void {
  if (!isLang(lang)) return;
  writePref(STORAGE_KEY, lang);
  if (typeof document !== "undefined") document.documentElement.lang = lang;
  store.set(lang);
}

/** BCP 47 locale for Intl date/time formatting. */
export function locale(lang: Lang = getLang()): string {
  return LOCALES[lang];
}

function pluralForm(m: Exclude<Msg, string>, n: string | number | undefined, lang: Lang): string {
  const count = Number(n);
  if (n === undefined || !Number.isFinite(count)) return m.other;
  let cat: Intl.LDMLPluralRule;
  try { cat = new Intl.PluralRules(LOCALES[lang]).select(count); } catch { cat = count === 1 ? "one" : "other"; }
  return (cat === "one" || cat === "few" || cat === "many" ? m[cat] : undefined) ?? m.other;
}

function interpolate(s: string, params?: Params): string {
  if (!params) return s;
  return s.replace(/\{(\w+)\}/g, (all, name: string) =>
    Object.prototype.hasOwnProperty.call(params, name) ? String(params[name]) : all);
}

export function t(key: Key, params?: Params, lang: Lang = getLang()): string {
  const m = DICTS[lang][key] ?? tr[key];
  return interpolate(typeof m === "string" ? m : pluralForm(m, params?.n, lang), params);
}

/** `t` bound to the current language; the component re-renders when it changes. */
export function useT(): (key: Key, params?: Params) => string {
  const lang = useLang();
  return (key, params) => t(key, params, lang);
}

const CODE = /^([a-z][a-z0-9_]*)(?::([\s\S]*))?$/;

/**
 * Text for an error from the backend. Commands reject with a code or
 * `code:detail` (src-tauri/src/errors.rs); the detail is a number, an OS
 * message or another code. Anything that is not a known code is shown as is.
 */
export function errorText(e: unknown, lang: Lang = getLang()): string {
  const s = typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
  const m = CODE.exec(s);
  if (!m) return s;
  const key = `err.${m[1]}`;
  if (!Object.prototype.hasOwnProperty.call(DICTS[lang], key)) return s;
  const msg = t(key as Key, undefined, lang);
  if (m[2] === undefined) return msg;
  const detail = errorText(m[2], lang);
  return msg.includes("{detail}") ? interpolate(msg, { detail }) : `${msg}: ${detail}`;
}
