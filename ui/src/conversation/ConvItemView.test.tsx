import { render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import type { ConvItem, ConvPage, SessionCard } from "../api";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
vi.mock("../terminal/store", () => ({ terminalForPid: () => undefined, openAdoptTerminal: vi.fn() }));
/** Texts Markdown was rendered with, one entry per render. */
const rendered: string[] = [];
vi.mock("./Markdown", () => ({
  default: ({ text }: { text: string }) => { rendered.push(text); return <p>{text}</p>; },
}));

import ConversationPage from "./ConversationPage";

function card(lastActivityMs: number): SessionCard {
  return {
    sessionId: "s1", pid: 7, parentPid: 70, name: "demo", cwd: "C:/x/demo", status: "idle", waitingFor: null,
    model: null, contextPct: null, costUsd: 0, tokens: { input: 0, output: 0, cacheRead: 0, cacheCreation: 0, cacheCreation1h: 0 },
    agentsTotal: 0, agentsFinished: 0, toolOk: 0, toolErr: 0, lastTool: null, startedAtMs: 0, lastActivityMs,
    gitBranch: null, hostKind: "embedded", movable: true, agentsRunning: 0, remoteControl: false,
  };
}

const a = (text: string, atMs: number): ConvItem => ({ kind: "assistant", text, truncated: false, atMs });
const noop = () => {};
const props = { onShowTerminal: noop, onTerminal: noop, onAdopted: noop, onResume: async () => {} };

test("a since merge does not render the unchanged items again", async () => {
  const first: ConvPage = { epoch: 3, total: 3, start: 0, items: [a("eski", 1), { kind: "user", text: "u", truncated: false, images: [], atMs: 2 }, a("son", 3)] };
  invoke.mockImplementation(async (cmd: string) => {
    if (cmd === "get_conversation_page") return first;
    if (cmd === "get_conversation_since") return { epoch: 3, total: 4, start: 2, items: [a("son", 3), a("yeni", 4)] };
    throw new Error(cmd);
  });
  const { rerender } = render(<ConversationPage sessionId="s1" card={card(1)} {...props} />);
  await screen.findByText("eski");
  const before = rendered.filter((t) => t === "eski").length;
  rerender(<ConversationPage sessionId="s1" card={card(2)} {...props} />);
  await screen.findByText("yeni");
  expect(rendered.filter((t) => t === "eski").length).toBe(before);
});
