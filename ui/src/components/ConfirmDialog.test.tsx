import { act, fireEvent, render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import ConfirmHost, { askConfirm } from "./ConfirmDialog";

test("an in-app question: OK has the focus, Enter on it agrees, Esc and Cancel refuse", async () => {
  render(<><button type="button">arkada</button><ConfirmHost /></>);
  expect(screen.queryByRole("alertdialog")).toBeNull();
  let answer = askConfirm("Terminal kapatılsın mı?");
  const dlg = await screen.findByRole("alertdialog");
  expect(dlg.textContent).toContain("Terminal kapatılsın mı?");
  const ok = screen.getByRole("button", { name: "Tamam" });
  expect(document.activeElement).toBe(ok);
  // Tab stays in the dialog.
  expect(fireEvent.keyDown(ok, { key: "Tab" })).toBe(false);
  fireEvent.click(ok);
  await expect(answer).resolves.toBe(true);
  expect(screen.queryByRole("alertdialog")).toBeNull();

  answer = askConfirm("İkinci soru");
  fireEvent.keyDown(await screen.findByRole("alertdialog"), { key: "Escape" });
  await expect(answer).resolves.toBe(false);

  answer = askConfirm("Üçüncü");
  fireEvent.click(await screen.findByRole("button", { name: "İptal" }));
  await expect(answer).resolves.toBe(false);
});

test("a new question while one is open refuses the open one", async () => {
  render(<ConfirmHost />);
  const first = askConfirm("bir");
  let second!: Promise<boolean>;
  act(() => { second = askConfirm("iki"); });
  await expect(first).resolves.toBe(false);
  expect((await screen.findByRole("alertdialog")).textContent).toContain("iki");
  fireEvent.click(screen.getByRole("button", { name: "Tamam" }));
  await expect(second).resolves.toBe(true);
});
