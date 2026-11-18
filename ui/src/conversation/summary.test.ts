import { expect, test } from "vitest";
import type { ConvStep } from "../api";
import { t } from "../i18n";
import { stepSummary } from "./summary";

const st = (tool: string, target = ""): ConvStep => ({ tool, target, done: true, isError: false, images: [] });
const en = (k: Parameters<typeof t>[0], p?: Parameters<typeof t>[1]) => t(k, p, "en");
const tr = (k: Parameters<typeof t>[0], p?: Parameters<typeof t>[1]) => t(k, p, "tr");

test("counts per kind in first-seen order", () => {
  expect(stepSummary([st("Bash"), st("Bash"), st("Read", "grid.jpg")], en)).toBe("Ran 2 commands, read grid.jpg");
});

test("singular and plural forms", () => {
  expect(stepSummary([st("Bash")], en)).toBe("Ran a command");
  expect(stepSummary([st("Read", "a"), st("Read", "b")], en)).toBe("Read 2 files");
  expect(stepSummary([st("Edit", "a.ts"), st("Write", "b.ts")], en)).toBe("Edited 2 files");
  expect(stepSummary([st("MultiEdit", "a.ts")], en)).toBe("Edited a.ts");
  expect(stepSummary([st("Grep"), st("Glob")], en)).toBe("Searched");
});

test("unknown tools once per distinct name", () => {
  expect(stepSummary([st("WebFetch"), st("WebFetch")], en)).toBe("Used WebFetch");
  expect(stepSummary([st("WebFetch"), st("Task"), st("WebFetch")], en)).toBe("Used WebFetch, used Task");
});

test("Turkish", () => {
  expect(stepSummary([st("Bash"), st("Bash"), st("Read", "grid.jpg")], tr)).toBe("2 komut çalıştırdı, grid.jpg okundu");
});

test("empty", () => {
  expect(stepSummary([], en)).toBe("");
});
