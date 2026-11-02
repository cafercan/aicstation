/**
 * Leading + trailing throttle: the first call runs at once, calls during the
 * next `ms` collapse into one trailing call (so the final state is applied).
 */
export function throttle(fn: () => void, ms: number): (() => void) & { cancel(): void } {
  let timer: ReturnType<typeof setTimeout> | null = null;
  let pending = false;
  const tick = () => {
    if (pending) {
      pending = false;
      fn();
      timer = setTimeout(tick, ms);
    } else {
      timer = null;
    }
  };
  const call = () => {
    if (timer !== null) { pending = true; return; }
    fn();
    timer = setTimeout(tick, ms);
  };
  call.cancel = () => {
    if (timer !== null) clearTimeout(timer);
    timer = null;
    pending = false;
  };
  return call;
}
