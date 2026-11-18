import { expect, test } from "vitest";
import { createImageCache } from "./imageCache";

test("get returns what was set", () => {
  const c = createImageCache();
  expect(c.get("a")).toBeUndefined();
  c.set("a", "u1");
  expect(c.get("a")).toBe("u1");
});

test("evicts the least recently used at 31", () => {
  const c = createImageCache();
  for (let i = 0; i < 30; i++) c.set(`k${i}`, `u${i}`);
  c.get("k0"); // k1 is now the oldest
  c.set("k30", "u30");
  expect(c.get("k1")).toBeUndefined();
  expect(c.get("k0")).toBe("u0");
  expect(c.get("k30")).toBe("u30");
});

test("set on an existing key refreshes it", () => {
  const c = createImageCache(2);
  c.set("a", "1"); c.set("b", "2"); c.set("a", "3"); c.set("c", "4");
  expect(c.get("b")).toBeUndefined();
  expect(c.get("a")).toBe("3");
});
