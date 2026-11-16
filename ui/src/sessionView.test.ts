import { expect, test } from "vitest";
import type { SessionCard } from "./api";
import type { View } from "./components/Sidebar";
import type { TerminalEntry } from "./terminal/store";
import { moveState, openSessionView, resolveView, tabBarFor } from "./sessionView";

function card(over: Partial<SessionCard>): SessionCard {
  return {
    sessionId: "a", pid: 100, parentPid: 10, name: "a", cwd: "C:/w", status: "idle", waitingFor: null,
    model: null, contextPct: null, costUsd: 0,
    tokens: { input: 0, output: 0, cacheRead: 0, cacheCreation: 0, cacheCreation1h: 0 },
    agentsTotal: 0, agentsFinished: 0, toolOk: 0, toolErr: 0, lastTool: null, startedAtMs: 0, lastActivityMs: 0,
    gitBranch: null, hostKind: "other", movable: true, agentsRunning: 0, ...over,
  };
}

const term = (id: number, pid: number) => ({ id, pid, n: id }) as unknown as TerminalEntry;
const NONE: ReadonlySet<number> = new Set();

test("card_click_in_app_terminal_opens_terminal_tab", () => {
  const sessions = [card({ sessionId: "a", parentPid: 10, hostKind: "embedded" })];
  const terminalFor = (p: number | null | undefined) => (p === 10 ? 1 : undefined);
  expect(openSessionView("a", sessions, terminalFor)).toEqual({ terminal: 1 });
});

test("card_click_external_opens_info_tab", () => {
  const sessions = [card({ sessionId: "a", parentPid: 77, hostKind: "windowsTerminal" })];
  expect(openSessionView("a", sessions, () => undefined)).toEqual({ session: "a", tab: "info" });
});

test("card_click_closed_opens_info_tab", () => {
  const sessions = [card({ sessionId: "a", pid: 0, parentPid: null })];
  expect(openSessionView("a", sessions, () => 1)).toEqual({ session: "a", tab: "info" });
  // Not in the snapshot at all (History row).
  expect(openSessionView("z", [], () => 1)).toEqual({ session: "z", tab: "info" });
});

test("follows_new_session_in_same_terminal", () => {
  const v: View = { session: "a", tab: "info", terminal: 1 };
  const sessions = [card({ sessionId: "b", parentPid: 10 })];
  expect(resolveView(v, [term(1, 10)], sessions, NONE)).toEqual({ session: "b", tab: "info", terminal: 1 });
});

test("info_view_becomes_plain_terminal_when_claude_exits", () => {
  const v: View = { session: "a", tab: "info", terminal: 1 };
  expect(resolveView(v, [term(1, 10)], [], NONE)).toEqual({ terminal: 1 });
});

test("terminal_gone_falls_back_to_info", () => {
  const v: View = { session: "a", tab: "terminal", terminal: 1 };
  expect(resolveView(v, [], [], NONE)).toEqual({ session: "a", tab: "info" });
});

test("terminal_tab_without_terminal_follows_session_into_app_terminal", () => {
  const v: View = { session: "a", tab: "terminal" };
  const sessions = [card({ sessionId: "a", parentPid: 20 })];
  expect(resolveView(v, [term(1, 10), term(2, 20)], sessions, NONE)).toEqual({ terminal: 2 });
});

test("pending_terminal_wins_over_closed_placeholder", () => {
  const v: View = { session: "a", tab: "terminal" };
  const sessions = [card({ sessionId: "a", pid: 0, parentPid: null })];
  expect(resolveView(v, [term(1, 10), term(2, 20)], sessions, new Set([2]))).toEqual({ terminal: 2 });
});

test("resolve_returns_same_object_when_unchanged", () => {
  const sessions = [card({ sessionId: "a", parentPid: 10 })];
  const views: View[] = [
    "dashboard", { terminal: 1 }, { session: "a", tab: "info", terminal: 1 },
    { session: "a", tab: "info" }, { session: "x", tab: "terminal" },
  ];
  for (const v of views) expect(resolveView(v, [term(1, 10)], sessions, NONE)).toBe(v);
});

test("tab_bar_hidden_for_plain_shell", () => {
  expect(tabBarFor({ terminal: 1 }, [term(1, 10)], [])).toBeNull();
  expect(tabBarFor("dashboard", [term(1, 10)], [])).toBeNull();
  expect(tabBarFor({ terminal: 9 }, [term(1, 10)], [])).toBeNull();
});

test("tab_bar_for_terminal_hosting_session", () => {
  const sessions = [card({ sessionId: "a", parentPid: 10 })];
  expect(tabBarFor({ terminal: 1 }, [term(1, 10)], sessions)).toEqual({ sessionId: "a", terminal: 1, active: "terminal" });
  expect(tabBarFor({ session: "a", tab: "info", terminal: 1 }, [term(1, 10)], sessions))
    .toEqual({ sessionId: "a", terminal: 1, active: "info" });
  expect(tabBarFor({ session: "z", tab: "info" }, [], [])).toEqual({ sessionId: "z", active: "info" });
});

test("move_state", () => {
  expect(moveState(card({}))).toEqual({ enabled: true, reason: null });
  expect(moveState(card({ status: "busy" }))).toEqual({ enabled: false, reason: "busy" });
  expect(moveState(card({ status: "compacting" }))).toEqual({ enabled: false, reason: "busy" });
  expect(moveState(card({ status: "waiting" }))).toEqual({ enabled: false, reason: "waiting" });
  expect(moveState(card({ agentsRunning: 2 }))).toEqual({ enabled: false, reason: "agents" });
  expect(moveState(card({ movable: false }))).toEqual({ enabled: false, reason: "notMovable" });
  // Order when several apply: notMovable, then waiting/busy, then agents.
  expect(moveState(card({ movable: false, status: "busy", agentsRunning: 1 })).reason).toBe("notMovable");
  expect(moveState(card({ status: "waiting", agentsRunning: 1 })).reason).toBe("waiting");
});
