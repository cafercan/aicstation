import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import CommandPalette from "./CommandPalette";
import ShortcutsHelp from "./ShortcutsHelp";

test("Tab does not leave the command palette", () => {
  render(<><button type="button">arkada</button><CommandPalette items={[]} onClose={() => {}} /></>);
  const box = screen.getByRole("combobox");
  expect(document.activeElement).toBe(box);
  const ev = fireEvent.keyDown(box, { key: "Tab" });
  expect(ev).toBe(false);
  expect(document.activeElement).toBe(box);
});

test("Tab does not leave the shortcuts dialog", () => {
  render(<><button type="button">arkada</button><ShortcutsHelp onClose={() => {}} /></>);
  const close = screen.getByRole("dialog").querySelector("button")!;
  expect(document.activeElement).toBe(close);
  expect(fireEvent.keyDown(close, { key: "Tab" })).toBe(false);
  expect(document.activeElement).toBe(close);
});
