import { useEffect, useRef } from "react";
import type { FileHint } from "../api";
import { useT } from "../i18n";
import { slashOptionId } from "./SlashMenu";

/**
 * The file list above the message box while an `@path` is typed, driven like
 * the command list: the box keeps the focus and its keys, a row is picked on
 * mouse press.
 */
export default function FileMenu({ id, hints, active, onPick, onHover }: {
  id: string; hints: FileHint[]; active: number; onPick: (h: FileHint) => void; onHover: (i: number) => void;
}) {
  const t = useT();
  const list = useRef<HTMLUListElement>(null);
  const paths = hints.map((h) => h.path).join("\n");
  useEffect(() => {
    const row = list.current?.children[active] as HTMLElement | undefined;
    row?.scrollIntoView?.({ block: "nearest" });
  }, [active, paths]);
  return (
    <ul ref={list} id={id} className="conv-slash conv-filemenu" role="listbox" aria-label={t("conv.fileMenu")}>
      {hints.map((h, i) => (
        <li key={`${h.dir ? "d" : "f"}:${h.path}`} id={slashOptionId(id, i)} role="option" aria-selected={i === active}
          className={i === active ? "active" : undefined}
          onMouseDown={(ev) => { ev.preventDefault(); onPick(h); }} onMouseMove={() => { if (i !== active) onHover(i); }}>
          <span className="kind" aria-hidden="true">{h.dir ? "▸" : "≡"}</span>
          <span className="name">{h.dir ? `${h.path}/` : h.path}</span>
        </li>
      ))}
    </ul>
  );
}
