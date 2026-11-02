/**
 * App settings kept by the backend (settings.json). Loaded once at startup;
 * a toggle is applied at once and rolled back if the backend refuses it.
 * The UI language is pushed along so toasts and the tray use it too.
 */
import { getSettings, setSettings, type Settings } from "./api";
import { getLang } from "./i18n";
import { createStore } from "./prefs";

export type Toggle = "notifyWaiting" | "notifyFinished" | "notifySubagent" | "notifySound" | "closeToTray";

const store = createStore<Settings | null>(null);
export const useSettings = store.use;
export const currentSettings = store.get;

/** Loads the settings; brings the backend's language in line with the UI's. */
export async function loadSettings(): Promise<Settings | null> {
  try {
    const s = await getSettings();
    store.set(s);
    if (s.lang !== getLang()) await pushLang(getLang());
    return store.get();
  } catch {
    return null;
  }
}

async function save(next: Settings): Promise<void> {
  const prev = store.get();
  store.set(next);
  try {
    store.set(await setSettings(next));
  } catch (e) {
    store.set(prev);
    throw e;
  }
}

/** Sets one toggle; rejects with the backend's error code (state rolled back). */
export async function setToggle(key: Toggle, value: boolean): Promise<void> {
  const cur = store.get();
  if (!cur) return;
  await save({ ...cur, [key]: value });
}

/** Tells the backend the UI language (no-op before load or when unchanged). */
export async function pushLang(lang: string): Promise<void> {
  const cur = store.get();
  if (!cur || cur.lang === lang) return;
  await save({ ...cur, lang });
}

/** Test helper. */
export function resetSettingsForTest(s: Settings | null = null): void {
  store.set(s);
}
