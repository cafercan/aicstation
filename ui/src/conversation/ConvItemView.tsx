import { useState } from "react";
import type { ConvImageRef, ConvItem, ConvStep } from "../api";
import { useT } from "../i18n";
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

/** One conversation item; images open through `onOpen`. */
export default function ConvItemView({ sessionId, item, onOpen }: { sessionId: string; item: ConvItem; onOpen: Open }) {
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
        </div>
      );
    case "assistant":
      return (
        <div className="conv-assistant">
          <Markdown text={item.text} />
          {item.truncated && <div className="conv-note">{t("conv.truncated")}</div>}
        </div>
      );
    case "steps":
      return <Steps sessionId={sessionId} steps={item.steps} onOpen={onOpen} />;
    case "compacted":
      return <div className="conv-divider" role="separator"><span>{t("conv.compacted")}</span></div>;
  }
}
