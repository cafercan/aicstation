import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { getSessionCommands, sendToSession, type SessionSkill } from "../api";
import { errorText, useT } from "../i18n";
import { appendQuote } from "./quote";
import { useConvFontSize } from "./convAppearance";
import { matchCommands, slashQuery, type SlashEntry } from "./slashCommands";
import SlashMenu from "./SlashMenu";

/**
 * Message box under the conversation of a session in an app terminal. Enter
 * sends the text to that terminal (the backend re-checks everything), Shift+Enter
 * adds a line. While the session waits for an approval the box is disabled:
 * typing would answer the prompt. Each new `quote` (a new `seq`) is added
 * to the text, with the caret after it, to answer under. Typing `/name`
 * opens the command menu (built-ins and the session's skills); the skills
 * are asked for again when the menu opens after `skillsKey` changed.
 */
export default function Composer({ sessionId, status, onShowTerminal, onSent, quote, skillsKey }: {
  sessionId: string; status: string; onShowTerminal: () => void; onSent?: () => void;
  quote?: { text: string; seq: number }; skillsKey?: number;
}) {
  const t = useT();
  const [text, setText] = useState("");
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
  const [active, setActive] = useState(0);
  const [closedFor, setClosedFor] = useState<string | null>(null);
  const query = slashQuery(text);
  const entries: SlashEntry[] = query === null ? [] : matchCommands(query, skills);
  const menuOpen = entries.length > 0 && closedFor !== text && !waiting;
  const fetchedFor = useRef<number | null | undefined>(undefined);
  useEffect(() => {
    if (query === null) return;
    const key = skillsKey ?? null;
    if (fetchedFor.current === key) return;
    fetchedFor.current = key;
    getSessionCommands(sessionId).then(setSkills, () => {});
  }, [query !== null, skillsKey, sessionId]); // eslint-disable-line react-hooks/exhaustive-deps
  // A name typed that only others contain (a command missing from the list,
  // typed in full) highlights nothing: Enter sends it as typed.
  const prefix = query !== null && entries.some((e) => e.name.toLowerCase().startsWith(query.toLowerCase()));
  useEffect(() => { setActive(prefix ? 0 : -1); }, [query, prefix]);

  const send = (value = text) => {
    if (waiting || sendingRef.current || value.trim() === "") return;
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
        onSent?.();
      }, (e: unknown) => {
        // Pasted but not submitted: the message says the text sits in the terminal.
        const pasted = typeof e === "string" && e.startsWith("send_not_submitted");
        setErr(pasted ? errorText(e) : t("conv.sendFailed", { err: errorText(e) }));
      })
      .finally(() => { sendingRef.current = false; setSending(false); });
  };

  /** Runs a menu entry; a picker command also shows the Terminal tab. */
  const pick = (e: SlashEntry) => {
    send(`/${e.name}`);
    if (e.terminal) onShowTerminal();
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    // keyCode 229: WebKitGTK can send the Enter that commits an IME word with isComposing false.
    const ime = e.nativeEvent.isComposing || e.nativeEvent.keyCode === 229;
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
    if (e.key !== "Enter" || e.shiftKey || ime) return;
    e.preventDefault();
    send();
  };

  return (
    <div className="conv-composer">
      {menuOpen && <SlashMenu entries={entries} active={Math.min(active, entries.length - 1)} onPick={pick} onHover={setActive} />}
      <div className="conv-composer-row">
        <textarea ref={box} value={text} rows={2} disabled={waiting} placeholder={t("conv.placeholder")}
          onChange={(e) => { setText(e.target.value); setErr(null); setClosedFor(null); }}
          onKeyDown={onKeyDown} />
        <button type="button" className="btn" disabled={waiting || sending || text.trim() === ""} onClick={() => send()}>
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
