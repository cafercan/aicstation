/**
 * App settings kept by the backend (settings.json). Loaded once at startup;
 * a toggle is applied at once and rolled back if the backend refuses it.
 * The UI language is pushed along so toasts and the tray use it too.
 */
import { getSettings, setSettings, type Settings } from "./api";
import { getLang } from "./i18n";
import { createStore } from "./prefs";

export type Toggle =
  | "notifyWaiting" | "notifyFinished" | "notifySubagent" | "notifyContext" | "notifyStalled" | "notifyBudget"
  | "notifySound"
  | "closeToTray";

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

/** The saves in flight run one after another: a reply never overwrites a newer change. */
let tail: Promise<unknown> = Promise.resolve();
let inFlight = 0;

/**
 * Applies `patch` at once and saves the whole current state after the saves
 * before it. Refused, only `patch`'s own keys go back to what they were.
 */
function save(patch: Partial<Settings>): Promise<void> {
  const cur = store.get();
  if (!cur) return Promise.resolve();
  const undo = Object.fromEntries(Object.keys(patch).map((k) => [k, cur[k as keyof Settings]])) as Partial<Settings>;
  store.set({ ...cur, ...patch });
  inFlight++;
  const run = tail.then(async () => {
    try {
      const saved = await setSettings(store.get()!);
      // A newer change waits behind this one: keep the shown state, it sends it.
      if (inFlight === 1) store.set(saved);
    } catch (e) {
      store.set({ ...store.get()!, ...undo });
      throw e;
    } finally {
      inFlight--;
    }
  });
  tail = run.catch(() => {});
  return run;
}

/** Sets one toggle; rejects with the backend's error code (state rolled back). */
export async function setToggle(key: Toggle, value: boolean): Promise<void> {
  if (!store.get()) return;
  await save({ [key]: value });
}

/** Settings chosen from a list (terminal shell and prompt). */
export type Choice = "shell" | "prompt" | "promptPreset";

/** Sets one choice; rejects with the backend's error code (state rolled back). */
export async function setChoice(key: Choice, value: string): Promise<void> {
  const cur = store.get();
  if (!cur || cur[key] === value) return;
  await save({ [key]: value });
}

/** Numeric settings (alert thresholds). */
export type NumberSetting = "contextThreshold" | "stalledMinutes" | "dailyBudgetUsd";

/** Sets one number; rejects with the backend's error code (state rolled back). */
export async function setNumber(key: NumberSetting, value: number): Promise<void> {
  const cur = store.get();
  if (!cur || cur[key] === value) return;
  await save({ [key]: value });
}

/** Tells the backend the UI language (no-op before load or when unchanged). */
export async function pushLang(lang: string): Promise<void> {
  const cur = store.get();
  if (!cur || cur.lang === lang) return;
  await save({ lang });
}
