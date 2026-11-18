/** Small LRU of image object URLs; key `"<sessionId>:<offset>:<ordinal>"`. */
export function createImageCache(max = 30) {
  const m = new Map<string, string>();
  return {
    get(key: string): string | undefined {
      const v = m.get(key);
      if (v === undefined) return undefined;
      m.delete(key);
      m.set(key, v);
      return v;
    },
    set(key: string, url: string): void {
      m.delete(key);
      m.set(key, url);
      while (m.size > max) m.delete(m.keys().next().value as string);
    },
  };
}
