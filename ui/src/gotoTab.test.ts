import { expect, test, vi } from "vitest";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));

import { goToTab, gotoTarget } from "./gotoTab";
import { setLang } from "./i18n";

test("gotoTarget: gömülü terminal önce, sonra Windows Terminal, yoksa null", () => {
  expect(gotoTarget({ pid: 5, hostKind: "other" }, 3)).toEqual({ kind: "embedded", terminal: 3 });
  expect(gotoTarget({ pid: 5, hostKind: "windowsTerminal" }, 3)).toEqual({ kind: "embedded", terminal: 3 });
  expect(gotoTarget({ pid: 5, hostKind: "windowsTerminal" }, undefined)).toEqual({ kind: "windowsTerminal" });
  expect(gotoTarget({ pid: 5, hostKind: "other" }, undefined)).toBeNull();
  expect(gotoTarget({ pid: 5, hostKind: "embedded" }, undefined)).toBeNull();
  // closed session: never
  expect(gotoTarget({ pid: 0, hostKind: "windowsTerminal" }, 3)).toBeNull();
});

test("goToTab: gömülü onTerminal, WT focus_session_tab notu, hata metni", async () => {
  const onTerminal = vi.fn();
  expect(await goToTab("s1", { kind: "embedded", terminal: 3 }, onTerminal)).toBeNull();
  expect(onTerminal).toHaveBeenCalledWith(3);
  expect(invoke).not.toHaveBeenCalled();
  invoke.mockResolvedValueOnce(null);
  expect(await goToTab("s1", { kind: "windowsTerminal" }, onTerminal)).toBeNull();
  expect(invoke).toHaveBeenCalledWith("focus_session_tab", { sessionId: "s1" });
  invoke.mockResolvedValueOnce("not");
  expect(await goToTab("s1", { kind: "windowsTerminal" }, onTerminal)).toBe("not");
  invoke.mockRejectedValueOnce("tab_not_found");
  await expect(goToTab("s1", { kind: "windowsTerminal" }, onTerminal)).rejects.toThrow("Sekmeye gidilemedi: Sekme bulunamadı");
  setLang("en");
  invoke.mockRejectedValueOnce("tab_not_found");
  await expect(goToTab("s1", { kind: "windowsTerminal" }, onTerminal)).rejects.toThrow("Could not go to tab: Tab not found");
  expect(await goToTab("s1", null, onTerminal)).toBeNull();
});
