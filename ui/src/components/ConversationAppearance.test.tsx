import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import ConversationAppearance from "./ConversationAppearance";
import { getConvFontSize, setConvFontSize } from "../conversation/convAppearance";

test("conversation panel: the font size slider sets and shows the size", () => {
  setConvFontSize(13);
  render(<ConversationAppearance />);
  expect(screen.getByRole("group", { name: "Sohbet görünümü" })).toBeTruthy();
  const slider = screen.getByRole("slider", { name: "Yazı boyutu" });
  fireEvent.change(slider, { target: { value: "16" } });
  expect(getConvFontSize()).toBe(16);
  expect(screen.getByText("16 px")).toBeTruthy();
  setConvFontSize(13);
});
