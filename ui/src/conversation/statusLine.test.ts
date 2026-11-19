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

/** Claude Code's input box: a rule, the prompt row, a rule (the lower one may carry a name). */
const BOX = ["─".repeat(60), "> ", `${"─".repeat(50)} aicstation ─`];

test("finds_mode_line_with_wide_glyphs_and_padding", () => {
  expect(findModeLine([...BOX, "", "  ⏵⏵ auto mode on · 1 shell · ↓ to manage   "])).toBe("⏵⏵ auto mode on · 1 shell");
  expect(findModeLine([...BOX, "⏸ plan mode on (shift+tab to cycle)"])).toBe("⏸ plan mode on");
  expect(findModeLine([...BOX, "> typing ⏵⏵ here"])).toBeNull();
});

test("mode line keeps only its left column, not the notice on the right", () => {
  expect(findModeLine([...BOX, "⏵⏵ accept edits on (shift+tab to cycle)            Context left until auto-compact: 8%"]))
    .toBe("⏵⏵ accept edits on");
});

test("only rows under the input box can be the mode line", () => {
  // Default mode draws no mode line; a reply row starting with ⏵⏵ is not one.
  expect(findModeLine(["⏵⏵ auto mode on · 1 shell", ...BOX, "  ? for shortcuts"])).toBeNull();
  expect(findModeLine(["⏵⏵ auto mode on"])).toBeNull();
});

test("the spinner is looked for above the input box", () => {
  expect(findSpinner([...BOX, "* Fermenting… (3s · ↓ 10 tokens)"])).toBeNull();
  expect(findSpinner(["* Fermenting… (3s · ↓ 10 tokens)", "", ...BOX, "⏵⏵ auto mode on"])).toBe("* Fermenting… (3s · ↓ 10 tokens)");
});

test("screenRows reads the screen at the bottom, not the scrolled view", () => {
  const lines = ["old", "a", "b"];
  const term = {
    rows: 2,
    buffer: { active: { baseY: 1, viewportY: 0, getLine: (i: number) => ({ translateToString: () => lines[i] }) } },
  };
  expect(screenRows(term as never)).toEqual(["a", "b"]);
});
