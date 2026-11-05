import { act, render, screen } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";
import type { ClaudeStatus, SessionCard } from "../api";

const h = vi.hoisted(() => ({
  status: null as ClaudeStatus | null,
  dirty: null as boolean | null,
}));
const invoke = vi.hoisted(() => vi.fn(async (cmd: string): Promise<unknown> => {
  if (cmd === "claude_status") return h.status;
  if (cmd === "git_dirty") return h.dirty;
  return undefined;
}));
vi.mock("@tauri-apps/api/core", () => ({ invoke, Channel: class {} }));

import ClaudeBar from "./ClaudeBar";
import { fmtReset, limitView, meter, shortModel, STATUS_POLL_MS } from "./claudeBar";
import { resetUiState } from "../testing/reset";
import { setLang } from "../i18n";

const NOW = 1_800_000_000_000;
const H = 3_600_000;

function card(over: Partial<SessionCard> = {}): SessionCard {
  return {
    sessionId: "s1", pid: 1, parentPid: 10, name: "demo", cwd: "/home/u/aicstation", status: "busy",
    waitingFor: null, model: "claude-opus-5-5", contextPct: 62, costUsd: 1.2,
    tokens: { input: 1_000_000, output: 200_000, cacheRead: 0, cacheCreation: 0, cacheCreation1h: 0 },
    agentsTotal: 0, agentsFinished: 0, toolOk: 0, toolErr: 0, lastTool: null, startedAtMs: 0, lastActivityMs: 0,
    gitBranch: "main", hostKind: "embedded", ...over,
  };
}

const flush = () => act(async () => { await Promise.resolve(); await Promise.resolve(); });

beforeEach(() => {
  resetUiState();
  setLang("en");
  h.status = null;
  h.dirty = null;
  invoke.mockClear();
});

test("fmtReset: compact minutes, hours with minutes, days", () => {
  expect(fmtReset(30_000, "en")).toBe("1m");
  expect(fmtReset(45 * 60_000, "en")).toBe("45m");
  expect(fmtReset(2 * H + 10 * 60_000, "en")).toBe("2h10");
  expect(fmtReset(2 * H + 5 * 60_000, "en")).toBe("2h05");
  expect(fmtReset(3 * H, "en")).toBe("3h");
  expect(fmtReset(3 * 24 * H, "en")).toBe("3d");
  expect(fmtReset(2 * H + 10 * 60_000, "tr")).toBe("2sa10");
});

test("limitView: percent left, meter cells, level; a passed reset hides the window", () => {
  expect(limitView({ usedPct: 42, resetsAtMs: NOW + H }, NOW)).toEqual({ left: 58, filled: 6, resetIn: H, level: "ok" });
  expect(limitView({ usedPct: 75, resetsAtMs: null }, NOW)).toMatchObject({ left: 25, level: "low", resetIn: null });
  expect(limitView({ usedPct: 95, resetsAtMs: null }, NOW)?.level).toBe("critical");
  expect(limitView({ usedPct: 140, resetsAtMs: null }, NOW)).toMatchObject({ left: 0, filled: 0 });
  expect(limitView({ usedPct: 10, resetsAtMs: NOW }, NOW)).toBeNull();
  expect(limitView(null, NOW)).toBeNull();
  expect(meter(6)).toBe("══════────");
  expect(shortModel("claude-opus-5-5")).toBe("opus-5-5");
  expect(shortModel(null)).toBeNull();
});

test("bar without the bridge: model, folder and branch, context, tokens; no limits", async () => {
  h.status = { bridge: false, limits: null, sessions: {}, defaultEffort: "high" };
  render(<ClaudeBar session={card()} now={NOW} />);
  await flush();
  const bar = screen.getByTestId("claude-bar");
  expect(bar.textContent).toBe("opus-5-5 high │ aicstation (main) │ ctx 62% │ 1.2M tok ~$1.20 est.");
  expect(screen.queryByTestId("cbar-fiveHour")).toBeNull();
  expect(invoke).toHaveBeenCalledWith("git_dirty", { cwd: "/home/u/aicstation" });
});

test("bar with the bridge: limits with reset, session effort and lines, dirty marker", async () => {
  h.dirty = true;
  h.status = {
    bridge: true,
    limits: { fiveHour: { usedPct: 42, resetsAtMs: NOW + 2 * H + 10 * 60_000 }, sevenDay: { usedPct: 59, resetsAtMs: NOW + 3 * 24 * H }, updatedMs: NOW },
    sessions: { s1: { effort: "max", linesAdded: 120, linesRemoved: 30, updatedMs: NOW } },
    defaultEffort: "high",
  };
  render(<ClaudeBar session={card()} now={NOW} />);
  await flush();
  expect(screen.getByTestId("claude-bar").textContent).toBe(
    "opus-5-5 max │ aicstation (main*) │ ctx 62% │ 5h ══════──── 58% ⟳ 2h10 │ 7d ════────── 41% ⟳ 3d │ 1.2M tok ~$1.20 est. │ +120/-30",
  );
  expect(screen.getByTestId("cbar-fiveHour").getAttribute("title")).toBe("5-hour limit: 58% left, resets in 2h10");
});

test("unknown parts are left out", async () => {
  render(<ClaudeBar session={card({ model: null, contextPct: null, gitBranch: null, costUsd: 0,
    tokens: { input: 0, output: 0, cacheRead: 0, cacheCreation: 0, cacheCreation1h: 0 } })} now={NOW} />);
  await flush();
  expect(screen.getByTestId("claude-bar").textContent).toBe("aicstation");
});

test("one shared poll for all bars, stopped when none is shown", async () => {
  vi.useFakeTimers();
  try {
    h.status = { bridge: false, limits: null, sessions: {}, defaultEffort: null };
    const a = render(<><ClaudeBar session={card()} now={NOW} /><ClaudeBar session={card({ sessionId: "s2" })} now={NOW} /></>);
    const polls = () => invoke.mock.calls.filter((c) => c[0] === "claude_status").length;
    expect(polls()).toBe(1);
    await act(async () => { vi.advanceTimersByTime(STATUS_POLL_MS); });
    expect(polls()).toBe(2);
    a.unmount();
    await act(async () => { vi.advanceTimersByTime(3 * STATUS_POLL_MS); });
    expect(polls()).toBe(2);
  } finally {
    vi.useRealTimers();
  }
});
