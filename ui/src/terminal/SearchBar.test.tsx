import { act, fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import SearchBar, { type SearchLike } from "./SearchBar";
import { setLang } from "../i18n";

function fakeSearch() {
  let cb: (r: { resultIndex: number; resultCount: number }) => void = () => {};
  const s = {
    findNext: vi.fn(() => true),
    findPrevious: vi.fn(() => true),
    clearDecorations: vi.fn(),
    onDidChangeResults: (f: typeof cb) => { cb = f; return { dispose() {} }; },
    emit: (r: { resultIndex: number; resultCount: number }) => cb(r),
  };
  return s;
}

function setup() {
  const search = fakeSearch();
  const onClose = vi.fn();
  render(<SearchBar search={search as unknown as SearchLike} onClose={onClose} />);
  const input = screen.getByRole("searchbox", { name: "Terminalde ara" }) as HTMLInputElement;
  return { search, onClose, input };
}

test("açılınca odaklanır; yazdıkça ilerleyerek arar", () => {
  const { search, input } = setup();
  expect(document.activeElement).toBe(input);
  fireEvent.change(input, { target: { value: "hata" } });
  expect(search.findNext).toHaveBeenLastCalledWith("hata", expect.objectContaining({ incremental: true }));
});

test("Enter sonraki, Shift+Enter önceki eşleşme", () => {
  const { search, input } = setup();
  fireEvent.change(input, { target: { value: "x" } });
  fireEvent.keyDown(input, { key: "Enter" });
  expect(search.findNext).toHaveBeenLastCalledWith("x", expect.not.objectContaining({ incremental: true }));
  fireEvent.keyDown(input, { key: "Enter", shiftKey: true });
  expect(search.findPrevious).toHaveBeenCalledWith("x", expect.anything());
});

test("Esc kapatır ve vurguları temizler", () => {
  const { search, onClose, input } = setup();
  fireEvent.keyDown(input, { key: "Escape" });
  expect(search.clearDecorations).toHaveBeenCalled();
  expect(onClose).toHaveBeenCalled();
});

test("sonuç sayısı Türkçe gösterilir", () => {
  const { search, input } = setup();
  fireEvent.change(input, { target: { value: "x" } });
  act(() => search.emit({ resultIndex: 1, resultCount: 4 }));
  expect(screen.getByText("2/4")).toBeTruthy();
  act(() => search.emit({ resultIndex: -1, resultCount: 0 }));
  expect(screen.getByText("Sonuç yok")).toBeTruthy();
  expect(screen.getByTitle("Sonraki (Enter)")).toBeTruthy();
  expect(screen.getByTitle("Önceki (Shift+Enter)")).toBeTruthy();
  fireEvent.click(screen.getByTitle("Kapat (Esc)"));
});

test("arama çubuğu seçili dilde", () => {
  setLang("en");
  const search = fakeSearch();
  render(<SearchBar search={search as unknown as SearchLike} onClose={() => {}} />);
  const input = screen.getByRole("searchbox", { name: "Search terminal" }) as HTMLInputElement;
  expect(input.placeholder).toBe("Search…");
  fireEvent.change(input, { target: { value: "x" } });
  act(() => search.emit({ resultIndex: -1, resultCount: 0 }));
  expect(screen.getByText("No results")).toBeTruthy();
  expect(screen.getByTitle("Next (Enter)")).toBeTruthy();
  expect(screen.getByTitle("Previous (Shift+Enter)")).toBeTruthy();
  expect(screen.getByTitle("Close (Esc)")).toBeTruthy();
});
