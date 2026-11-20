import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type MouseEvent } from "react";
import { getConversationPage, getConversationSince, type ConvPage, type SessionCard } from "../api";
import { errorText, useT } from "../i18n";
import SessionTerminalPlaceholder from "../pages/SessionTerminalPlaceholder";
import { terminalForPid } from "../terminal/store";
import Composer from "./Composer";
import ConvStatus from "./ConvStatus";
import { inputImages } from "./statusLine";
import { useTerminalScreen } from "./useTerminalScreen";
import { useConvFontSize } from "./convAppearance";
import ConvItemView from "./ConvItemView";
import { applyPage, hasOlder, lastIndex, type ConvState } from "./convState";
import ImageViewer from "./ImageViewer";
import { quoteText } from "./quote";
import QuoteMenu from "./QuoteMenu";

/** Within this many pixels of the bottom the list follows new items. */
const PIN_PX = 40;
/** Within this many pixels of the top the older page is loaded. */
const OLDER_PX = 200;
/** After a send, the transcript gets the message a moment later. */
const AFTER_SEND_MS = 500;

const NO_IMAGES: number[] = [];
const sameNumbers = (a: number[], b: number[]) => a.length === b.length && a.every((n, i) => n === b[i]);

interface Props {
  sessionId: string;
  /** The session's card in the current snapshot; missing (or pid 0) when it is closed. */
  card: SessionCard | undefined;
  /** "Terminal tab" button while the session waits for an approval. */
  onShowTerminal: () => void;
  onTerminal: (id: number) => void;
  onAdopted: (id: number, sessionId: string) => void;
  onResume: (sessionId: string, name: string) => Promise<unknown>;
}

/**
 * Conversation tab: the session's transcript (user messages, Claude's
 * replies, tool steps with their images), newest at the bottom. Refreshes
 * when the snapshot reports new activity; older pages load when scrolled
 * to the top. Below it the message box (session in an app terminal) or the
 * Terminal tab's placeholder (outside the app: Move to app; closed: Resume).
 */
export default function ConversationPage({ sessionId, card, onShowTerminal, onTerminal, onAdopted, onResume }: Props) {
  const t = useT();
  const fontSize = useConvFontSize();
  const [conv, setConv] = useState<ConvState | null>(null);
  /** Why the first load failed (`code` is the backend error code, `text` its translation). */
  const [err, setErr] = useState<{ code: string; text: string } | null>(null);
  const [viewing, setViewing] = useState<string | null>(null);
  /** Right-click menu over selected text: where, and the text it acts on. */
  const [menu, setMenu] = useState<{ x: number; y: number; text: string } | null>(null);
  /** Last quote handed to the message box; `seq` makes a repeat a new one. */
  const [quote, setQuote] = useState<{ text: string; seq: number } | undefined>(undefined);
  /** The latest state, also between a merge and its render (async callbacks read it). */
  const convRef = useRef<ConvState | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const alive = useRef(true);
  /** The list was at the bottom before the last change, so it stays there. */
  const pinned = useRef(true);
  /** Scrolled away from the bottom: the "to the newest" button shows. */
  const [away, setAway] = useState(false);
  /** Scroll height before an older page was prepended, to keep the view in place. */
  const prepended = useRef<number | null>(null);
  const loadingOlder = useRef(false);
  /** A load (first page or since) in flight; `again`: activity arrived meanwhile. */
  const since = useRef({ busy: false, again: false });

  const merge = (p: ConvPage) => {
    const prev = convRef.current;
    // A new epoch (rewind, other branch) replaces the list: show its newest end.
    if (prev && prev.epoch !== p.epoch) { pinned.current = true; prepended.current = null; }
    const next = applyPage(prev, p);
    convRef.current = next;
    setConv(next);
  };

  useEffect(() => {
    alive.current = true;
    return () => { alive.current = false; };
  }, []);

  /**
   * Loads what is new: the newest page while nothing is shown yet (first
   * load, or after it failed: a new or cleared session has no transcript
   * yet), else the items since the last one.
   */
  const refresh = useCallback(() => {
    if (since.current.busy) { since.current.again = true; return; }
    since.current.busy = true;
    const s = convRef.current;
    const load = s ? getConversationSince(sessionId, s.epoch, lastIndex(s)) : getConversationPage(sessionId);
    load.then((p) => {
      if (!alive.current) return;
      if (!s) pinned.current = true;
      setErr(null);
      merge(p);
    }, (e: unknown) => {
      // Nothing shown yet: say why. A failed refresh of a shown list is left
      // unsaid by design: what is shown stays valid and the next activity
      // (or send) tries again.
      if (alive.current && !s) setErr({ code: typeof e === "string" ? e.split(":")[0] : "", text: errorText(e) });
    }).finally(() => {
      since.current.busy = false;
      if (since.current.again && alive.current) { since.current.again = false; refresh(); }
    });
  }, [sessionId]);

  useEffect(() => { refresh(); }, [refresh]);

  const live = card !== undefined && card.pid !== 0;
  // The transcript size too: Claude Code marks a turn idle before it writes
  // the reply, whose older timestamp leaves lastActivityMs unchanged.
  const activity = live ? `${card.lastActivityMs}:${card.transcriptBytes}` : null;
  const seen = useRef(activity);
  useEffect(() => {
    if (activity === null || activity === seen.current) return;
    seen.current = activity;
    refresh();
  }, [activity, refresh]);

  const onSent = useCallback(() => {
    window.setTimeout(() => { if (alive.current) refresh(); }, AFTER_SEND_MS);
  }, [refresh]);

  const loadOlder = useCallback(() => {
    const s = convRef.current;
    const el = listRef.current;
    if (!s || !el || !hasOlder(s) || loadingOlder.current) return;
    loadingOlder.current = true;
    getConversationPage(sessionId, s.start).then((p) => {
      // Cleared before the merge renders, so the layout effect can load the next page.
      loadingOlder.current = false;
      if (!alive.current) return;
      // A since call replaced the window meanwhile: this page no longer fits.
      const cur = convRef.current;
      if (!cur || cur.epoch !== s.epoch || cur.start !== s.start) return;
      prepended.current = listRef.current?.scrollHeight ?? null;
      merge(p);
    }, () => { loadingOlder.current = false; });
  }, [sessionId]);

  // After every change: keep the old content in place above a prepended
  // page, or follow the bottom; then fetch more history if at the top.
  useLayoutEffect(() => {
    const el = listRef.current;
    if (!el || !conv) return;
    if (prepended.current !== null) {
      el.scrollTop += el.scrollHeight - prepended.current;
      prepended.current = null;
    } else if (pinned.current) {
      el.scrollTop = el.scrollHeight;
    }
    if (el.scrollTop < OLDER_PX) loadOlder();
  }, [conv, loadOlder]);

  const onScroll = () => {
    const el = listRef.current;
    if (!el) return;
    pinned.current = el.scrollHeight - el.scrollTop - el.clientHeight <= PIN_PX;
    setAway(!pinned.current);
    if (el.scrollTop < OLDER_PX) loadOlder();
  };
  const toBottom = () => {
    const el = listRef.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
    pinned.current = true;
    setAway(false);
  };

  const hosted = live && terminalForPid(card.parentPid) !== undefined;
  /** The user's messages of the loaded pages, oldest first, for ↑ in the message box. */
  const sent = useMemo(
    () => (conv?.items ?? []).flatMap((i) => (i.kind === "user" && i.text.trim() !== "" ? [i.text] : [])),
    [conv],
  );
  /** Images pasted into Claude's input, read from its app terminal's screen. */
  const images = useTerminalScreen(card?.parentPid ?? null, inputImages, sameNumbers, NO_IMAGES);

  // Right click on text selected in the list opens the Quote/Copy menu;
  // without a selection the system's own menu stays.
  const onContextMenu = (e: MouseEvent) => {
    const sel = window.getSelection();
    const text = sel?.toString() ?? "";
    if (text.trim() === "" || !sel?.anchorNode || !listRef.current?.contains(sel.anchorNode)) return;
    e.preventDefault();
    setMenu({ x: e.clientX, y: e.clientY, text });
  };
  const quoteSelection = () => {
    if (!menu) return;
    const text = quoteText(menu.text);
    setQuote((q) => ({ text, seq: (q?.seq ?? 0) + 1 }));
    setMenu(null);
  };
  const copySelection = () => {
    if (!menu) return;
    void navigator.clipboard?.writeText(menu.text).catch(() => {});
    setMenu(null);
  };

  // A live session has no transcript until its first message (new session,
  // after /clear): that is an empty conversation, not an error.
  const notYet = err !== null && live && err.code === "conv_unavailable";

  let body;
  if (notYet) body = <div className="conv-status">{t("conv.empty")}</div>;
  else if (err !== null) body = <div className="meta r" role="alert">{err.text}</div>;
  else if (conv === null) body = <div className="conv-status">{t("conv.loading")}</div>;
  else if (conv.items.length === 0) body = <div className="conv-status">{t("conv.empty")}</div>;
  else {
    // Keyed by epoch too: after a branch change an index names another item,
    // which must not inherit the old one's state (an open step list).
    body = conv.items.map((item, i) => (
      <ConvItemView key={`${conv.epoch}:${conv.start + i}`} sessionId={sessionId} item={item} onOpen={setViewing} />
    ));
  }

  return (
    <div className="conv" style={{ "--conv-font": `${fontSize}px` } as CSSProperties}>
      <div className="conv-list" ref={listRef} onScroll={onScroll} onContextMenu={onContextMenu}>
        <div className="conv-items">{body}</div>
        {away && (
          <div className="conv-to-bottom">
            <button type="button" className="btn" onClick={toBottom}
              aria-label={t("conv.toBottom")} title={t("conv.toBottom")}>↓</button>
          </div>
        )}
      </div>
      <div className="conv-foot">
        {live && <ConvStatus card={card} />}
        {hosted
          ? <Composer sessionId={sessionId} status={card.status} onShowTerminal={onShowTerminal} onSent={onSent} quote={quote}
            skillsKey={card.transcriptBytes} sent={sent} images={images} />
          : <SessionTerminalPlaceholder card={card} sessionId={sessionId} onTerminal={onTerminal}
            onAdopted={onAdopted} onResume={onResume} />}
      </div>
      {viewing !== null && <ImageViewer src={viewing} onClose={() => setViewing(null)} />}
      {menu && (
        <QuoteMenu x={menu.x} y={menu.y} onQuote={hosted ? quoteSelection : undefined}
          onCopy={copySelection} onClose={() => setMenu(null)} />
      )}
    </div>
  );
}
