import { render } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import QuoteMenu from "./QuoteMenu";

const proto = HTMLElement.prototype;
const noop = () => {};

afterEach(() => {
  delete (proto as unknown as Record<string, unknown>).offsetHeight;
});

test("a menu opened near the bottom of the window stays inside it", () => {
  Object.defineProperty(proto, "offsetHeight", { configurable: true, get() { return 60; } });
  const h = window.innerHeight;
  const { container } = render(<QuoteMenu x={10} y={h - 10} onQuote={noop} onCopy={noop} onClose={noop} />);
  const menu = container.querySelector(".conv-menu") as HTMLDivElement;
  expect(menu.style.top).toBe(`${h - 60}px`);
});

test("a menu with room below opens at the pointer", () => {
  Object.defineProperty(proto, "offsetHeight", { configurable: true, get() { return 60; } });
  const { container } = render(<QuoteMenu x={10} y={100} onQuote={noop} onCopy={noop} onClose={noop} />);
  expect((container.querySelector(".conv-menu") as HTMLDivElement).style.top).toBe("100px");
});
