import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { getConversationPage, getConversationSince, type ConvPage, type SessionCard } from "../api";
import { errorText, useT } from "../i18n";
import SessionTerminalPlaceholder from "../pages/SessionTerminalPlaceholder";
import { terminalForPid } from "../terminal/store";
import Composer from "./Composer";
import ConvItemView from "./ConvItemView";
import { applyPage, hasOlder, lastIndex, type ConvState } from "./convState";
import ImageViewer from "./ImageViewer";

/** Within this many pixels of the bottom the list follows new items. */
const PIN_PX = 40;
/** Within this many pixels of the top the older page is loaded. */
const OLDER_PX = 200;
/** After a send, the transcript gets the message a moment later. */
const AFTER_SEND_MS = 500;

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
  const [conv, setConv] = useState<ConvState | null>(null);
  const [err, setErr] = useState<string | null>(null);
  const [viewing, setViewing] = useState<string | null>(null);
  /** The latest state, also between a merge and its render (async callbacks read it). */
  const convRef = useRef<ConvState | null>(null);
  const listRef = useRef<HTMLDivElement>(null);
  const alive = useRef(true);
  /** The list was at the bottom before the last change, so it stays there. */
  const pinned = useRef(true);
  /** Scroll height before an older page was prepended, to keep the view in place. */
  const prepended = useRef<number | null>(null);
  const loadingOlder = useRef(false);
  /** A since call in flight; `again`: activity arrived meanwhile (or before the first page). */
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

  const refresh = useCallback(() => {
    const s = convRef.current;
    if (!s || since.current.busy) { since.current.again = true; return; }
    since.current.busy = true;
    getConversationSince(sessionId, s.epoch, lastIndex(s)).then((p) => {
      if (alive.current) merge(p);
    }, () => { /* the next activity tries again */ }).finally(() => {
      since.current.busy = false;
      if (since.current.again && alive.current) { since.current.again = false; refresh(); }
    });
  }, [sessionId]);

  useEffect(() => {
    getConversationPage(sessionId).then((p) => {
      if (!alive.current) return;
      pinned.current = true;
      merge(p);
      if (since.current.again) { since.current.again = false; refresh(); }
    }, (e: unknown) => { if (alive.current) setErr(errorText(e)); });
  }, [sessionId, refresh]);

  const live = card !== undefined && card.pid !== 0;
  const activity = live ? card.lastActivityMs : null;
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
    if (el.scrollTop < OLDER_PX) loadOlder();
  };

  const hosted = live && terminalForPid(card.parentPid) !== undefined;

  let body;
  if (err !== null) body = <div className="meta r" role="alert">{err}</div>;
  else if (conv === null) body = <div className="conv-status">{t("conv.loading")}</div>;
  else if (conv.items.length === 0) body = <div className="conv-status">{t("conv.empty")}</div>;
  else {
    body = conv.items.map((item, i) => (
      <ConvItemView key={conv.start + i} sessionId={sessionId} item={item} onOpen={setViewing} />
    ));
  }

  return (
    <div className="conv">
      <div className="conv-list" ref={listRef} onScroll={onScroll}>
        <div className="conv-items">{body}</div>
      </div>
      <div className="conv-foot">
        {hosted
          ? <Composer sessionId={sessionId} status={card.status} onShowTerminal={onShowTerminal} onSent={onSent} />
          : <SessionTerminalPlaceholder card={card} sessionId={sessionId} onTerminal={onTerminal}
            onAdopted={onAdopted} onResume={onResume} />}
      </div>
      {viewing !== null && <ImageViewer src={viewing} onClose={() => setViewing(null)} />}
    </div>
  );
}
