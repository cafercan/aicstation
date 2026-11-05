import { expect, test, vi } from "vitest";
import { ClipboardAddon } from "@xterm/addon-clipboard";
import type { Terminal } from "@xterm/xterm";
import { osc52Clipboard, type ClipboardLike } from "./clipboard";

const flush = () => new Promise((r) => setTimeout(r, 0));

/** The real addon with our provider, on a terminal that only parses OSC 52. */
function setup() {
  const clip: ClipboardLike = {
    writeText: vi.fn(async () => {}),
    readText: vi.fn(async () => "secret password"),
  };
  let osc: (data: string) => boolean | Promise<boolean> = () => false;
  const replies: string[] = [];
  const term = {
    parser: { registerOscHandler: (id: number, h: typeof osc) => { expect(id).toBe(52); osc = h; return { dispose() {} }; } },
    input: (d: string) => { replies.push(d); },
  };
  new ClipboardAddon(undefined, osc52Clipboard(clip)).activate(term as unknown as Terminal);
  return { clip, replies, osc52: async (data: string) => { await osc(data); await flush(); } };
}

const b64 = (s: string) => btoa(String.fromCharCode(...new TextEncoder().encode(s)));

test("OSC 52 yazma: metin panoya gider (UTF-8 dahil)", async () => {
  const { clip, osc52 } = setup();
  await osc52(`c;${b64("seçili metin ✓")}`);
  await osc52(`;${b64("seçim yok")}`);
  expect(clip.writeText).toHaveBeenNthCalledWith(1, "seçili metin ✓");
  expect(clip.writeText).toHaveBeenNthCalledWith(2, "seçim yok");
});

test("OSC 52 okuma: pano okunmaz, cevap boş", async () => {
  const { clip, replies, osc52 } = setup();
  await osc52("c;?");
  await osc52("p;?");
  expect(clip.readText).not.toHaveBeenCalled();
  expect(replies).toEqual(["\x1b]52;c;\x07", "\x1b]52;p;\x07"]);
  expect(replies.join("")).not.toContain(b64("secret password"));
});

test("OSC 52: birincil seçim, boş ve geçersiz veri panoyu değiştirmez", async () => {
  const { clip, osc52 } = setup();
  await osc52(`p;${b64("birincil")}`);
  await osc52("c;");
  await osc52("c;%%geçersiz%%");
  expect(clip.writeText).not.toHaveBeenCalled();
});
