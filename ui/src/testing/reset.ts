/**
 * Test-only: restores every in-memory store (page filters, cost range,
 * settings, terminals) to its initial state. Imported only by tests; the
 * stores register themselves through `onTestReset` (prefs.ts), which keeps
 * nothing outside Vitest.
 */
import { testResets } from "../prefs";

export function resetUiState(): void {
  for (const fn of testResets) fn();
}
