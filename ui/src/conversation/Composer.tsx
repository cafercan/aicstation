import { useEffect, useId, useLayoutEffect, useRef, useState } from "react";
import { getSessionCommands, getSessionFiles, pasteImageToSession, sendToSession, type FileHint, type SessionSkill } from "../api";
import { errorText, useT } from "../i18n";
import { getDraft, setDraft } from "./drafts";
import { appendQuote } from "./quote";
import { useConvFontSize } from "./convAppearance";
import { matchCommands, slashQuery, type SlashEntry } from "./slashCommands";
import SlashMenu, { slashOptionId } from "./SlashMenu";
import FileMenu from "./FileMenu";
import { atQuery, completeAt } from "./atMention";

/**
 * Message box under the conversation of a session in an app terminal. Enter
 * sends the text to that terminal (the backend re-checks everything), Shift+Enter
 * adds a line. While the session waits for an approval the box is disabled:
 * typing would answer the prompt. Each new `quote` (a new `seq`) is added
 * to the text, with the caret after it, to answer under. Typing `/name`
 * opens the command menu (built-ins and the session's skills); the skills
 * are asked for again when the menu opens after `skillsKey` changed. Typing
 * `@path` lists the session folder's matching files: Tab or Enter writes the
 * picked one. An image pasted (or Alt+V, as in the terminal) goes into
 * Claude's own input: Claude Code reads the clipboard itself, and `images`
 * (the `[Image #n]` in its input box) shows what the next message takes
 * along. Unsent text survives leaving the tab (see `drafts`).
 */
export default function Composer({ sessionId, status, onShowTerminal, onSent, quote, skillsKey, sent = [], images = [] }: {
  sessionId: string; status: string; onShowTerminal: () => void; onSent?: () => void;
  quote?: { text: string; seq: number }; skillsKey?: number;
  /** Numbers of the images waiting in Claude's input (`[Image #n]`). */
  images?: number[];
  /** Messages sent before, oldest first: ↑ in the empty box brings them back. */
  sent?: string[];
}) {
  const t = useT();
  // What was being written when the box was last shown for this session.
  const [text, setText] = useState(() => getDraft(sessionId));
  useEffect(() => { setDraft(sessionId, text); }, [sessionId, text]);
  const [sending, setSending] = useState(false);
  const sendingRef = useRef(false);
  const [err, setErr] = useState<string | null>(null);
  const waiting = status === "waiting";
  const box = useRef<HTMLTextAreaElement>(null);
  /** A quote was just added: focus the box with the caret at the end. */
  const toEnd = useRef(false);

  // Only a new quote (a new seq) adds text, not a re-render with the same one.
  const quoteRef = useRef(quote);
  quoteRef.current = quote;
  const quoteSeq = quote?.seq;
  useEffect(() => {
    const q = quoteRef.current;
    if (q === undefined) return;
    toEnd.current = true;
    setText((cur) => appendQuote(cur, q.text));
    setErr(null);
  }, [quoteSeq]);

  /** Height of the empty box (its 2 rows), measured once: the grow base. */
  const baseH = useRef(0);
  const fontSize = useConvFontSize();
  const measuredAt = useRef(fontSize);
  // The box grows with its text up to twice its first height; past that its
  // content scrolls. Runs before the caret effect below, which scrolls it.
  // A new font size measures the 2 rows again.
  useLayoutEffect(() => {
    const el = box.current;
    if (el && measuredAt.current !== fontSize) {
      measuredAt.current = fontSize;
      baseH.current = 0;
      el.style.height = "";
    }
    // Not laid out (hidden): measure when it is, still at its own 2 rows.
    if (!el || (baseH.current === 0 && el.offsetHeight === 0)) return;
    if (baseH.current === 0) baseH.current = el.offsetHeight;
    const border = el.offsetHeight - el.clientHeight;
    el.style.height = "auto";
    const need = el.scrollHeight + border;
    const max = baseH.current * 2;
    el.style.height = `${Math.max(baseH.current, Math.min(need, max))}px`;
    el.style.overflowY = need > max ? "auto" : "hidden";
  }, [text, fontSize]);

  // History: `browse` is how far back the shown sent message is (0 = the
  // newest), null while the text is the user's own.
  const [browse, setBrowse] = useState<number | null>(null);
  const caretTo = useRef<"start" | "end" | number | null>(null);
  /** Where the caret is: the `@` suggestions follow the word it is in. */
  const [caret, setCaret] = useState(text.length);
  useLayoutEffect(() => {
    const el = box.current;
    const to = caretTo.current;
    if (to === null || !el) return;
    caretTo.current = null;
    const at = to === "start" ? 0 : to === "end" ? el.value.length : to;
    el.setSelectionRange(at, at);
    setCaret(at);
  }, [text]);
  /** A recalled "/command" does not open the menu: ↑/↓ keep going through the history. */
  const show = (v: string) => { setText(v); setClosedFor(v); setErr(null); };
  /** ↑/↓ through `sent`; false when the key is left to the box (moving in its lines). */
  const recall = (up: boolean, el: HTMLTextAreaElement): boolean => {
    const n = sent.length;
    if (n === 0) return false;
    if (up) {
      if (browse === null ? text !== "" : el.value.slice(0, el.selectionStart).includes("\n")) return false;
      const next = Math.min((browse ?? -1) + 1, n - 1);
      // Already at the oldest: nothing changes (and no caret move is left pending).
      if (next === browse) return true;
      setBrowse(next);
      caretTo.current = "start";
      show(sent[n - 1 - next]);
      return true;
    }
    if (browse === null || el.value.slice(el.selectionEnd).includes("\n")) return false;
    const next = browse - 1;
    setBrowse(next < 0 ? null : next);
    caretTo.current = "end";
    show(next < 0 ? "" : sent[n - 1 - next]);
    return true;
  };

  useLayoutEffect(() => {
    const el = box.current;
    if (!toEnd.current || !el) return;
    toEnd.current = false;
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
    el.scrollTop = el.scrollHeight;
  }, [text]);

  // Command menu: open while a `/name` is typed, unless Esc closed it for
  // this very text.
  const [skills, setSkills] = useState<SessionSkill[]>([]);
  /** The list's id: unique, two boxes can be on screen (split view). */
  const listId = useId();
  const [active, setActive] = useState(0);
  const [closedFor, setClosedFor] = useState<string | null>(null);
  const query = slashQuery(text);
  const entries: SlashEntry[] = query === null ? [] : matchCommands(query, skills);
  const menuOpen = entries.length > 0 && closedFor !== text && !waiting;
  const fetchedFor = useRef<number | null | undefined>(undefined);
  /** Number of the last skills request: an older reply arriving later is dropped. */
  const skillsReq = useRef(0);
  useEffect(() => {
    if (query === null) return;
    const key = skillsKey ?? null;
    if (fetchedFor.current === key) return;
    fetchedFor.current = key;
    const n = ++skillsReq.current;
    getSessionCommands(sessionId).then((s) => { if (n === skillsReq.current) setSkills(s); }, () => {});
  }, [query !== null, skillsKey, sessionId]); // eslint-disable-line react-hooks/exhaustive-deps
  // A name typed that only others contain (a command missing from the list,
  // typed in full) highlights nothing: Enter sends it as typed.
  const prefix = query !== null && entries.some((e) => e.name.toLowerCase().startsWith(query.toLowerCase()));
  useEffect(() => { setActive(prefix ? 0 : -1); }, [query, prefix]);

  // File suggestions: while the caret is in an `@` word (and no command menu shows).
  const at = menuOpen || waiting ? null : atQuery(text, caret);
  const atKey = at ? `${at.start}:${at.query}` : null;
  const [hints, setHints] = useState<FileHint[]>([]);
  const [hintActive, setHintActive] = useState(0);
  /** Number of the last files request: an older reply arriving later is dropped. */
  const filesReq = useRef(0);
  useEffect(() => {
    const n = ++filesReq.current;
    if (at === null) { setHints([]); return; }
    const q = at.query;
    // A short pause: a fast typist asks once, not per key.
    const timer = setTimeout(() => {
      getSessionFiles(sessionId, q).then((h) => { if (n === filesReq.current) { setHints(h); setHintActive(0); } }, () => {});
    }, 80);
    return () => clearTimeout(timer);
  }, [atKey, sessionId]); // eslint-disable-line react-hooks/exhaustive-deps
  const filesOpen = at !== null && hints.length > 0 && closedFor !== text;
  const complete = (h: FileHint) => {
    if (!at) return;
    const done = completeAt(text, at, h);
    caretTo.current = done.caret;
    setText(done.text);
    setErr(null);
    setBrowse(null);
  };

  /** Starts sending `value`; false when nothing was sent (waiting, a send in flight, empty). */
  const send = (value = text): boolean => {
    // Only images: the empty text sends Claude's input as it is.
    if (value.trim() === "" && images.length > 0) value = "";
    else if (value.trim() === "") return false;
    if (waiting || sendingRef.current) return false;
    sendingRef.current = true;
    setSending(true);
    setErr(null);
    const sent = value;
    const before = text;
    sendToSession(sessionId, sent)
      .then(() => {
        // Cleared only when unchanged: text edited while the send was on its
        // way is the user's and stays as it is.
        setText((cur) => (cur === before ? "" : cur));
        // The box may be gone already (Terminal tab): its draft must not keep the sent text.
        if (getDraft(sessionId) === before) setDraft(sessionId, "");
        setBrowse(null);
        onSent?.();
      }, (e: unknown) => {
        // Pasted but not submitted: the message says the text sits in the terminal.
        const pasted = typeof e === "string" && e.startsWith("send_not_submitted");
        setErr(pasted ? errorText(e) : t("conv.sendFailed", { err: errorText(e) }));
      })
      .finally(() => { sendingRef.current = false; setSending(false); });
    return true;
  };

  /** The clipboard's image into Claude's input; it shows above the box once there. */
  const pasteImage = () => {
    if (waiting) return;
    setErr(null);
    pasteImageToSession(sessionId).catch((e: unknown) => setErr(t("conv.pasteFailed", { err: errorText(e) })));
  };
  const onPaste = (e: React.ClipboardEvent<HTMLTextAreaElement>) => {
    const dt = e.clipboardData;
    const image = Array.from(dt.items ?? []).some((i) => i.kind === "file" && i.type.startsWith("image/"));
    // Text (also text copied with a picture) pastes as text.
    if (!image || dt.types.includes("text/plain")) return;
    e.preventDefault();
    pasteImage();
  };

  /** Runs a menu entry; a picker command it sent also shows the Terminal tab. */
  const pick = (e: SlashEntry) => {
    if (send(`/${e.name}`) && e.terminal) onShowTerminal();
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    // keyCode 229: WebKitGTK can send the Enter that commits an IME word with isComposing false.
    const ime = e.nativeEvent.isComposing || e.nativeEvent.keyCode === 229;
    // Alt+V: Claude Code's image paste key in the terminal (AltGr is Ctrl+Alt, not this).
    if (e.altKey && !e.ctrlKey && !e.metaKey && !e.shiftKey && (e.code === "KeyV" || e.key.toLowerCase() === "v")) {
      e.preventDefault();
      pasteImage();
      return;
    }
    if (filesOpen && !ime) {
      const n = hints.length;
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        const down = e.key === "ArrowDown";
        setHintActive((i) => (i + (down ? 1 : n - 1)) % n);
        return;
      }
      if ((e.key === "Enter" || e.key === "Tab") && !e.shiftKey) {
        e.preventDefault();
        complete(hints[Math.min(hintActive, n - 1)]);
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        setClosedFor(text);
        return;
      }
    }
    if (menuOpen && !ime) {
      const n = entries.length;
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        const down = e.key === "ArrowDown";
        setActive((i) => (i < 0 ? (down ? 0 : n - 1) : (i + (down ? 1 : n - 1)) % n));
        return;
      }
      const cur = active >= 0 ? entries[Math.min(active, n - 1)] : undefined;
      if (cur && e.key === "Enter" && !e.shiftKey) {
        e.preventDefault();
        pick(cur);
        return;
      }
      if (cur && e.key === "Tab" && !e.shiftKey) {
        e.preventDefault();
        setText(`/${cur.name} `);
        return;
      }
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        setClosedFor(text);
        return;
      }
    }
    if ((e.key === "ArrowUp" || e.key === "ArrowDown") && !ime && !e.shiftKey && !e.altKey && !e.ctrlKey && !e.metaKey) {
      if (recall(e.key === "ArrowUp", e.currentTarget)) e.preventDefault();
      return;
    }
    if (e.key !== "Enter" || e.shiftKey || ime) return;
    e.preventDefault();
    send();
  };

  return (
    <div className="conv-composer">
      {menuOpen && <SlashMenu id={listId} entries={entries} active={Math.min(active, entries.length - 1)} onPick={pick} onHover={setActive} />}
      {filesOpen && <FileMenu id={listId} hints={hints} active={Math.min(hintActive, hints.length - 1)} onPick={complete} onHover={setHintActive} />}
      {images.length > 0 && (
        <div className="conv-images" role="status">
          {t("conv.images", { list: images.map((n) => `[Image #${n}]`).join(" ") })}
        </div>
      )}
      <div className="conv-composer-row">
        <textarea ref={box} value={text} rows={2} disabled={waiting} placeholder={t("conv.placeholder")}
          aria-autocomplete="list" aria-controls={menuOpen || filesOpen ? listId : undefined}
          aria-activedescendant={menuOpen && active >= 0 ? slashOptionId(listId, Math.min(active, entries.length - 1))
            : filesOpen ? slashOptionId(listId, Math.min(hintActive, hints.length - 1)) : undefined}
          onChange={(e) => {
            setText(e.target.value); setCaret(e.target.selectionStart); setErr(null); setClosedFor(null); setBrowse(null);
          }}
          onSelect={(e) => setCaret(e.currentTarget.selectionStart)}
          onKeyDown={onKeyDown} onPaste={onPaste} />
        <button type="button" className="btn" disabled={waiting || sending || (text.trim() === "" && images.length === 0)}
          onClick={() => send()}>
          {t("conv.send")}
        </button>
      </div>
      {waiting && (
        <div className="conv-composer-note">
          <span>{t("conv.waiting")}</span>
          <button type="button" className="btn" onClick={onShowTerminal}>{t("conv.toTerminal")}</button>
        </div>
      )}
      {err && <div className="meta r" role="alert">{err}</div>}
    </div>
  );
}
