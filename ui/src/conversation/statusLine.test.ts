import { expect, test } from "vitest";
import { findModeLine, findSpinner, screenRows } from "./statusLine";

test("finds the spinner, ASCII star included, and drops key hints", () => {
  expect(findSpinner(["", "* Fermenting… (33s · ↓ 2.8k tokens · thinking with high effort)", "  ⎿ Tip: x", "> "]))
    .toBe("* Fermenting… (33s · ↓ 2.8k tokens · thinking with high effort)");
  expect(findSpinner(["✽ Prestidigitating… (46s · ↓ 3.9k tokens) · esc to interrupt"]))
    .toBe("✽ Prestidigitating… (46s · ↓ 3.9k tokens)");
});

test("finished_line_is_not_a_spinner", () => {
  expect(findSpinner(["✻ Crunched for 3s · done 10:40 · 1 shell still running"])).toBeNull();
});

test("finds_mode_line_with_wide_glyphs_and_padding", () => {
  expect(findModeLine(["  ⏵⏵ auto mode on · 1 shell · ↓ to manage   "])).toBe("⏵⏵ auto mode on · 1 shell");
  expect(findModeLine(["⏸ plan mode on (shift+tab to cycle)"])).toBe("⏸ plan mode on");
  expect(findModeLine(["> typing ⏵⏵ here"])).toBeNull();
});

test("screenRows reads the screen at the bottom, not the scrolled view", () => {
  const lines = ["old", "a", "b"];
  const term = {
    rows: 2,
    buffer: { active: { baseY: 1, viewportY: 0, getLine: (i: number) => ({ translateToString: () => lines[i] }) } },
  };
  expect(screenRows(term as never)).toEqual(["a", "b"]);
});
