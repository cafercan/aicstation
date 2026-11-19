import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import ImageViewer from "./ImageViewer";

// jsdom lays nothing out: the image box's rect is all zeros, so its centre
// (the zoom origin) is at (0, 0) and cursor positions are offsets from it.

/** `transform` as the DOM writes it back (jsdom shortens `translate(0px, 0px)`). */
const tf = (s: string) => { const d = document.createElement("div"); d.style.transform = s; return d.style.transform; };

const view = () => {
  const onClose = vi.fn();
  render(<ImageViewer src="data:image/png;base64,QUJD" onClose={onClose} />);
  const img = screen.getByRole("dialog").querySelector("img")!;
  return { onClose, img, box: img.parentElement!, dialog: screen.getByRole("dialog") };
};

test("Ctrl+wheel zooms in and out between fit and 8x, and is not passed on", () => {
  const { img, dialog } = view();
  expect(img.style.transform).toBe(tf("translate(0px, 0px) scale(1)"));
  expect(fireEvent.wheel(img, { deltaY: -100, ctrlKey: true })).toBe(false);
  expect(img.style.transform).toBe(tf("translate(0px, 0px) scale(1.25)"));
  expect(dialog.textContent).toContain("125%");
  fireEvent.wheel(img, { deltaY: 100, ctrlKey: true });
  expect(img.style.transform).toBe(tf("translate(0px, 0px) scale(1)"));
  expect(dialog.textContent).not.toContain("%");
  // Never smaller than the fitted size, never larger than 8x.
  fireEvent.wheel(img, { deltaY: 100, ctrlKey: true });
  expect(img.style.transform).toBe(tf("translate(0px, 0px) scale(1)"));
  for (let i = 0; i < 20; i++) fireEvent.wheel(img, { deltaY: -100, ctrlKey: true });
  expect(img.style.transform).toBe(tf("translate(0px, 0px) scale(8)"));
});

test("a wheel without Ctrl does not zoom", () => {
  const { img } = view();
  expect(fireEvent.wheel(img, { deltaY: -100 })).toBe(true);
  expect(img.style.transform).toBe(tf("translate(0px, 0px) scale(1)"));
});

test("zoom keeps the point under the cursor in place", () => {
  const { img } = view();
  // Cursor 100 px right, 40 px below the centre: after 1.25x that point has
  // moved 25 / 10 px away, so the image shifts back by that much.
  fireEvent.wheel(img, { deltaY: -100, ctrlKey: true, clientX: 100, clientY: 40 });
  expect(img.style.transform).toBe(tf("translate(-25px, -10px) scale(1.25)"));
  // Back at the fitted size the image is centred again.
  fireEvent.wheel(img, { deltaY: 100, ctrlKey: true, clientX: 300, clientY: 300 });
  expect(img.style.transform).toBe(tf("translate(0px, 0px) scale(1)"));
});

test("a zoomed image is dragged, and the click ending the drag does not close", () => {
  const { img, box, onClose } = view();
  fireEvent.wheel(img, { deltaY: -100, ctrlKey: true });
  fireEvent.mouseDown(img, { button: 0, clientX: 10, clientY: 10 });
  fireEvent.mouseMove(window, { clientX: 40, clientY: 30 });
  fireEvent.mouseUp(window, { clientX: 40, clientY: 30 });
  expect(img.style.transform).toBe(tf("translate(30px, 20px) scale(1.25)"));
  fireEvent.click(box);
  expect(onClose).not.toHaveBeenCalled();
});

test("at the fitted size the image is not dragged and a backdrop click closes", () => {
  const { img, box, onClose } = view();
  fireEvent.mouseDown(img, { button: 0, clientX: 10, clientY: 10 });
  fireEvent.mouseMove(window, { clientX: 40, clientY: 30 });
  fireEvent.mouseUp(window, { clientX: 40, clientY: 30 });
  expect(img.style.transform).toBe(tf("translate(0px, 0px) scale(1)"));
  fireEvent.click(box);
  expect(onClose).toHaveBeenCalledTimes(1);
});

test("+, - and 0 zoom from the keyboard", () => {
  const { img, onClose } = view();
  fireEvent.keyDown(document.body, { key: "+" });
  expect(img.style.transform).toBe(tf("translate(0px, 0px) scale(1.25)"));
  fireEvent.keyDown(document.body, { key: "=" });
  expect(img.style.transform).toBe(tf("translate(0px, 0px) scale(1.5625)"));
  fireEvent.keyDown(document.body, { key: "-" });
  expect(img.style.transform).toBe(tf("translate(0px, 0px) scale(1.25)"));
  fireEvent.keyDown(document.body, { key: "0" });
  expect(img.style.transform).toBe(tf("translate(0px, 0px) scale(1)"));
  expect(onClose).not.toHaveBeenCalled();
});
