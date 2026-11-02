import { afterEach } from "vitest";
import { act, cleanup } from "@testing-library/react";
import { setLang } from "./i18n";

afterEach(() => {
  cleanup();
  // Preferences are global: every test starts in Turkish with empty storage.
  act(() => { setLang("tr"); });
  try { localStorage.clear(); } catch { /* storage mocked away */ }
});
