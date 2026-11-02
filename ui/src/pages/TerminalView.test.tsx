import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";

const resizeTerminal = vi.fn();
vi.mock("../terminal/store", () => ({ resizeTerminal: (...a: unknown[]) => resizeTerminal(...a), setFontSize: vi.fn() }));
vi.mock("../terminal/render", () => ({ enableWebgl: vi.fn() }));

import TerminalView from "./TerminalView";
import { focusSearch } from "../shortcuts/bus";
import type { TerminalEntry } from "../terminal/store";

afterEach(() => { resizeTerminal.mockReset(); vi.useRealTimers(); });

function entry(id: number): TerminalEntry {
  const term = {
    open: vi.fn(), focus: vi.fn(), options: {}, attachCustomKeyEventHandler: vi.fn(),
    hasSelection: () => false, getSelection: () => "", clearSelection: vi.fn(), paste: vi.fn(),
  };
  const search = {
    findNext: vi.fn(), findPrevious: vi.fn(), clearDecorations: vi.fn(),
    onDidChangeResults: () => ({ dispose: () => {} }),
  };
  return { id, pid: id, n: id, term, fit: {}, search } as unknown as TerminalEntry;
}

test("bölmede: kutu yüzdesi, etkin bölmede odak halkası ve klavye odağı; tıklama etkinleştirir", () => {
  const a = entry(1);
  const b = entry(2);
  const onActivate = vi.fn();
  const { container } = render(<>
    <TerminalView entry={a} visible pane={{ left: 0, width: 40 }} active />
    <TerminalView entry={b} visible pane={{ left: 40, width: 60 }} active={false} onActivate={onActivate} />
  </>);
  const [wa, wb] = [...container.querySelectorAll<HTMLElement>(".term-wrap")];
  expect(wa.className).toBe("term-wrap split active");
  expect(wb.className).toBe("term-wrap split");
  expect(wb.style.left).toBe("40%");
  expect(wb.style.width).toBe("60%");
  expect(a.term.focus).toHaveBeenCalled();
  expect(b.term.focus).not.toHaveBeenCalled();
  fireEvent.mouseDown(wb);
  expect(onActivate).toHaveBeenCalled();
  // Both panes were fitted (pty_resize) when shown.
  expect(resizeTerminal).toHaveBeenCalledWith(a);
  expect(resizeTerminal).toHaveBeenCalledWith(b);
});

test("tek terminal: tam boyut, bölme sınıfı yok", () => {
  const { container } = render(<TerminalView entry={entry(1)} visible />);
  const w = container.querySelector<HTMLElement>(".term-wrap")!;
  expect(w.className).toBe("term-wrap");
  expect(w.style.width).toBe("");
});

test("Ctrl+Shift+F hedefi: yalnızca görünen etkin bölmenin arama çubuğu açılır", () => {
  render(<>
    <TerminalView entry={entry(1)} visible={false} />
    <TerminalView entry={entry(2)} visible pane={{ left: 0, width: 50 }} active={false} />
    <TerminalView entry={entry(3)} visible pane={{ left: 50, width: 50 }} active />
  </>);
  act(() => { expect(focusSearch()).toBe(true); });
  const bars = screen.getAllByRole("searchbox");
  expect(bars).toHaveLength(1);
  expect(bars[0].closest("[data-terminal]")!.getAttribute("data-terminal")).toBe("3");
});
