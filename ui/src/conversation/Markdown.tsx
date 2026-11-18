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
const SEP_CELL = /^\s*(:?)-+(:?)\s*$/;

type Align = "left" | "center" | "right" | undefined;

/** Cells of a pipe table row: outer pipes optional, `\|` is a literal pipe. One pass. */
function splitRow(line: string): string[] {
  let s = line.trim();
  if (s.startsWith("|")) s = s.slice(1);
  if (s.endsWith("|") && !s.endsWith("\\|")) s = s.slice(0, -1);
  const cells: string[] = [];
  let cur = "";
  for (let i = 0; i < s.length; i++) {
    if (s[i] === "\\" && s[i + 1] === "|") { cur += "|"; i++; }
    else if (s[i] === "|") { cells.push(cur.trim()); cur = ""; }
    else cur += s[i];
  }
  cells.push(cur.trim());
  return cells;
}

/** Column alignments when `line` is the separator row under a header of `n` cells. */
function separator(line: string, n: number): Align[] | null {
  if (!line.includes("|") || !line.includes("-")) return null;
  const cells = splitRow(line);
  if (cells.length !== n) return null;
  const out: Align[] = [];
  for (const c of cells) {
    const m = SEP_CELL.exec(c);
    if (!m) return null;
    out.push(m[1] && m[2] ? "center" : m[2] ? "right" : m[1] ? "left" : undefined);
  }
  return out;
}

/** A pipe table starts at `i`: a header row with a pipe and a matching separator row below. */
function tableAt(lines: string[], i: number): Align[] | null {
  if (!lines[i].includes("|") || i + 1 >= lines.length) return null;
  return separator(lines[i + 1], splitRow(lines[i]).length);
}

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
    } else if (tableAt(lines, i)) {
      const align = tableAt(lines, i)!;
      const key = `b${b++}`;
      const row = (src: string, r: number, Cell: "th" | "td") => {
        const cells = splitRow(src);
        return (
          <tr key={r}>
            {align.map((a, c) => (
              <Cell key={c} style={a ? { textAlign: a } : undefined}>{inline(cells[c] ?? "", `${key}.${r}.${c}`)}</Cell>
            ))}
          </tr>
        );
      };
      const head = row(lines[i], 0, "th");
      i += 2;
      const body: ReactNode[] = [];
      while (i < lines.length && lines[i].trim() !== "" && lines[i].includes("|") && !lines[i].trimStart().startsWith("```")) {
        body.push(row(lines[i++], body.length + 1, "td"));
      }
      blocks.push(
        <div key={key} className="md-table-wrap">
          <table className="md-table">
            <thead>{head}</thead>
            <tbody>{body}</tbody>
          </table>
        </div>,
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
        !ITEM.test(lines[i]) &&
        !tableAt(lines, i)
      ) {
        para.push(lines[i++]);
      }
      const key = `b${b++}`;
      blocks.push(<p key={key}>{inline(para.join("\n"), key)}</p>);
    }
  }
  return <div className="md">{blocks}</div>;
}
