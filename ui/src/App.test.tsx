import { render, screen, waitFor } from "@testing-library/react";
import { expect, test, vi } from "vitest";

const invoke = vi.hoisted(() => vi.fn(async (cmd: string, _args?: unknown): Promise<unknown> => {
  if (cmd === "get_snapshot") return { sessions: [], runningAgents: 0, waitingCount: 0 };
  if (cmd === "pty_list") return [{ id: 2, pid: 20 }];
  if (cmd === "pty_kill") return undefined;
  return { rows: [], todayCost: 0, todayTokens: 0, todayComplete: true, indexing: false };
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: class {} }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

vi.mock("@xterm/xterm", () => ({ Terminal: class {} }));
vi.mock("@xterm/addon-fit", () => ({ FitAddon: class {} }));

import App from "./App";

test("iskelet: menü ve status bar görünür", async () => {
  render(<App />);
  expect(screen.getByText("▦ Dashboard")).toBeTruthy();
  await waitFor(() => expect(screen.getByText("0 açık oturum")).toBeTruthy());
});

test("açılışta önceki sayfadan kalan PTY'ler kapatılır", async () => {
  render(<App />);
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("pty_kill", { id: 2 }));
});
