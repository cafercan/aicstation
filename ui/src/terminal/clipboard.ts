/**
 * Clipboard access for the embedded terminal. Uses navigator.clipboard (no
 * Tauri plugin). Ctrl+V / Ctrl+Shift+V do not come here: they use the
 * browser's native paste event, which xterm turns into a bracketed paste.
 */

export interface ClipboardLike {
  writeText(text: string): Promise<void>;
  readText(): Promise<string>;
}

export function systemClipboard(): ClipboardLike | undefined {
  return typeof navigator !== "undefined" ? navigator.clipboard : undefined;
}

/** Fallback when writeText is refused: answer a synchronous copy event. */
function copyViaEvent(text: string): boolean {
  const onCopy = (e: ClipboardEvent) => {
    e.clipboardData?.setData("text/plain", text);
    e.preventDefault();
  };
  document.addEventListener("copy", onCopy, true);
  try {
    return document.execCommand("copy");
  } catch {
    return false;
  } finally {
    document.removeEventListener("copy", onCopy, true);
  }
}

export async function copyText(text: string, clip: ClipboardLike | undefined = systemClipboard()): Promise<boolean> {
  if (!text) return false;
  try {
    if (!clip) throw new Error("no clipboard");
    await clip.writeText(text);
    return true;
  } catch {
    return copyViaEvent(text);
  }
}

/** `term.paste` applies bracketed paste mode and line-ending normalisation. */
export async function pasteFromClipboard(
  term: { paste(data: string): void },
  clip: ClipboardLike | undefined = systemClipboard(),
): Promise<boolean> {
  if (!clip) return false;
  let text: string;
  try { text = await clip.readText(); } catch { return false; }
  if (!text) return false;
  term.paste(text);
  return true;
}

/** Windows Terminal: right-click copies a selection, otherwise pastes. */
export function rightClickAction(hasSelection: boolean): "copy" | "paste" {
  return hasSelection ? "copy" : "paste";
}
