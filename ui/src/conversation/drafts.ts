/**
 * Unsent message box text per session, kept while the app runs: switching
 * to the Terminal tab (or another session) unmounts the box, and coming back
 * shows what was being written.
 */
const drafts = new Map<string, string>();

export function getDraft(sessionId: string): string {
  return drafts.get(sessionId) ?? "";
}

/** Stores the text; an empty one removes the draft. */
export function setDraft(sessionId: string, text: string): void {
  if (text === "") drafts.delete(sessionId);
  else drafts.set(sessionId, text);
}

/** Forgets every draft (tests). */
export function clearDrafts(): void {
  drafts.clear();
}
