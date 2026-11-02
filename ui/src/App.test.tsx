import { render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import App from "./App";

test("uygulama başlığını gösterir", () => {
  render(<App />);
  expect(screen.getByText("Claude Pano")).toBeTruthy();
});
