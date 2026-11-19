import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { TermBar } from "./SplitControls";

const OTHERS = [{ id: 1, label: "bir" }, { id: 2, label: "iki" }];

function open(onSplit = vi.fn()) {
  render(<TermBar splitShown={false} others={OTHERS} onSplit={onSplit} onJoin={() => {}} />);
  fireEvent.click(screen.getByRole("button", { name: /Böl/ }));
  return { onSplit, select: screen.getByRole("combobox") as HTMLSelectElement };
}

test("arrows walk the terminals; Enter splits with the one reached", () => {
  const { onSplit, select } = open();
  // Chromium fires change on each arrow of a closed select.
  fireEvent.keyDown(select, { key: "ArrowDown" });
  fireEvent.change(select, { target: { value: "1" } });
  fireEvent.keyDown(select, { key: "ArrowDown" });
  fireEvent.change(select, { target: { value: "2" } });
  expect(onSplit).not.toHaveBeenCalled();
  fireEvent.keyDown(select, { key: "Enter" });
  expect(onSplit).toHaveBeenCalledWith(2);
});

test("a choice made in the opened list (Alt+Down) splits at once", () => {
  const { onSplit, select } = open();
  fireEvent.keyDown(select, { key: "ArrowDown" });
  fireEvent.change(select, { target: { value: "1" } });
  fireEvent.keyDown(select, { key: "ArrowDown", altKey: true });
  fireEvent.change(select, { target: { value: "2" } });
  expect(onSplit).toHaveBeenCalledWith(2);
});

test("a choice made with the mouse splits at once", () => {
  const { onSplit, select } = open();
  fireEvent.mouseDown(select);
  fireEvent.change(select, { target: { value: "new" } });
  expect(onSplit).toHaveBeenCalledWith("new");
});
