import type { ReactNode } from "react";

// Safe Markdown subset. Input is untrusted: only React elements and text nodes
// are produced (no raw HTML, no anchors).

interface InlineOpts {
  bold: boolean;
  italic: boolean;
}

function isSpace(ch: string | undefined): boolean {
  return ch === undefined || ch === " " || ch === "\t" || ch === "\n";
}

function parseInline(src: string, opts: InlineOpts, keyPrefix: string): ReactNode[] {
  const out: ReactNode[] = [];
  // Tokens whose search failed once can never match later (indexOf from a
  // later start), so remember that. Combined with the bounded link-label scan
  // below this keeps parsing roughly linear in the input size.
  const missing = new Set<string>();
  const find = (token: string, from: number): number => {
    if (missing.has(token)) return -1;
    const at = src.indexOf(token, from);
    if (at < 0) missing.add(token);
    return at;
  };
  let text = "";
  let n = 0;
  const flush = () => {
    if (text) out.push(text);
    text = "";
  };
  const push = (node: ReactNode) => {
    flush();
    out.push(node);
  };
  let i = 0;
  while (i < src.length) {
    const ch = src[i];
    if (ch === "`") {
      const end = find("`", i + 1);
      if (end > i + 1) {
        push(<code key={`${keyPrefix}${n++}`}>{src.slice(i + 1, end)}</code>);
        i = end + 1;
        continue;
      }
    } else if (ch === "*" && src[i + 1] === "*" && opts.bold && !isSpace(src[i + 2])) {
      const end = find("**", i + 2);
      if (end > i + 2 && !isSpace(src[end - 1])) {
        const key = `${keyPrefix}${n++}`;
        push(<strong key={key}>{parseInline(src.slice(i + 2, end), { ...opts, bold: false }, `${key}.`)}</strong>);
        i = end + 2;
        continue;
      }
    } else if (ch === "*" && src[i + 1] !== "*" && opts.italic && !isSpace(src[i + 1])) {
      const end = find("*", i + 1);
      if (end > i + 1 && !isSpace(src[end - 1]) && src[end + 1] !== "*") {
        const key = `${keyPrefix}${n++}`;
        push(<em key={key}>{parseInline(src.slice(i + 1, end), { bold: false, italic: false }, `${key}.`)}</em>);
        i = end + 1;
        continue;
      }
    } else if (ch === "[") {
      // A label never contains brackets: the scan stops at the next one, so
      // every character is looked at by at most one "[" scan.
      let j = i + 1;
      while (j < src.length && src[j] !== "[" && src[j] !== "]") j++;
      if (j > i + 1 && src[j] === "]" && src[j + 1] === "(") {
        const close = find(")", j + 2);
        if (close >= 0) {
          push(
            <span key={`${keyPrefix}${n++}`} title={src.slice(j + 2, close)}>
              {src.slice(i + 1, j)}
            </span>,
          );
          i = close + 1;
          continue;
        }
      }
    }
    text += ch;
    i++;
  }
  flush();
  return out;
}

const HEADING = /^(#{1,3}) +(.*)$/;
const ITEM = /^\s*([-*]|\d+\.) +(.*)$/;

export default function Markdown({ text }: { text: string }) {
  const lines = text.replace(/\r\n?/g, "\n").split("\n");
  const blocks: ReactNode[] = [];
  const inline = (s: string, key: string) => parseInline(s, { bold: true, italic: true }, `${key}.`);
  let i = 0;
  let b = 0;
  while (i < lines.length) {
    const line = lines[i];
    if (line.trim() === "") {
      i++;
    } else if (line.trimStart().startsWith("```")) {
      const code: string[] = [];
      i++;
      while (i < lines.length && !lines[i].trimStart().startsWith("```")) code.push(lines[i++]);
      i++; // closing fence (or past the end when unclosed)
      blocks.push(
        <pre key={b++}>
          <code>{code.join("\n")}</code>
        </pre>,
      );
    } else if (HEADING.test(line)) {
      const m = HEADING.exec(line)!;
      const Tag = `h${m[1].length}` as "h1" | "h2" | "h3";
      const key = `b${b++}`;
      blocks.push(<Tag key={key}>{inline(m[2], key)}</Tag>);
      i++;
    } else if (ITEM.test(line)) {
      const ordered = /^\s*\d/.test(line);
      const key = `b${b++}`;
      const items: ReactNode[] = [];
      while (i < lines.length) {
        const m = ITEM.exec(lines[i]);
        if (!m || /^\d/.test(m[1]) !== ordered) break;
        items.push(<li key={items.length}>{inline(m[2], `${key}${items.length}`)}</li>);
        i++;
      }
      blocks.push(ordered ? <ol key={key}>{items}</ol> : <ul key={key}>{items}</ul>);
    } else {
      const para: string[] = [];
      while (
        i < lines.length &&
        lines[i].trim() !== "" &&
        !lines[i].trimStart().startsWith("```") &&
        !HEADING.test(lines[i]) &&
        !ITEM.test(lines[i])
      ) {
        para.push(lines[i++]);
      }
      const key = `b${b++}`;
      blocks.push(<p key={key}>{inline(para.join("\n"), key)}</p>);
    }
  }
  return <div className="md">{blocks}</div>;
}
