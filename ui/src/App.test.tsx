import { render, screen, waitFor } from "@testing-library/react";
import { expect, test, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string) =>
    cmd === "get_snapshot"
      ? { sessions: [], runningAgents: 0, waitingCount: 0 }
      : { rows: [], todayCost: 0, todayComplete: true, indexing: false },
  ),
}));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

import App from "./App";

test("iskelet: menü ve status bar görünür", async () => {
  render(<App />);
  expect(screen.getByText("▦ Dashboard")).toBeTruthy();
  await waitFor(() => expect(screen.getByText("0 açık oturum")).toBeTruthy());
});
