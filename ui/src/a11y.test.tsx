import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { pressable } from "./a11y";

test("a pressable element takes the focus and runs on Enter and Space", () => {
  const fn = vi.fn();
  render(<div {...pressable(fn)}>satır</div>);
  const el = screen.getByRole("button", { name: "satır" });
  expect(el.tabIndex).toBe(0);
  fireEvent.keyDown(el, { key: "Enter" });
  fireEvent.keyDown(el, { key: " " });
  fireEvent.keyDown(el, { key: "a" });
  fireEvent.click(el);
  expect(fn).toHaveBeenCalledTimes(3);
});

test("a control inside it acts alone", () => {
  const row = vi.fn();
  const inner = vi.fn();
  render(<div {...pressable(row)}>satır <span {...pressable(inner)}>×</span></div>);
  const x = screen.getByRole("button", { name: "×" });
  fireEvent.keyDown(x, { key: "Enter" });
  fireEvent.click(x);
  expect(inner).toHaveBeenCalledTimes(2);
  expect(row).not.toHaveBeenCalled();
});
