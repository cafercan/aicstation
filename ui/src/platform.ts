/**
 * The OS the app runs on, read once from the webview's user agent
 * (WebView2: "Windows NT ..."; WebKitGTK: "X11; Linux ..."). Only labels
 * depend on it; the backend decides what actually runs.
 */
export type Os = "windows" | "linux" | "mac";

function detect(): Os {
  const ua = typeof navigator === "undefined" ? "" : navigator.userAgent;
  if (/Windows/i.test(ua)) return "windows";
  if (/Mac OS X|Macintosh/i.test(ua)) return "mac";
  return "linux";
}

let os: Os = detect();

export const currentOs = (): Os => os;
export const isWindows = (): boolean => os === "windows";

/** Tests only. */
export function setOsForTests(o: Os): void {
  os = o;
}
