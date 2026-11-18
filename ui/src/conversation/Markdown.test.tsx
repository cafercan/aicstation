import { render } from "@testing-library/react";
import { expect, test } from "vitest";
import Markdown from "./Markdown";

function html(text: string) {
  const { container } = render(<Markdown text={text} />);
  return container;
}

test("paragraphs split on blank lines", () => {
  const c = html("one\n\ntwo");
  expect(c.querySelector(".md")).toBeTruthy();
  expect([...c.querySelectorAll("p")].map((p) => p.textContent)).toEqual(["one", "two"]);
});

test("headings level 1 to 3", () => {
  const c = html("# A\n## B\n### C");
  expect(c.querySelector("h1")?.textContent).toBe("A");
  expect(c.querySelector("h2")?.textContent).toBe("B");
  expect(c.querySelector("h3")?.textContent).toBe("C");
});

test("bold, italic and inline code", () => {
  const c = html("a **b** *i* `c`");
  expect(c.querySelector("strong")?.textContent).toBe("b");
  expect(c.querySelector("em")?.textContent).toBe("i");
  expect(c.querySelector("code")?.textContent).toBe("c");
});

test("bold containing inline code; code content is not parsed", () => {
  const c = html("**see `x`** and `**y**`");
  expect(c.querySelector("strong code")?.textContent).toBe("x");
  expect(c.querySelectorAll("strong")).toHaveLength(1);
  expect(c.querySelectorAll("code")[1].textContent).toBe("**y**");
});

test("fenced code block with language", () => {
  const c = html("```ts\nconst a = 1;\n\nb\n```\nafter");
  expect(c.querySelector("pre code")?.textContent).toBe("const a = 1;\n\nb");
  expect(c.querySelector("p")?.textContent).toBe("after");
});

test("unclosed fence renders the rest as code", () => {
  const c = html("```\nrest **x**\nmore");
  expect(c.querySelector("pre code")?.textContent).toBe("rest **x**\nmore");
});

test("unordered and ordered lists", () => {
  const c = html("- a\n* b\n\n1. x\n2. y");
  expect([...c.querySelectorAll("ul li")].map((l) => l.textContent)).toEqual(["a", "b"]);
  expect([...c.querySelectorAll("ol li")].map((l) => l.textContent)).toEqual(["x", "y"]);
});

test("link renders as span with title, never anchor", () => {
  const c = html("[label](https://e.com) [x](javascript:alert(1))");
  const s = c.querySelector("span[title='https://e.com']");
  expect(s?.textContent).toBe("label");
  expect(c.querySelector("a")).toBeNull();
});

test("html stays text", () => {
  const c = html("<script>alert(1)</script> <img src=x onerror=y>");
  expect(c.querySelector("script")).toBeNull();
  expect(c.querySelector("img")).toBeNull();
  expect(c.textContent).toContain("<script>alert(1)</script>");
  expect(c.textContent).toContain("<img src=x onerror=y>");
});

test("unknown constructs and unmatched markers stay plain; long input is fast", () => {
  expect(html("a ** b * c ` d [e](f").textContent).toBe("a ** b * c ` d [e](f");
  const big = "*a ".repeat(50000) + "**" .repeat(20000) + "[".repeat(20000);
  const t = Date.now();
  html(big);
  expect(Date.now() - t).toBeLessThan(3000);
});

test("many unmatched link openers stay fast and keep their text", () => {
  const src = "[a".repeat(30000) + "](x)";
  const t = Date.now();
  const c = html(src);
  expect(Date.now() - t).toBeLessThan(3000);
  expect(c.textContent).toContain("[a[a[a");
  expect(c.querySelector("span[title='x']")?.textContent).toBe("a");
});

const cells = (row: Element) => [...row.children].map((c) => c.textContent);

test("pipe table with header, separator and body rows", () => {
  const c = html("Intro\n\n| Ad | **Durum** |\n|---|:-:|\n| a | `x` |\n| b |\n\nafter");
  const wrap = c.querySelector(".md-table-wrap");
  const table = wrap?.querySelector("table.md-table");
  expect(table).toBeTruthy();
  expect(cells(table!.querySelector("thead tr")!)).toEqual(["Ad", "Durum"]);
  expect(table!.querySelector("thead th strong")?.textContent).toBe("Durum");
  const rows = [...table!.querySelectorAll("tbody tr")];
  expect(rows.map(cells)).toEqual([["a", "x"], ["b", ""]]);
  expect(rows[0].querySelector("td code")?.textContent).toBe("x");
  expect([...c.querySelectorAll("p")].map((p) => p.textContent)).toEqual(["Intro", "after"]);
});

test("table without outer pipes, extra cells dropped, escaped pipe kept", () => {
  const c = html("a | b\n--- | ---\nx | y | z\nm \\| n | o");
  const rows = [...c.querySelectorAll("table tbody tr")];
  expect(cells(c.querySelector("thead tr")!)).toEqual(["a", "b"]);
  expect(rows.map(cells)).toEqual([["x", "y"], ["m | n", "o"]]);
});

test("table right after a paragraph line ends the paragraph", () => {
  const c = html("Sonuç:\n| k | v |\n| - | - |\n| 1 | 2 |");
  expect(c.querySelector("p")?.textContent).toBe("Sonuç:");
  expect(c.querySelectorAll("tbody tr")).toHaveLength(1);
});

test("pipes without a separator row stay a paragraph", () => {
  const c = html("| a | b |\n| c | d |");
  expect(c.querySelector("table")).toBeNull();
  expect(c.querySelector("p")?.textContent).toBe("| a | b |\n| c | d |");
  // A separator with a different cell count is no table either.
  expect(html("| a | b |\n| --- |\n| c | d |").querySelector("table")).toBeNull();
});

test("a big table stays fast", () => {
  const src = "| a | b |\n|---|---|\n" + "| x | y |\n".repeat(2000) + "|".repeat(100000) + "\n" + "\\|".repeat(100000)
    + "\n\n" + "a|b\n".repeat(30000);
  const t = Date.now();
  const c = html(src);
  expect(Date.now() - t).toBeLessThan(3000);
  expect(c.querySelectorAll("tbody tr").length).toBe(2002);
  // Pipe lines without a separator row: one paragraph, each line checked once.
  expect(c.querySelectorAll("p")).toHaveLength(1);
});
