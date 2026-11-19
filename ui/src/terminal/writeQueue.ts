/**
 * Terminal input to the PTY. xterm hands over a paste as one string, which
 * may be larger than one `pty_write` accepts (64 KiB of UTF-8); and writes
 * invoked without waiting can reach the backend out of order. Input is
 * therefore cut into chunks and written one at a time, in order.
 */

/** UTF-16 units per chunk: at most 3 UTF-8 bytes each, so 48 KiB < 64 KiB. */
export const MAX_CHUNK = 16 * 1024;

/** `d` in pieces of at most `max` units; a surrogate pair is never split. */
export function chunkInput(d: string, max = MAX_CHUNK): string[] {
  const out: string[] = [];
  let i = 0;
  while (i < d.length) {
    let end = Math.min(i + max, d.length);
    // Ends on a high surrogate: its low half goes with it into the next piece.
    if (end < d.length && end - i > 1) {
      const c = d.charCodeAt(end - 1);
      if (c >= 0xd800 && c <= 0xdbff) end--;
    }
    out.push(d.slice(i, end));
    i = end;
  }
  return out;
}

/**
 * A `send` that writes each piece after the previous write settled; a
 * failed write (the terminal closed) does not stop the ones after it.
 */
export function createWriter(write: (s: string) => Promise<unknown>, max = MAX_CHUNK): (d: string) => void {
  let tail: Promise<unknown> = Promise.resolve();
  return (d) => {
    for (const part of chunkInput(d, max)) {
      tail = tail.then(() => write(part)).catch(() => {});
    }
  };
}
