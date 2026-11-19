import { beforeEach, expect, test, vi } from "vitest";
import type { Settings } from "./api";

/** Each set_settings call waits until the test resolves or rejects it. */
const calls: { sent: Settings; ok: () => void; fail: () => void }[] = [];
vi.mock("./api", () => ({
  getSettings: async () => ({ notifyWaiting: false, notifyFinished: false, lang: "tr" }) as unknown as Settings,
  setSettings: (s: Settings) => new Promise<Settings>((ok, fail) => {
    calls.push({ sent: s, ok: () => ok(s), fail: () => fail("settings_save_failed") });
  }),
}));

import { currentSettings, loadSettings, setToggle } from "./settings";

const flush = async () => { for (let i = 0; i < 5; i++) await Promise.resolve(); };

beforeEach(async () => {
  calls.length = 0;
  await loadSettings();
  calls.length = 0;
});

test("two quick toggles are saved one after the other and both stay", async () => {
  const a = setToggle("notifyWaiting", true);
  const b = setToggle("notifyFinished", true);
  await flush();
  // The second save waits for the first: no reply can overwrite a newer state.
  expect(calls).toHaveLength(1);
  calls[0].ok();
  await flush();
  expect(calls).toHaveLength(2);
  expect(calls[1].sent).toMatchObject({ notifyWaiting: true, notifyFinished: true });
  calls[1].ok();
  await Promise.all([a, b]);
  expect(currentSettings()).toMatchObject({ notifyWaiting: true, notifyFinished: true });
});

test("a refused toggle rolls back only itself, not a later one", async () => {
  const a = setToggle("notifyWaiting", true).catch((e: unknown) => e);
  const b = setToggle("notifyFinished", true);
  await flush();
  calls[0].fail();
  await flush();
  expect(calls[1].sent).toMatchObject({ notifyWaiting: false, notifyFinished: true });
  calls[1].ok();
  expect(await a).toBe("settings_save_failed");
  await b;
  expect(currentSettings()).toMatchObject({ notifyWaiting: false, notifyFinished: true });
});
