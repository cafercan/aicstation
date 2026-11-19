/**
 * Selected conversation text as a Markdown quote: surrounding blank lines and
 * spaces dropped, every line prefixed with "> " (a blank line with ">").
 */
export function quoteText(sel: string): string {
  return sel
    .replace(/\r\n?/g, "\n")
    .trim()
    .split("\n")
    .map((l) => (l.trim() === "" ? ">" : `> ${l.trimEnd()}`))
    .join("\n");
}

/**
 * Message box text with `quote` added: after what is already there with one
 * blank line between, then a blank line to answer under.
 */
export function appendQuote(cur: string, quote: string): string {
  const before = cur.trimEnd();
  return `${before === "" ? "" : `${before}\n\n`}${quote}\n\n`;
}
