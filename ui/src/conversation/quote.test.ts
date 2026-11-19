import { expect, test } from "vitest";
import { appendQuote, quoteText } from "./quote";

test("quoteText: every line gets a quote mark, blank lines a bare one", () => {
  expect(quoteText("tek satır")).toBe("> tek satır");
  expect(quoteText("bir\r\niki\n\nüç")).toBe("> bir\n> iki\n>\n> üç");
});

test("quoteText: surrounding blank lines and spaces are dropped", () => {
  expect(quoteText("\n\n  metin  \n\n")).toBe("> metin");
});

test("appendQuote: into an empty box the quote and a blank line to answer under", () => {
  expect(appendQuote("", "> a")).toBe("> a\n\n");
  expect(appendQuote(" \n", "> a")).toBe("> a\n\n");
});

test("appendQuote: after text already in the box, one blank line between", () => {
  expect(appendQuote("önce", "> a")).toBe("önce\n\n> a\n\n");
  expect(appendQuote("önce\n\n\n", "> a")).toBe("önce\n\n> a\n\n");
});
