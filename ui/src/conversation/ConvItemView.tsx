import { memo, useState } from "react";
import type { ConvImageRef, ConvItem, ConvStep } from "../api";
import { locale, useT } from "../i18n";
import { fmtElapsed } from "../format";
import ImageThumb from "./ImageThumb";
import Markdown from "./Markdown";
import { stepSummary } from "./summary";

type Open = (src: string) => void;

function Thumbs({ sessionId, images, onOpen }: { sessionId: string; images: ConvImageRef[]; onOpen: Open }) {
  if (images.length === 0) return null;
  return (
    <span className="conv-thumbs">
      {images.map((im) => <ImageThumb key={`${im.offset}:${im.ordinal}`} sessionId={sessionId} image={im} onOpen={onOpen} />)}
    </span>
  );
}

/** A run of tool calls: one muted summary line, a click lists the calls. */
function Steps({ sessionId, steps, onOpen }: { sessionId: string; steps: ConvStep[]; onOpen: Open }) {
  const t = useT();
  const [open, setOpen] = useState(false);
  const images = steps.flatMap((s) => s.images);
  return (
    <div className="conv-steps">
      <div className="conv-steps-head">
        <button type="button" className="conv-steps-line" aria-expanded={open} onClick={() => setOpen(!open)}>
          {stepSummary(steps, t)} <span className="chev" aria-hidden="true">›</span>
        </button>
        {!open && <Thumbs sessionId={sessionId} images={images} onOpen={onOpen} />}
      </div>
      {open && (
        <ul className="conv-step-list">
          {steps.map((s, i) => (
            <li key={i} className={s.isError ? "err" : s.done ? undefined : "running"}>
              <span className="tool">{s.tool}</span>
              <span className="target">{s.target}</span>
              {s.isError && <span className="mark" aria-hidden="true">✕</span>}
              <Thumbs sessionId={sessionId} images={s.images} onOpen={onOpen} />
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/**
 * Claude's thinking: a muted line with the text open under it, since it
 * often says what Claude is doing next; a click on the line hides the text.
 */
function Thinking({ text, truncated }: { text: string; truncated: boolean }) {
  const t = useT();
  const [open, setOpen] = useState(true);
  return (
    <div className="conv-thinking">
      <button type="button" className="conv-steps-line" aria-expanded={open} onClick={() => setOpen(!open)}>
        <span aria-hidden="true">∴ </span>{t("conv.thought")} <span className="chev" aria-hidden="true">›</span>
      </button>
      {open && (
        <div className="conv-thinking-text">
          <Markdown text={text} />
          {truncated && <div className="conv-note">{t("conv.truncated")}</div>}
        </div>
      )}
    </div>
  );
}

/**
 * One conversation item; images open through `onOpen`. Memoised: a merge
 * keeps the unchanged items' objects, so their Markdown is not parsed again.
 */
function ConvItemView({ sessionId, item, onOpen }: { sessionId: string; item: ConvItem; onOpen: Open }) {
  const t = useT();
  switch (item.kind) {
    case "user":
      return (
        <div className="conv-user">
          <div className="conv-bubble">
            {item.text && <div className="conv-user-text">{item.text}</div>}
            <Thumbs sessionId={sessionId} images={item.images} onOpen={onOpen} />
            {item.truncated && <div className="conv-note">{t("conv.truncated")}</div>}
          </div>
          {item.files.length > 0 && (
            // Like the terminal's "Read gizli.txt" under a message naming @gizli.txt.
            <ul className="conv-files" aria-label={t("conv.files")}>
              {item.files.map((f) => (
                <li key={f.path} title={f.path}>
                  <span aria-hidden="true">{f.dir ? "▸ " : "≡ "}</span>{f.dir && !/[\\/]$/.test(f.path) ? `${f.path}/` : f.path}
                </li>
              ))}
            </ul>
          )}
        </div>
      );
    case "assistant":
      return (
        <div className="conv-assistant">
          <Markdown text={item.text} />
          {item.truncated && <div className="conv-note">{t("conv.truncated")}</div>}
        </div>
      );
    case "thinking":
      return <Thinking text={item.text} truncated={item.truncated} />;
    case "steps":
      return <Steps sessionId={sessionId} steps={item.steps} onOpen={onOpen} />;
    case "compacted":
      return <div className="conv-divider" role="separator"><span>{t("conv.compacted")}</span></div>;
    case "notice": {
      const text = item.text || t("conv.taskFinished");
      return <div className="conv-notice" title={text}>{text}</div>;
    }
    case "turnDone": {
      // Like the terminal's "Worked for 1m 30s · done 16:31" under a turn.
      const end = new Date(item.atMs);
      const time = end.toLocaleTimeString(locale(), { hour: "2-digit", minute: "2-digit" });
      return (
        <div className="conv-turn" title={end.toLocaleString(locale())}>
          <span aria-hidden="true">✻ </span>
          <span>{t("conv.worked", { dur: fmtElapsed(item.durationMs), time })}</span>
        </div>
      );
    }
  }
}

export default memo(ConvItemView);
