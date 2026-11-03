import { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import "@xterm/xterm/css/xterm.css";
import { butaglyphPreview } from "../api";
import { useT } from "../i18n";
import { useSettings } from "../settings";
import { useAppearance, xtermOptions } from "../terminal/appearance";
import { renderOptions } from "../terminal/theme";
import { SCHEMES } from "../terminal/schemes";

const ROWS = 11;
/** Wide enough that long prompt lines never wrap (the box cuts them). */
const COLS = 300;
const E = "\x1b[";

/** A plain prompt for when the shell's own prompt is used. */
const PLAIN = [`${E}1;32muser@host${E}0m:${E}1;34m~/project${E}0m$ `, `${E}1;32muser@host${E}0m:${E}1;34m~/project${E}0m$ `];

/** The sample screen: two commands with typical colored output, the 16
 *  ANSI colors, and the prompt again (after a failure for buta). */
export function sampleScreen(prompts: string[]): string {
  const p = prompts.map((s) => s.replace(/\r?\n/g, "\r\n"));
  const swatch = (from: number) =>
    Array.from({ length: 8 }, (_, i) => `${E}${from + i}m  ${E}0m`).join("");
  return [
    `${p[0]}ls --color`,
    `${E}1;34msrc${E}0m  ${E}1;32mbuild.sh${E}0m  README.md  ${E}1;36mlink${E}0m  ${E}1;31marchive.zip${E}0m`,
    `${p[0]}git status --short`,
    ` ${E}31mM${E}0m src/main.rs   ${E}32mA${E}0m src/new.rs   ${E}33m??${E}0m notes.txt`,
    `${swatch(40)}  ${E}1mbold${E}0m ${E}3mitalic${E}0m ${E}4munderline${E}0m`,
    `${swatch(100)}  ${E}2mdim${E}0m ${E}7minverse${E}0m`,
    `${p[1] ?? p[0]}`,
  ].join("\r\n");
}

/**
 * A small read-only terminal showing the current appearance, and with the
 * butaglyph prompt chosen, prompts drawn by the real buta.
 */
export default function TerminalPreview() {
  const t = useT();
  const look = useAppearance();
  const settings = useSettings();
  const host = useRef<HTMLDivElement>(null);
  const term = useRef<Terminal | null>(null);
  const [prompts, setPrompts] = useState<string[]>(PLAIN);

  const buta = settings?.prompt === "butaglyph";
  const preset = settings?.promptPreset ?? "";
  useEffect(() => {
    if (!buta) { setPrompts(PLAIN); return; }
    let live = true;
    butaglyphPreview(preset)
      .then((p) => { if (live && Array.isArray(p) && p.length) setPrompts(p); })
      .catch(() => { if (live) setPrompts(PLAIN); });
    return () => { live = false; };
  }, [buta, preset]);

  useEffect(() => {
    const el = host.current;
    if (!el) return;
    try {
      const tm = new Terminal({ ...renderOptions, rows: ROWS, cols: COLS, disableStdin: true });
      tm.open(el);
      term.current = tm;
      return () => { term.current = null; tm.dispose(); };
    } catch {
      return; // no canvas (tests): the preview stays empty
    }
  }, []);

  useEffect(() => {
    const tm = term.current;
    if (!tm) return;
    try {
      Object.assign(tm.options, { ...xtermOptions(look), scrollback: 0 });
      tm.reset();
      // As tall as the sample (two-line prompts make it longer); the width
      // is cut by the box.
      const text = sampleScreen(prompts);
      const rows = text.split("\r\n").length;
      if (tm.rows !== rows) tm.resize(COLS, rows);
      tm.write(text);
    } catch { /* not rendered */ }
  }, [look, prompts]);

  return (
    <div className="term-preview" aria-label={t("look.preview")} role="img"
      style={{ background: SCHEMES[look.scheme].theme.background, padding: look.padding }}>
      <div ref={host} />
    </div>
  );
}
