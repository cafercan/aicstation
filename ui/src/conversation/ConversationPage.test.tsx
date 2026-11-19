import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, expect, test, vi } from "vitest";
import type { ConvItem, ConvPage, SessionCard } from "../api";

const invoke = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({ invoke: (...a: unknown[]) => invoke(...a) }));
const terminalForPid = vi.fn((_p: number | null | undefined): number | undefined => undefined);
vi.mock("../terminal/store", () => ({
  terminalForPid: (p: number | null) => terminalForPid(p),
  openAdoptTerminal: vi.fn(),
}));

import ConversationPage from "./ConversationPage";
import Composer from "./Composer";

function card(over: Partial<SessionCard> = {}): SessionCard {
  return {
    sessionId: "s1", pid: 7, parentPid: 70, name: "demo", cwd: "C:/x/demo", status: "idle", waitingFor: null,
    model: null, contextPct: null, costUsd: 0, tokens: { input: 0, output: 0, cacheRead: 0, cacheCreation: 0, cacheCreation1h: 0 },
    agentsTotal: 0, agentsFinished: 0, toolOk: 0, toolErr: 0, lastTool: null, startedAtMs: 0, lastActivityMs: 1, transcriptBytes: 0,
    gitBranch: null, hostKind: "embedded", movable: true, agentsRunning: 0, remoteControl: false, ...over,
  };
}

const img = (offset: number, ordinal = 0) => ({ offset, ordinal, mediaType: "image/png" });
const page = (items: ConvItem[], over: Partial<ConvPage> = {}): ConvPage =>
  ({ epoch: 1, total: items.length, start: 0, items, ...over });

const ITEMS: ConvItem[] = [
  { kind: "user", text: "merhaba", truncated: false, images: [], atMs: 1 },
  { kind: "assistant", text: "**kalın** yanıt", truncated: true, atMs: 2 },
  {
    kind: "steps", atMs: 3, steps: [
      { tool: "Bash", target: "ls -la", done: true, isError: true, images: [] },
      { tool: "Read", target: "grid.jpg", done: true, isError: false, images: [img(100)] },
    ],
  },
  { kind: "compacted", atMs: 4 },
];

/** Answers each command from `handlers`; unknown commands reject. */
function serve(handlers: Record<string, (args: Record<string, unknown>) => unknown>) {
  invoke.mockImplementation(async (cmd: string, args: Record<string, unknown>) => {
    const h = handlers[cmd];
    if (!h) throw new Error(`unexpected ${cmd}`);
    return h(args);
  });
}

const noop = () => {};
const props = { onShowTerminal: noop, onTerminal: noop, onAdopted: noop, onResume: async () => {} };

beforeEach(() => {
  invoke.mockReset();
  terminalForPid.mockReset();
  terminalForPid.mockImplementation((p) => (p === 70 ? 3 : undefined));
});

test("renders user, assistant, steps and compacted items from a page", async () => {
  let n = 0;
  serve({
    get_conversation_page: () => page(ITEMS),
    get_conversation_image: () => ({ mediaType: "image/png", data: `R${n++}` }),
  });
  render(<ConversationPage sessionId="s1" card={card()} {...props} />);
  expect(await screen.findByText("merhaba")).toBeTruthy();
  expect(screen.getByText("kalın").tagName).toBe("STRONG");
  expect(screen.getByText("(kısaltıldı)")).toBeTruthy();
  expect(screen.getByText("Sohbet özetlendi")).toBeTruthy();
  const line = screen.getByRole("button", { name: /1 komut çalıştırdı, grid\.jpg okundu/ });
  expect(line.getAttribute("aria-expanded")).toBe("false");
  expect(screen.queryByText("ls -la")).toBeNull();
  fireEvent.click(line);
  expect(line.getAttribute("aria-expanded")).toBe("true");
  expect(screen.getByText("ls -la").closest("li")?.className).toContain("err");
  expect(screen.getByText("Bash")).toBeTruthy();
  expect(invoke).toHaveBeenCalledWith("get_conversation_page", { sessionId: "s1", before: null });
});

test("empty conversation shows the empty note", async () => {
  serve({ get_conversation_page: () => page([]) });
  render(<ConversationPage sessionId="s1" card={card()} {...props} />);
  expect(await screen.findByText("Henüz mesaj yok.")).toBeTruthy();
});

test("thumb opens viewer and Esc closes it", async () => {
  serve({
    get_conversation_page: () => page([{ kind: "user", text: "bak", truncated: false, images: [img(200, 1)], atMs: 1 }]),
    get_conversation_image: () => ({ mediaType: "image/png", data: "QUJD" }),
  });
  render(<ConversationPage sessionId="s-view" card={card()} {...props} />);
  const thumb = await screen.findByRole("button", { name: "Resmi aç" });
  await waitFor(() => expect(thumb.querySelector("img")?.getAttribute("src")).toBe("data:image/png;base64,QUJD"));
  expect(invoke).toHaveBeenCalledWith("get_conversation_image", { sessionId: "s-view", offset: 200, ordinal: 1 });
  fireEvent.click(thumb);
  const dialog = screen.getByRole("dialog");
  expect(dialog.querySelector("img")?.getAttribute("src")).toBe("data:image/png;base64,QUJD");
  fireEvent.keyDown(document.body, { key: "Escape" });
  expect(screen.queryByRole("dialog")).toBeNull();
  // Close button and a click on the backdrop close it too.
  fireEvent.click(thumb);
  fireEvent.click(screen.getByRole("button", { name: "Kapat" }));
  expect(screen.queryByRole("dialog")).toBeNull();
  fireEvent.click(thumb);
  fireEvent.click(screen.getByRole("dialog").querySelector("img")!);
  expect(screen.getByRole("dialog")).toBeTruthy();
  fireEvent.click(screen.getByRole("dialog"));
  expect(screen.queryByRole("dialog")).toBeNull();
});

test("viewer is a modal dialog that keeps focus and gives it back", async () => {
  serve({
    get_conversation_page: () => page([{ kind: "user", text: "bak", truncated: false, images: [img(210)], atMs: 1 }]),
    get_conversation_image: () => ({ mediaType: "image/png", data: "QUJE" }),
  });
  render(<ConversationPage sessionId="s-focus" card={card()} {...props} />);
  const thumb = await screen.findByRole("button", { name: "Resmi aç" });
  await waitFor(() => expect(thumb.querySelector("img")).toBeTruthy());
  thumb.focus();
  fireEvent.click(thumb);
  const dialog = screen.getByRole("dialog");
  expect(dialog.getAttribute("aria-modal")).toBe("true");
  const close = screen.getByRole("button", { name: "Kapat" });
  expect(dialog.contains(close)).toBe(true);
  expect(document.activeElement).toBe(close);
  fireEvent.keyDown(document.activeElement!, { key: "Tab" });
  expect(document.activeElement).toBe(close);
  fireEvent.keyDown(document.activeElement!, { key: "Tab", shiftKey: true });
  expect(document.activeElement).toBe(close);
  // Ctrl/Alt/Meta+Tab are not the viewer's: they are not cancelled.
  for (const mod of ["ctrlKey", "altKey", "metaKey"]) {
    expect(fireEvent.keyDown(document.activeElement!, { key: "Tab", [mod]: true })).toBe(true);
  }
  expect(fireEvent.keyDown(document.activeElement!, { key: "Tab" })).toBe(false);
  fireEvent.keyDown(document.body, { key: "Escape" });
  expect(screen.queryByRole("dialog")).toBeNull();
  expect(document.activeElement).toBe(thumb);
});

/** IntersectionObserver stand-in: `show(on)` reports every observed thumb in or out of view. */
class FakeIO {
  static all: FakeIO[] = [];
  el: Element | null = null;
  constructor(public cb: IntersectionObserverCallback, public opts: IntersectionObserverInit) { FakeIO.all.push(this); }
  observe(el: Element) { this.el = el; }
  unobserve() {}
  disconnect() { this.el = null; }
  takeRecords() { return []; }
}
const show = (on: boolean) => act(async () => {
  for (const io of FakeIO.all) {
    if (io.el) io.cb([{ isIntersecting: on, target: io.el } as unknown as IntersectionObserverEntry], io as unknown as IntersectionObserver);
  }
});

test("a thumb drops its image out of view and gets it back from the cache", async () => {
  FakeIO.all = [];
  vi.stubGlobal("IntersectionObserver", FakeIO);
  try {
    serve({
      get_conversation_page: () => page([{ kind: "user", text: "bak", truncated: false, images: [img(400)], atMs: 1 }]),
      get_conversation_image: () => ({ mediaType: "image/png", data: "VklT" }),
    });
    const { container } = render(<ConversationPage sessionId="s-io" card={card()} {...props} />);
    const thumb = await screen.findByRole("button", { name: "Resmi aç" });
    // The thumb can be found before its effect has created the observer.
    await waitFor(() => expect(FakeIO.all).toHaveLength(1));
    expect(FakeIO.all[0].opts.root).toBe(container.querySelector(".conv-list"));
    expect(FakeIO.all[0].opts.rootMargin).toBe("300px");
    expect(thumb.querySelector("img")).toBeNull();
    await show(true);
    await waitFor(() => expect(thumb.querySelector("img")?.getAttribute("src")).toBe("data:image/png;base64,VklT"));
    await show(false);
    expect(thumb.querySelector("img")).toBeNull();
    await show(true);
    expect(thumb.querySelector("img")?.getAttribute("src")).toBe("data:image/png;base64,VklT");
    const loads = invoke.mock.calls.filter(([c]) => c === "get_conversation_image");
    expect(loads).toHaveLength(1);
  } finally {
    vi.unstubAllGlobals();
  }
});

test("a step list opened before a branch change does not stay open on another item", async () => {
  serve({
    get_conversation_page: () => page([ITEMS[0], ITEMS[2]], { epoch: 1 }),
    get_conversation_since: () => page([ITEMS[0], { kind: "steps", atMs: 7, steps: [
      { tool: "Read", target: "foo.txt", done: true, isError: false, images: [] },
    ] }], { epoch: 2 }),
  });
  const { rerender } = render(<ConversationPage sessionId="s1" card={card({ lastActivityMs: 1 })} {...props} />);
  const line = await screen.findByRole("button", { name: /1 komut çalıştırdı/ });
  fireEvent.click(line);
  expect(line.getAttribute("aria-expanded")).toBe("true");
  rerender(<ConversationPage sessionId="s1" card={card({ lastActivityMs: 2 })} {...props} />);
  const other = await screen.findByRole("button", { name: /foo\.txt/ });
  expect(other.getAttribute("aria-expanded")).toBe("false");
});

test("background task notices show as a muted line, with a fallback text", async () => {
  serve({
    get_conversation_page: () => page([
      { kind: "assistant", text: "çalışıyorum", truncated: false, atMs: 1 },
      { kind: "notice", text: "Background command \"ls\" completed", atMs: 2 },
      { kind: "notice", text: "", atMs: 3 },
    ]),
  });
  render(<ConversationPage sessionId="s1" card={card()} {...props} />);
  const note = await screen.findByText("Background command \"ls\" completed");
  expect(note.className).toBe("conv-notice");
  expect(note.closest(".conv-bubble")).toBeNull();
  expect(screen.getByText("Arka plan görevi bitti").className).toBe("conv-notice");
});

test("a failed first load recovers on the next activity", async () => {
  let calls = 0;
  serve({
    get_conversation_page: () => {
      if (calls++ === 0) throw "conv_unavailable";
      return page(ITEMS.slice(0, 1));
    },
  });
  const { rerender } = render(<ConversationPage sessionId="s1" card={card({ lastActivityMs: 1 })} {...props} />);
  // A live session without a transcript yet is simply empty, not an error.
  expect(await screen.findByText("Henüz mesaj yok.")).toBeTruthy();
  expect(screen.queryByRole("alert")).toBeNull();
  rerender(<ConversationPage sessionId="s1" card={card({ lastActivityMs: 2 })} {...props} />);
  expect(await screen.findByText("merhaba")).toBeTruthy();
  expect(screen.queryByRole("alert")).toBeNull();
  expect(invoke).not.toHaveBeenCalledWith("get_conversation_since", expect.anything());
});

test("a closed session without a transcript still says it was not found", async () => {
  serve({ get_conversation_page: () => { throw "conv_unavailable"; } });
  render(<ConversationPage sessionId="s1" card={undefined} {...props} />);
  expect((await screen.findByRole("alert")).textContent).toBe("Bu oturumun konuşma kaydı bulunamadı.");
});

test("other first-load errors on a live session stay visible", async () => {
  serve({ get_conversation_page: () => { throw "session_invalid"; } });
  render(<ConversationPage sessionId="s1" card={card()} {...props} />);
  expect(await screen.findByRole("alert")).toBeTruthy();
});

test("thumb shows placeholder on error", async () => {
  serve({
    get_conversation_page: () => page([{ kind: "user", text: "bak", truncated: false, images: [img(300)], atMs: 1 }]),
    get_conversation_image: () => { throw "image_unavailable"; },
  });
  render(<ConversationPage sessionId="s-err" card={card()} {...props} />);
  expect(await screen.findByRole("img", { name: "Resim açılamadı" })).toBeTruthy();
});

test("since appends on lastActivity change", async () => {
  serve({
    get_conversation_page: () => page(ITEMS.slice(0, 2), { epoch: 5 }),
    get_conversation_since: () => page([ITEMS[1], { kind: "assistant", text: "yeni yanıt", truncated: false, atMs: 9 }],
      { epoch: 5, start: 1, total: 3 }),
  });
  const { rerender } = render(<ConversationPage sessionId="s1" card={card({ lastActivityMs: 1 })} {...props} />);
  await screen.findByText("merhaba");
  expect(invoke).not.toHaveBeenCalledWith("get_conversation_since", expect.anything());
  rerender(<ConversationPage sessionId="s1" card={card({ lastActivityMs: 2 })} {...props} />);
  expect(await screen.findByText("yeni yanıt")).toBeTruthy();
  expect(invoke).toHaveBeenCalledWith("get_conversation_since", { sessionId: "s1", epoch: 5, from: 1 });
  expect(screen.getAllByText("kalın")).toHaveLength(1);
});

test("since appends when the transcript grows with an unchanged lastActivity", async () => {
  // Claude Code marks the turn idle before it writes the reply, whose
  // timestamp is older: only the transcript size tells the reply arrived.
  serve({
    get_conversation_page: () => page(ITEMS.slice(0, 1), { epoch: 5 }),
    get_conversation_since: () => page([ITEMS[0], { kind: "assistant", text: "geç yanıt", truncated: false, atMs: 9 }],
      { epoch: 5, start: 0, total: 2 }),
  });
  const { rerender } = render(
    <ConversationPage sessionId="s1" card={card({ lastActivityMs: 5, transcriptBytes: 100 })} {...props} />);
  await screen.findByText("merhaba");
  rerender(<ConversationPage sessionId="s1" card={card({ lastActivityMs: 5, transcriptBytes: 180 })} {...props} />);
  expect(await screen.findByText("geç yanıt")).toBeTruthy();
});

test("closed session does not refresh and shows Resume", async () => {
  serve({ get_conversation_page: () => page(ITEMS.slice(0, 1)) });
  const { rerender } = render(<ConversationPage sessionId="s1" card={undefined} {...props} />);
  await screen.findByText("merhaba");
  rerender(<ConversationPage sessionId="s1" card={card({ pid: 0, lastActivityMs: 9 })} {...props} />);
  expect(screen.getByRole("button", { name: "▶ Devam et" })).toBeTruthy();
  expect(screen.queryByRole("textbox")).toBeNull();
  await act(async () => {});
  expect(invoke).not.toHaveBeenCalledWith("get_conversation_since", expect.anything());
});

test("scrolled to the top loads the older page", async () => {
  serve({
    get_conversation_page: (a) => (a.before == null
      ? page(ITEMS.slice(1, 2), { start: 1, total: 2 })
      : page(ITEMS.slice(0, 1), { start: 0, total: 2 })),
  });
  render(<ConversationPage sessionId="s1" card={card()} {...props} />);
  expect(await screen.findByText("merhaba")).toBeTruthy();
  expect(invoke).toHaveBeenCalledWith("get_conversation_page", { sessionId: "s1", before: 1 });
});

test("an older page that arrives after a since reset is dropped", async () => {
  let resolveOlder: (p: ConvPage) => void = () => {};
  serve({
    get_conversation_page: (a) => (a.before == null
      ? page(ITEMS.slice(1, 2), { epoch: 1, start: 1, total: 2 })
      : new Promise<ConvPage>((r) => { resolveOlder = r; })),
    get_conversation_since: () => page([{ kind: "assistant", text: "yeni dal", truncated: false, atMs: 9 }],
      { epoch: 2, start: 0, total: 1 }),
  });
  const { rerender } = render(<ConversationPage sessionId="s1" card={card({ lastActivityMs: 1 })} {...props} />);
  expect(await screen.findByText("kalın")).toBeTruthy();
  await waitFor(() => expect(invoke).toHaveBeenCalledWith("get_conversation_page", { sessionId: "s1", before: 1 }));
  rerender(<ConversationPage sessionId="s1" card={card({ lastActivityMs: 2 })} {...props} />);
  expect(await screen.findByText("yeni dal")).toBeTruthy();
  await act(async () => { resolveOlder(page(ITEMS.slice(0, 1), { epoch: 1, start: 0, total: 2 })); });
  expect(screen.queryByText("merhaba")).toBeNull();
  expect(screen.queryByText("kalın")).toBeNull();
  expect(screen.getByText("yeni dal")).toBeTruthy();
});

test("outside-app card shows the Move to app button instead of the composer", async () => {
  terminalForPid.mockImplementation(() => undefined);
  serve({ get_conversation_page: () => page([]) });
  render(<ConversationPage sessionId="s1" card={card({ hostKind: "other", parentPid: 99 })} {...props} />);
  await screen.findByText("Henüz mesaj yok.");
  expect(screen.getByRole("button", { name: "Uygulamaya taşı" })).toBeTruthy();
  expect(screen.queryByRole("textbox")).toBeNull();
});

test("hosted live card shows the composer", async () => {
  serve({ get_conversation_page: () => page([]) });
  render(<ConversationPage sessionId="s1" card={card()} {...props} />);
  await screen.findByText("Henüz mesaj yok.");
  expect(screen.getByRole("textbox")).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Uygulamaya taşı" })).toBeNull();
});

test("sent message is picked up by a since call shortly after", async () => {
  serve({
    get_conversation_page: () => page(ITEMS.slice(0, 1)),
    send_to_session: () => undefined,
    get_conversation_since: () => page([ITEMS[0], { kind: "user", text: "gönderilen", truncated: false, images: [], atMs: 5 }],
      { total: 2 }),
  });
  render(<ConversationPage sessionId="s1" card={card()} {...props} />);
  await screen.findByText("merhaba");
  const box = screen.getByRole("textbox");
  fireEvent.change(box, { target: { value: "gönderilen" } });
  fireEvent.keyDown(box, { key: "Enter" });
  expect(await screen.findByText("gönderilen", {}, { timeout: 2000 })).toBeTruthy();
});

// Composer

test("composer: Enter sends the text and clears it", async () => {
  serve({ send_to_session: () => undefined });
  const onSent = vi.fn();
  render(<Composer sessionId="s1" status="idle" onShowTerminal={noop} onSent={onSent} />);
  const box = screen.getByRole("textbox") as HTMLTextAreaElement;
  expect(box.placeholder).toBe("Claude'a yazın… (Enter gönderir, Shift+Enter yeni satır)");
  fireEvent.change(box, { target: { value: "selam\nikinci" } });
  fireEvent.keyDown(box, { key: "Enter" });
  await waitFor(() => expect(box.value).toBe(""));
  expect(invoke).toHaveBeenCalledWith("send_to_session", { sessionId: "s1", text: "selam\nikinci" });
  expect(onSent).toHaveBeenCalled();
});

test("composer: Shift+Enter does not send", () => {
  serve({ send_to_session: () => undefined });
  render(<Composer sessionId="s1" status="idle" onShowTerminal={noop} />);
  const box = screen.getByRole("textbox");
  fireEvent.change(box, { target: { value: "selam" } });
  fireEvent.keyDown(box, { key: "Enter", shiftKey: true });
  expect(invoke).not.toHaveBeenCalled();
  // Blank text is not sent either.
  fireEvent.change(box, { target: { value: "  " } });
  fireEvent.keyDown(box, { key: "Enter" });
  expect(invoke).not.toHaveBeenCalled();
});

test("composer: Enter that commits an IME word does not send", () => {
  serve({ send_to_session: () => undefined });
  render(<Composer sessionId="s1" status="idle" onShowTerminal={noop} />);
  const box = screen.getByRole("textbox");
  fireEvent.change(box, { target: { value: "日本" } });
  fireEvent.keyDown(box, { key: "Enter", keyCode: 229 });
  fireEvent.keyDown(box, { key: "Enter", isComposing: true });
  expect(invoke).not.toHaveBeenCalled();
});

test("composer: waiting disables the box and the button shows the terminal", () => {
  const onShowTerminal = vi.fn();
  render(<Composer sessionId="s1" status="waiting" onShowTerminal={onShowTerminal} />);
  expect((screen.getByRole("textbox") as HTMLTextAreaElement).disabled).toBe(true);
  expect((screen.getByRole("button", { name: "Gönder" }) as HTMLButtonElement).disabled).toBe(true);
  expect(screen.getByText("Onayı Terminal sekmesinde verin")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Terminal sekmesi" }));
  expect(onShowTerminal).toHaveBeenCalled();
});

test("composer: send error keeps text", async () => {
  serve({ send_to_session: () => { throw "send_waiting"; } });
  render(<Composer sessionId="s1" status="busy" onShowTerminal={noop} />);
  const box = screen.getByRole("textbox") as HTMLTextAreaElement;
  fireEvent.change(box, { target: { value: "selam" } });
  fireEvent.click(screen.getByRole("button", { name: "Gönder" }));
  expect((await screen.findByRole("alert")).textContent)
    .toBe("Gönderilemedi: Oturum terminalinde bir yanıt bekliyor; önce orada yanıtlanmalı.");
  expect(box.value).toBe("selam");
});

test("composer: text edited while the send is on its way stays as it is", async () => {
  let finish: () => void = () => {};
  serve({ send_to_session: () => new Promise<void>((r) => { finish = r; }) });
  render(<Composer sessionId="s1" status="idle" onShowTerminal={noop} />);
  const box = screen.getByRole("textbox") as HTMLTextAreaElement;
  fireEvent.change(box, { target: { value: "selam" } });
  fireEvent.keyDown(box, { key: "Enter" });
  fireEvent.change(box, { target: { value: "selam, bir de şu" } });
  await act(async () => { finish(); });
  expect(box.value).toBe("selam, bir de şu");
  // Edited to something else entirely: also kept.
  fireEvent.keyDown(box, { key: "Enter" });
  fireEvent.change(box, { target: { value: "başka" } });
  await act(async () => { finish(); });
  expect(box.value).toBe("başka");
});

test("composer: typing clears the send error", async () => {
  serve({ send_to_session: () => { throw "send_waiting"; } });
  render(<Composer sessionId="s1" status="busy" onShowTerminal={noop} />);
  const box = screen.getByRole("textbox") as HTMLTextAreaElement;
  fireEvent.change(box, { target: { value: "selam" } });
  fireEvent.click(screen.getByRole("button", { name: "Gönder" }));
  expect(await screen.findByRole("alert")).toBeTruthy();
  fireEvent.change(box, { target: { value: "selam!" } });
  expect(screen.queryByRole("alert")).toBeNull();
});

test("composer: text pasted but not submitted says so", async () => {
  serve({ send_to_session: () => { throw "send_not_submitted:send_waiting"; } });
  render(<Composer sessionId="s1" status="busy" onShowTerminal={noop} />);
  const box = screen.getByRole("textbox") as HTMLTextAreaElement;
  fireEvent.change(box, { target: { value: "selam" } });
  fireEvent.click(screen.getByRole("button", { name: "Gönder" }));
  expect((await screen.findByRole("alert")).textContent).toBe(
    "Metin oturumun terminaline yapıştırıldı ama gönderilmedi. Orada silin ya da Terminal sekmesinden gönderin. "
      + "Neden: Oturum terminalinde bir yanıt bekliyor; önce orada yanıtlanmalı.",
  );
});
