import { afterEach } from "vitest";
import { act, cleanup } from "@testing-library/react";
import { setLang } from "./i18n";
import { applyTheme } from "./theme";
import { setOsForTests } from "./platform";

// The suite was written against the Windows labels; Linux ones are tested explicitly.
setOsForTests("windows");

afterEach(() => {
  cleanup();
  // Preferences are global: every test starts in Turkish with empty storage.
  act(() => { setLang("tr"); applyTheme("station-dark"); });
  setOsForTests("windows");
  try { localStorage.clear(); } catch { /* storage mocked away */ }
});
