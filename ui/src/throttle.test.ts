import { afterEach, expect, test, vi } from "vitest";
import { throttle } from "./throttle";

afterEach(() => { vi.useRealTimers(); });

test("throttle: ilk çağrı hemen, aradakiler tek bir son çağrıda birleşir", () => {
  vi.useFakeTimers();
  const fn = vi.fn();
  const t = throttle(fn, 100);
  t(); t(); t();
  expect(fn).toHaveBeenCalledTimes(1);
  vi.advanceTimersByTime(100);
  expect(fn).toHaveBeenCalledTimes(2);
  vi.advanceTimersByTime(500);
  expect(fn).toHaveBeenCalledTimes(2);
  t();
  expect(fn).toHaveBeenCalledTimes(3);
});

test("throttle: cancel bekleyen son çağrıyı iptal eder", () => {
  vi.useFakeTimers();
  const fn = vi.fn();
  const t = throttle(fn, 100);
  t(); t();
  t.cancel();
  vi.advanceTimersByTime(200);
  expect(fn).toHaveBeenCalledTimes(1);
});
