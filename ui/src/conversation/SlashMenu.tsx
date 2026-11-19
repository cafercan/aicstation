import { useEffect, useRef } from "react";
import { useT } from "../i18n";
import type { SlashEntry } from "./slashCommands";

/** Id of row `i` of the list `listId` (the box points at it for screen readers). */
export const slashOptionId = (listId: string, i: number) => `${listId}-${i}`;

/**
 * The command list above the message box while a `/name` is typed. The box
 * keeps the focus and drives it with its keys; a row is picked with the
 * mouse on press (before the box would lose the focus).
 */
export default function SlashMenu({ id, entries, active, onPick, onHover }: {
  id: string; entries: SlashEntry[]; active: number; onPick: (e: SlashEntry) => void; onHover: (i: number) => void;
}) {
  const t = useT();
  const list = useRef<HTMLUListElement>(null);
  // Also when the list changed under the same index (a narrower query).
  const names = entries.map((e) => e.name).join("\n");
  useEffect(() => {
    const row = list.current?.children[active] as HTMLElement | undefined;
    row?.scrollIntoView?.({ block: "nearest" });
  }, [active, names]);
  return (
    <ul ref={list} id={id} className="conv-slash" role="listbox">
      {entries.map((e, i) => (
        <li key={`${e.builtin ? "b" : "s"}:${e.name}`} id={slashOptionId(id, i)} role="option" aria-selected={i === active}
          className={i === active ? "active" : undefined}
          // A move, not mouseenter: the list scrolling under a still pointer
          // (arrows) must not take the highlight.
          onMouseDown={(ev) => { ev.preventDefault(); onPick(e); }} onMouseMove={() => { if (i !== active) onHover(i); }}>
          <span className="name">/{e.name}</span>
          <span className="desc">{e.description}</span>
          {e.terminal && <span className="term" title={t("conv.cmdTerminal")} aria-label={t("conv.cmdTerminal")}>⧉</span>}
        </li>
      ))}
    </ul>
  );
}
