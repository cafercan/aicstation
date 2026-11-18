import { useEffect, useRef, useState } from "react";
import { getConversationImage, type ConvImageRef } from "../api";
import { useT } from "../i18n";
import { createImageCache } from "./imageCache";

/** Loaded images of every conversation view (keys carry the session id). */
const cache = createImageCache(30);

const MEDIA = /^image\/(png|jpeg|gif|webp)$/;

/**
 * Small fixed-height thumbnail of an image in the transcript. Loads once it
 * scrolls near the viewport (at once where IntersectionObserver is missing);
 * a click hands the `data:` URL to `onOpen`. A failed load shows a
 * broken-image mark.
 */
export default function ImageThumb({ sessionId, image, onOpen }: {
  sessionId: string; image: ConvImageRef; onOpen: (src: string) => void;
}) {
  const t = useT();
  const key = `${sessionId}:${image.offset}:${image.ordinal}`;
  const [url, setUrl] = useState<string | undefined>(() => cache.get(key));
  const [failed, setFailed] = useState(false);
  const [visible, setVisible] = useState(false);
  const ref = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (url !== undefined) return;
    const el = ref.current;
    if (typeof IntersectionObserver === "undefined" || !el) { setVisible(true); return; }
    const io = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) { setVisible(true); io.disconnect(); }
    }, { root: el.closest(".conv-list"), rootMargin: "300px" });
    io.observe(el);
    return () => io.disconnect();
  }, [url]);

  useEffect(() => {
    if (!visible || url !== undefined) return;
    let alive = true;
    getConversationImage(sessionId, image.offset, image.ordinal).then((img) => {
      if (!MEDIA.test(img.mediaType)) throw new Error("media type");
      const src = `data:${img.mediaType};base64,${img.data}`;
      cache.set(key, src);
      if (alive) setUrl(src);
    }).catch(() => { if (alive) setFailed(true); });
    return () => { alive = false; };
  }, [visible, url, key, sessionId, image.offset, image.ordinal]);

  if (failed) {
    const label = t("conv.imageFailed");
    return (
      <span className="conv-thumb broken" role="img" aria-label={label} title={label}>
        <svg width="20" height="20" viewBox="0 0 20 20" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="1.4">
          <rect x="2.5" y="3.5" width="15" height="13" rx="1.5" />
          <path d="M2.5 13l4-4 3 3 2-2 6 6" />
          <path d="M3 17L17 3" />
        </svg>
      </span>
    );
  }
  const label = t("conv.imageOpen");
  return (
    <button ref={ref} type="button" className={url ? "conv-thumb" : "conv-thumb loading"} aria-label={label} title={label}
      disabled={!url} onClick={() => { if (url) onOpen(url); }}>
      {url && <img src={url} alt="" draggable={false} />}
    </button>
  );
}
