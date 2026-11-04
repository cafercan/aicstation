import { beforeEach, expect, test, vi } from "vitest";
import { copyText, pasteFromClipboard, rightClickAction, type ClipboardLike } from "./clipboard";
import { attachInput, type InputTerm } from "./input";
import { enableWebgl } from "./render";

const flush = () => new Promise((r) => setTimeout(r, 0));

function fakeClip(over: Partial<ClipboardLike> = {}): ClipboardLike & { written: string[] } {
  const written: string[] = [];
  return {
    written,
    writeText: vi.fn(async (t: string) => { written.push(t); }),
    readText: vi.fn(async () => "pano"),
    ...over,
  };
}

function fakeTerm(selection = "") {
  let handler: (ev: KeyboardEvent) => boolean = () => true;
  const t = {
    sel: selection,
    pasted: [] as string[],
    hasSelection: () => t.sel.length > 0,
    getSelection: () => t.sel,
    clearSelection: vi.fn(() => { t.sel = ""; }),
    paste: (d: string) => { t.pasted.push(d); },
    attachCustomKeyEventHandler: (h: (ev: KeyboardEvent) => boolean) => { handler = h; },
    press: (init: KeyboardEventInit & { type?: string }) => {
      const ev = new KeyboardEvent(init.type ?? "keydown", { cancelable: true, ctrlKey: true, ...init });
      return { result: handler(ev), prevented: ev.defaultPrevented };
    },
  };
  return t;
}

beforeEach(() => { vi.restoreAllMocks(); });

// --- clipboard ---------------------------------------------------------------

test("copyText: navigator.clipboard.writeText ile yazar", async () => {
  const clip = fakeClip();
  expect(await copyText("merhaba", clip)).toBe(true);
  expect(clip.written).toEqual(["merhaba"]);
});

test("copyText: writeText reddedilirse copy olayı üzerinden yazar", async () => {
  const clip = fakeClip({ writeText: vi.fn(async () => { throw new Error("denied"); }) });
  let set: [string, string] | null = null;
  const exec = vi.fn((cmd: string) => {
    const ev = new Event("copy", { cancelable: true }) as ClipboardEvent;
    Object.defineProperty(ev, "clipboardData", { value: { setData: (f: string, d: string) => { set = [f, d]; } } });
    document.dispatchEvent(ev);
    return cmd === "copy";
  });
  Object.defineProperty(document, "execCommand", { value: exec, configurable: true });
  expect(await copyText("yedek", clip)).toBe(true);
  expect(exec).toHaveBeenCalledWith("copy");
  expect(set).toEqual(["text/plain", "yedek"]);
});

test("copyText: boş metin hiçbir şey yazmaz", async () => {
  const clip = fakeClip();
  expect(await copyText("", clip)).toBe(false);
  expect(clip.writeText).not.toHaveBeenCalled();
});

test("pasteFromClipboard: metni term.paste ile verir (bracketed paste xterm'de kalır)", async () => {
  const term = fakeTerm();
  expect(await pasteFromClipboard(term, fakeClip())).toBe(true);
  expect(term.pasted).toEqual(["pano"]);
});

test("pasteFromClipboard: boş pano ya da okuma reddi hiçbir şey yapıştırmaz, hata fırlatmaz", async () => {
  const term = fakeTerm();
  expect(await pasteFromClipboard(term, fakeClip({ readText: vi.fn(async () => "") }))).toBe(false);
  expect(await pasteFromClipboard(term, fakeClip({ readText: vi.fn(async () => { throw new Error("no"); }) }))).toBe(false);
  expect(await pasteFromClipboard(term, undefined)).toBe(false);
  expect(term.pasted).toEqual([]);
});

test("sağ tık: seçim varsa kopyalar, yoksa yapıştırır (Windows Terminal)", () => {
  expect(rightClickAction(true)).toBe("copy");
  expect(rightClickAction(false)).toBe("paste");
});

// --- key wiring --------------------------------------------------------------

function wire(term: ReturnType<typeof fakeTerm>, clip = fakeClip()) {
  const host = document.createElement("div");
  const hooks = { onSearch: vi.fn(), onFont: vi.fn() };
  const detach = attachInput(term as unknown as InputTerm, host, hooks, clip);
  return { host, hooks, clip, detach };
}

test("Ctrl+C seçimle: kopyalar, seçimi temizler, xterm'e ^C gitmez", async () => {
  const term = fakeTerm("ls -la");
  const { clip } = wire(term);
  const r = term.press({ key: "c" });
  expect(r).toEqual({ result: false, prevented: true });
  await flush();
  expect(clip.written).toEqual(["ls -la"]);
  expect(term.clearSelection).toHaveBeenCalled();
});

test("Ctrl+C seçimsiz: xterm işler (^C kabuğa)", () => {
  const term = fakeTerm();
  const { clip } = wire(term);
  expect(term.press({ key: "c" })).toEqual({ result: true, prevented: false });
  expect(clip.writeText).not.toHaveBeenCalled();
});

test("Ctrl+V: xterm'e ^V gitmez ve varsayılan engellenmez (yerel paste olayı)", () => {
  const term = fakeTerm();
  const { clip } = wire(term);
  expect(term.press({ key: "v" })).toEqual({ result: false, prevented: false });
  expect(term.press({ key: "V", shiftKey: true })).toEqual({ result: false, prevented: false });
  expect(clip.readText).not.toHaveBeenCalled();
  expect(term.pasted).toEqual([]);
});

test("Ctrl+Shift+C seçimsiz: hiçbir şey gönderilmez", () => {
  const term = fakeTerm();
  const { clip } = wire(term);
  expect(term.press({ key: "C", shiftKey: true })).toEqual({ result: false, prevented: true });
  expect(clip.writeText).not.toHaveBeenCalled();
});

test("Ctrl+F ve yazı boyutu kısayolları kancalara gider", () => {
  const term = fakeTerm();
  const { hooks } = wire(term);
  expect(term.press({ key: "f" }).result).toBe(false);
  expect(hooks.onSearch).toHaveBeenCalledTimes(1);
  term.press({ key: "=" });
  term.press({ key: "-" });
  term.press({ key: "0" });
  expect(hooks.onFont.mock.calls.map((c) => c[0])).toEqual(["fontUp", "fontDown", "fontReset"]);
  // keypress of a handled combo does not fire the hook again
  term.press({ key: "f", type: "keypress" });
  expect(hooks.onSearch).toHaveBeenCalledTimes(1);
});

test("Ctrl+tekerlek yazı boyutunu değiştirir ve sayfa yakınlaştırmasını engeller", () => {
  const term = fakeTerm();
  const { host, hooks, detach } = wire(term);
  const up = new WheelEvent("wheel", { deltaY: -120, ctrlKey: true, cancelable: true });
  host.dispatchEvent(up);
  expect(up.defaultPrevented).toBe(true);
  const plain = new WheelEvent("wheel", { deltaY: -120, cancelable: true });
  host.dispatchEvent(plain);
  expect(plain.defaultPrevented).toBe(false);
  expect(hooks.onFont.mock.calls.map((c) => c[0])).toEqual(["fontUp"]);
  detach();
  host.dispatchEvent(new WheelEvent("wheel", { deltaY: 120, ctrlKey: true, cancelable: true }));
  expect(hooks.onFont).toHaveBeenCalledTimes(1);
});

test("sağ tık: seçimsiz panodan yapıştırır, seçimle kopyalar; menü açılmaz", async () => {
  const term = fakeTerm();
  const { host, clip } = wire(term);
  const ev = new MouseEvent("contextmenu", { cancelable: true, bubbles: true });
  host.dispatchEvent(ev);
  expect(ev.defaultPrevented).toBe(true);
  await flush();
  expect(term.pasted).toEqual(["pano"]);
  term.sel = "seçili";
  host.dispatchEvent(new MouseEvent("contextmenu", { cancelable: true, bubbles: true }));
  await flush();
  expect(clip.written).toEqual(["seçili"]);
  expect(term.pasted).toEqual(["pano"]);
});

test("sağ tuş basma/bırakma xterm'e ulaşmaz (fare izleyen TUI ikinci kez yapıştırmaz); sol tuş ulaşır", () => {
  const term = fakeTerm();
  const { host, detach } = wire(term);
  const inner = document.createElement("div");
  host.appendChild(inner);
  const seen: string[] = [];
  for (const type of ["mousedown", "mouseup"]) inner.addEventListener(type, (e) => seen.push(`${type}:${(e as MouseEvent).button}`));
  for (const button of [2, 0]) {
    inner.dispatchEvent(new MouseEvent("mousedown", { button, bubbles: true }));
    inner.dispatchEvent(new MouseEvent("mouseup", { button, bubbles: true }));
  }
  expect(seen).toEqual(["mousedown:0", "mouseup:0"]);
  detach();
  inner.dispatchEvent(new MouseEvent("mousedown", { button: 2, bubbles: true }));
  expect(seen).toContain("mousedown:2");
});

// --- renderer ----------------------------------------------------------------

test("WebGL yüklenir; bağlam kaybında atılır (DOM render'a döner)", () => {
  let lose: () => void = () => {};
  const addon = { onContextLoss: (cb: () => void) => { lose = cb; return { dispose() {} }; }, dispose: vi.fn() };
  const term = { loadAddon: vi.fn() };
  expect(enableWebgl(term, () => addon)).toBe(true);
  expect(term.loadAddon).toHaveBeenCalledWith(addon);
  lose();
  expect(addon.dispose).toHaveBeenCalled();
});

test("WebGL yüklenemezse çökmeden varsayılan render'da kalır", () => {
  const term = { loadAddon: vi.fn(() => { throw new Error("no webgl2"); }) };
  const addon = { onContextLoss: () => ({ dispose() {} }), dispose: vi.fn() };
  expect(enableWebgl(term, () => addon)).toBe(false);
  expect(addon.dispose).toHaveBeenCalled();
  expect(enableWebgl({ loadAddon: vi.fn() }, () => { throw new Error("ctor"); })).toBe(false);
});
