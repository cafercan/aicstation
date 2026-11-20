import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import ContextMenu from "./ContextMenu";

function menu() {
  const calls: string[] = [];
  const onClose = vi.fn(() => calls.push("close"));
  render(<ContextMenu x={10} y={10} label="İşlemler" onClose={onClose} items={[
    { label: "Bir", onSelect: () => calls.push("bir") },
    { label: "İki", disabled: true, onSelect: () => calls.push("iki") },
    { label: "Sil", danger: true, separated: true, onSelect: () => calls.push("sil") },
  ]} />);
  return { calls, onClose, items: screen.getAllByRole("menuitem") };
}

test("the first item has the focus; arrows, Home and End skip disabled items", () => {
  const { items } = menu();
  expect(screen.getByRole("menu", { name: "İşlemler" })).toBeTruthy();
  expect(document.activeElement).toBe(items[0]);
  fireEvent.keyDown(items[0], { key: "ArrowDown" });
  expect(document.activeElement).toBe(items[2]);
  fireEvent.keyDown(items[2], { key: "ArrowDown" });
  expect(document.activeElement).toBe(items[0]);
  fireEvent.keyDown(items[0], { key: "ArrowUp" });
  expect(document.activeElement).toBe(items[2]);
  fireEvent.keyDown(items[2], { key: "Home" });
  expect(document.activeElement).toBe(items[0]);
  fireEvent.keyDown(items[0], { key: "End" });
  expect(document.activeElement).toBe(items[2]);
  expect(screen.getByRole("separator")).toBeTruthy();
  expect(items[2].className).toBe("danger");
});

test("picking an item closes the menu, then runs it; Esc and a press outside only close", () => {
  const { calls, onClose, items } = menu();
  fireEvent.click(items[2]);
  expect(calls).toEqual(["close", "sil"]);
  fireEvent.keyDown(window, { key: "Escape" });
  fireEvent.mouseDown(document.body);
  expect(onClose).toHaveBeenCalledTimes(3);
  expect(calls).toEqual(["close", "sil", "close", "close"]);
});
