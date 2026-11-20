import { fireEvent, screen, within } from "@testing-library/react";

/** Answers the app's confirm dialog (it must be rendered: `ConfirmHost`); returns its text. */
export async function answerConfirm(yes: boolean): Promise<string> {
  const dlg = await screen.findByRole("alertdialog");
  const text = dlg.textContent ?? "";
  fireEvent.click(within(dlg).getByRole("button", { name: yes ? "Tamam" : "İptal" }));
  return text;
}
