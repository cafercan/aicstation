import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import TerminalAppearance from "./TerminalAppearance";
import { DEFAULT_APPEARANCE, getAppearance } from "../terminal/appearance";
import { SCHEME_IDS } from "../terminal/schemes";

test("görünüm paneli: her şema bir kart; seçilen işaretlenir ve kaydedilir", () => {
  render(<TerminalAppearance />);
  const cards = screen.getAllByRole("button", { pressed: false }).concat(screen.getAllByRole("button", { pressed: true }));
  expect(cards.filter((c) => c.classList.contains("scheme-card"))).toHaveLength(SCHEME_IDS.length);
  expect(screen.getByRole("button", { name: /Ubuntu/, pressed: true })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: /Dracula/ }));
  expect(getAppearance().scheme).toBe("dracula");
  expect(screen.getByRole("button", { name: /Dracula/, pressed: true })).toBeTruthy();
  expect(JSON.parse(localStorage.getItem("station.terminal")!).scheme).toBe("dracula");
});

test("görünüm paneli: boyut, imleç ve yanıp sönme", () => {
  render(<TerminalAppearance />);
  fireEvent.change(screen.getByRole("slider", { name: "Boyut" }), { target: { value: "17" } });
  expect(getAppearance().fontSize).toBe(17);
  expect(screen.getByText("17 px")).toBeTruthy();
  fireEvent.click(screen.getByRole("radio", { name: "Çizgi" }));
  expect(getAppearance().cursorStyle).toBe("bar");
  expect(screen.getByRole("radio", { name: "Çizgi" }).getAttribute("aria-checked")).toBe("true");
  fireEvent.click(screen.getByRole("checkbox", { name: "Yanıp sönen imleç" }));
  expect(getAppearance().cursorBlink).toBe(false);
});

test("görünüm paneli: özel yazı tipi; CSS'e kaçabilecek ad kabul edilmez", () => {
  render(<TerminalAppearance />);
  expect(screen.queryByRole("textbox", { name: "Yazı tipi adı" })).toBeNull();
  fireEvent.change(screen.getByRole("combobox", { name: "Yazı tipi" }), { target: { value: "custom" } });
  const box = screen.getByRole("textbox", { name: "Yazı tipi adı" });
  fireEvent.change(box, { target: { value: "Maple Mono" } });
  expect(getAppearance().customFont).toBe("Maple Mono");
  fireEvent.change(box, { target: { value: 'x"; color:red' } });
  expect(getAppearance().customFont).toBe("Maple Mono");
});

test("görünüm paneli: varsayılanlara dön", () => {
  render(<TerminalAppearance />);
  fireEvent.click(screen.getByRole("button", { name: /Nord/ }));
  fireEvent.click(screen.getByRole("button", { name: "Varsayılanlara dön" }));
  expect(getAppearance()).toEqual(DEFAULT_APPEARANCE);
});

test("önizleme: örnek ekranda istemler, renkli çıktı ve 16 renk var", async () => {
  const { sampleScreen } = await import("./TerminalPreview");
  const s = sampleScreen(["P0>", "P1\n>"]);
  expect(s.startsWith("P0>ls --color")).toBe(true);
  expect(s).toContain("\x1b[40m");
  expect(s).toContain("\x1b[107m");
  expect(s.endsWith("P1\r\n>")).toBe(true);
});

test("görünüm paneli: uygulama temasını izle; bir şema seçmek izlemeyi kapatır", async () => {
  const { applyTheme } = await import("../theme");
  applyTheme("nord-dark");
  render(<TerminalAppearance />);
  fireEvent.click(screen.getByRole("checkbox", { name: /uygulama temasını izlesin/ }));
  expect(getAppearance().followApp).toBe(true);
  expect(screen.getByRole("button", { name: /^Nord/, pressed: true })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: /Monokai/ }));
  expect(getAppearance()).toMatchObject({ followApp: false, scheme: "monokai" });
});
