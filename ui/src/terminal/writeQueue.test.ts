import { expect, test } from "vitest";
import { chunkInput, createWriter, MAX_CHUNK } from "./writeQueue";

test("input over the chunk size is split, never inside a surrogate pair", () => {
  expect(chunkInput("abc", 2)).toEqual(["ab", "c"]);
  expect(chunkInput("", 2)).toEqual([]);
  // "a😀b": the emoji's two halves stay together.
  expect(chunkInput("a😀b", 2)).toEqual(["a", "😀", "b"]);
  const big = "ş".repeat(40_000);
  const parts = chunkInput(big);
  expect(parts.join("")).toBe(big);
  // Each part fits the backend's 64 KiB write limit in UTF-8.
  for (const p of parts) expect(new TextEncoder().encode(p).length).toBeLessThanOrEqual(64 * 1024);
  expect(MAX_CHUNK * 3).toBeLessThanOrEqual(64 * 1024);
});

test("writes go out one after another, in order, even when one fails", async () => {
  const log: string[] = [];
  const pending: (() => void)[] = [];
  const write = (s: string) => new Promise<void>((ok, fail) => {
    log.push(`start ${s}`);
    pending.push(() => { log.push(`end ${s}`); if (s === "x") fail(new Error("x")); else ok(); });
  });
  const send = createWriter(write, 2);
  send("abc");
  send("x");
  send("d");
  const flush = async () => { await Promise.resolve(); await Promise.resolve(); await Promise.resolve(); };
  await flush();
  expect(log).toEqual(["start ab"]);
  while (pending.length) { pending.shift()!(); await flush(); }
  expect(log).toEqual(["start ab", "end ab", "start c", "end c", "start x", "end x", "start d", "end d"]);
});
