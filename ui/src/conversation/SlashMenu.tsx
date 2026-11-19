import { useEffect, useRef } from "react";
import { useT } from "../i18n";
import type { SlashEntry } from "./slashCommands";

/**
 * The command list above the message box while a `/name` is typed. The box
 * keeps the focus and drives it with its keys; a row is picked with the
 * mouse on press (before the box would lose the focus).
 */
export default function SlashMenu({ entries, active, onPick, onHover }: {
  entries: SlashEntry[]; active: number; onPick: (e: SlashEntry) => void; onHover: (i: number) => void;
}) {
  const t = useT();
  const list = useRef<HTMLUListElement>(null);
  useEffect(() => {
    const row = list.current?.children[active] as HTMLElement | undefined;
    row?.scrollIntoView?.({ block: "nearest" });
  }, [active]);
  return (
    <ul ref={list} className="conv-slash" role="listbox">
      {entries.map((e, i) => (
        <li key={`${e.builtin ? "b" : "s"}:${e.name}`} role="option" aria-selected={i === active}
          className={i === active ? "active" : undefined}
          onMouseDown={(ev) => { ev.preventDefault(); onPick(e); }} onMouseEnter={() => onHover(i)}>
          <span className="name">/{e.name}</span>
          <span className="desc">{e.description}</span>
          {e.terminal && <span className="term" title={t("conv.cmdTerminal")} aria-label={t("conv.cmdTerminal")}>⧉</span>}
        </li>
      ))}
    </ul>
  );
}
