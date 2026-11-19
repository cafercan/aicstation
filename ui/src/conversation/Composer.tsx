import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { sendToSession } from "../api";
import { errorText, useT } from "../i18n";
import { appendQuote } from "./quote";

/**
 * Message box under the conversation of a session in an app terminal. Enter
 * sends the text to that terminal (the backend re-checks everything), Shift+Enter
 * adds a line. While the session waits for an approval the box is disabled:
 * typing would answer the prompt. Each new `quote` (a new `seq`) is added
 * to the text, with the caret after it, to answer under.
 */
export default function Composer({ sessionId, status, onShowTerminal, onSent, quote }: {
  sessionId: string; status: string; onShowTerminal: () => void; onSent?: () => void;
  quote?: { text: string; seq: number };
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

  useLayoutEffect(() => {
    const el = box.current;
    if (!toEnd.current || !el) return;
    toEnd.current = false;
    el.focus();
    el.setSelectionRange(el.value.length, el.value.length);
    el.scrollTop = el.scrollHeight;
  }, [text]);

  const send = () => {
    if (waiting || sendingRef.current || text.trim() === "") return;
    sendingRef.current = true;
    setSending(true);
    setErr(null);
    const sent = text;
    sendToSession(sessionId, sent)
      .then(() => {
        // Cleared only when unchanged: text edited while the send was on its
        // way is the user's and stays as it is.
        setText((cur) => (cur === sent ? "" : cur));
        onSent?.();
      }, (e: unknown) => {
        // Pasted but not submitted: the message says the text sits in the terminal.
        const pasted = typeof e === "string" && e.startsWith("send_not_submitted");
        setErr(pasted ? errorText(e) : t("conv.sendFailed", { err: errorText(e) }));
      })
      .finally(() => { sendingRef.current = false; setSending(false); });
  };

  return (
    <div className="conv-composer">
      <div className="conv-composer-row">
        <textarea ref={box} value={text} rows={2} disabled={waiting} placeholder={t("conv.placeholder")}
          onChange={(e) => { setText(e.target.value); setErr(null); }}
          onKeyDown={(e) => {
            // keyCode 229: WebKitGTK can send the Enter that commits an IME word with isComposing false.
            if (e.key !== "Enter" || e.shiftKey || e.nativeEvent.isComposing || e.nativeEvent.keyCode === 229) return;
            e.preventDefault();
            send();
          }} />
        <button type="button" className="btn" disabled={waiting || sending || text.trim() === ""} onClick={send}>
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
