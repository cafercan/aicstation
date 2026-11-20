import { expect, test } from "vitest";
import { findModeLine, findSpinner, inputImages, screenRows } from "./statusLine";

test("finds the spinner, ASCII star included, and drops key hints", () => {
  expect(findSpinner(["", "* Fermenting… (33s · ↓ 2.8k tokens · thinking with high effort)", "  ⎿ Tip: x", "> "]))
    .toBe("* Fermenting… (33s · ↓ 2.8k tokens · thinking with high effort)");
  expect(findSpinner(["✽ Prestidigitating… (46s · ↓ 3.9k tokens) · esc to interrupt"]))
    .toBe("✽ Prestidigitating… (46s · ↓ 3.9k tokens)");
});

test("a spinner of several words is a spinner too", () => {
  expect(findSpinner(["✻ Compacting conversation… (12s)"])).toBe("✻ Compacting conversation… (12s)");
  expect(findSpinner(["✻ Compacting conversation…"])).toBe("✻ Compacting conversation…");
  // A reply line with an ellipsis is not.
  expect(findSpinner(["● Okay, I will look… (later)"])).toBeNull();
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

test("a key hint inside the spinner's parentheses is dropped too", () => {
  expect(findSpinner(["✻ Fermenting… (33s · ↓ 2.8k tokens · esc to interrupt)"])).toBe("✻ Fermenting… (33s · ↓ 2.8k tokens)");
  expect(findSpinner(["✻ Fermenting… (esc to interrupt)"])).toBe("✻ Fermenting…");
});

test("only a key hint in parentheses leaves the mode line, not other text", () => {
  expect(findModeLine([...BOX, "⏵⏵ auto mode on · 2 shells (1 running)"])).toBe("⏵⏵ auto mode on · 2 shells (1 running)");
  expect(findModeLine([...BOX, "⏵⏵ auto mode on · 1 shell (ctrl+b to run in background)"])).toBe("⏵⏵ auto mode on · 1 shell");
});

test("a spinner wrapped onto the next row is read whole", () => {
  const lines = [
    { text: "✻ Fermenting… (33s · ↓ 2.8k to", wrapped: false },
    { text: "kens · thinking with high effort)", wrapped: true },
    { text: "next", wrapped: false },
  ];
  const term = {
    rows: 3,
    buffer: { active: { baseY: 0, getLine: (i: number) => ({ isWrapped: lines[i].wrapped, translateToString: () => lines[i].text }) } },
  };
  const rows = screenRows(term as never);
  expect(rows).toEqual(["✻ Fermenting… (33s · ↓ 2.8k tokens · thinking with high effort)", "next"]);
  expect(findSpinner(rows)).toBe("✻ Fermenting… (33s · ↓ 2.8k tokens · thinking with high effort)");
});

test("screenRows reads the screen at the bottom, not the scrolled view", () => {
  const lines = ["old", "a", "b"];
  const term = {
    rows: 2,
    buffer: { active: { baseY: 1, viewportY: 0, getLine: (i: number) => ({ translateToString: () => lines[i] }) } },
  };
  expect(screenRows(term as never)).toEqual(["a", "b"]);
});

test("the images waiting in Claude's input box are read from its rows", () => {
  const rule = "─".repeat(40);
  const rows = [
    "> [Image #1] gönderilmiş eski mesaj",
    rule,
    "> [Image #2] [Image #3] şuna bak",
    "  [Image #3]",
    rule,
    "  ⏵⏵ auto mode on",
  ];
  expect(inputImages(rows)).toEqual([2, 3]);
  expect(inputImages([rule, "> boş", rule])).toEqual([]);
  expect(inputImages(["> [Image #4]"])).toEqual([]);
});

test("a full-width rule does not swallow the row xterm marks as wrapped after it", () => {
  const rule = "─".repeat(244);
  const lines = [
    { text: rule, wrapped: false },
    { text: "❯ [Image #1]", wrapped: true },
    { text: rule, wrapped: false },
    { text: "  ⏵⏵ auto mode on", wrapped: false },
  ];
  const term = {
    rows: 4,
    buffer: { active: { baseY: 0, getLine: (i: number) => ({ isWrapped: lines[i].wrapped, translateToString: () => lines[i].text }) } },
  };
  const rows = screenRows(term as never);
  expect(rows).toEqual([rule, "❯ [Image #1]", rule, "  ⏵⏵ auto mode on"]);
  expect(inputImages(rows)).toEqual([1]);
});
