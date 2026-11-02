import { useEffect, useRef, useState } from "react";
import type { ISearchOptions, SearchAddon } from "@xterm/addon-search";

export type SearchLike = Pick<SearchAddon, "findNext" | "findPrevious" | "clearDecorations" | "onDidChangeResults">;

interface Props { search: SearchLike; onClose: () => void }

const OPTS: ISearchOptions = {
  decorations: {
    matchBackground: "#5E4A00",
    matchBorder: "#C4A000",
    matchOverviewRuler: "#C4A000",
    activeMatchBackground: "#E95420",
    activeMatchBorder: "#FFFFFF",
    activeMatchColorOverviewRuler: "#E95420",
  },
};

/** Ctrl+F search over the terminal buffer (Enter / Shift+Enter / Esc). */
export default function SearchBar({ search, onClose }: Props) {
  const input = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  const [result, setResult] = useState<{ resultIndex: number; resultCount: number } | null>(null);

  useEffect(() => { input.current?.focus(); input.current?.select(); }, []);
  useEffect(() => {
    const d = search.onDidChangeResults((r) => setResult(r));
    return () => d.dispose();
  }, [search]);

  const next = () => { if (query) search.findNext(query, OPTS); };
  const prev = () => { if (query) search.findPrevious(query, OPTS); };
  const close = () => { search.clearDecorations(); onClose(); };

  const onChange = (q: string) => {
    setQuery(q);
    if (q) search.findNext(q, { ...OPTS, incremental: true });
    else { search.clearDecorations(); setResult(null); }
  };

  let label = "";
  if (query && result) {
    label = result.resultCount === 0 ? "Sonuç yok"
      : `${result.resultIndex >= 0 ? result.resultIndex + 1 : "?"}/${result.resultCount}`;
  }

  return (
    <div className="term-search" onMouseDown={(e) => e.stopPropagation()}>
      <input ref={input} type="search" aria-label="Terminalde ara" placeholder="Ara…" value={query}
        spellCheck={false}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter") { e.preventDefault(); if (e.shiftKey) prev(); else next(); }
          else if (e.key === "Escape") { e.preventDefault(); close(); }
        }} />
      <span className="count">{label}</span>
      <button type="button" title="Önceki (Shift+Enter)" onClick={prev}>↑</button>
      <button type="button" title="Sonraki (Enter)" onClick={next}>↓</button>
      <button type="button" title="Kapat (Esc)" onClick={close}>×</button>
    </div>
  );
}
