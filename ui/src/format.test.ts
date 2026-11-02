import { expect, test } from "vitest";
import { fmtCost, fmtTokens, fmtElapsed } from "./format";

test("format: sayılar ve maliyet", () => {
  expect(fmtTokens(1234)).toBe("1.2k");
  expect(fmtTokens(1_500_000)).toBe("1.50M");
  expect(fmtTokens(12)).toBe("12");
  expect(fmtCost(3.4216)).toBe("$3.42");
});

test("format: geçen süre", () => {
  expect(fmtElapsed(30_000)).toBe("şimdi");
  expect(fmtElapsed(12 * 60_000)).toBe("12 dk");
  expect(fmtElapsed(3 * 3_600_000 + 5 * 60_000)).toBe("3 sa 5 dk");
  expect(fmtElapsed(-5)).toBe("şimdi");
});
