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
