import { useRef, useState } from "react";
import { sendToSession } from "../api";
import { errorText, useT } from "../i18n";

/**
 * Message box under the conversation of a session in an app terminal. Enter
 * sends the text to that terminal (the backend re-checks everything), Shift+Enter
 * adds a line. While the session waits for an approval the box is disabled:
 * typing would answer the prompt.
 */
export default function Composer({ sessionId, status, onShowTerminal, onSent }: {
  sessionId: string; status: string; onShowTerminal: () => void; onSent?: () => void;
}) {
  const t = useT();
  const [text, setText] = useState("");
  const [sending, setSending] = useState(false);
  const sendingRef = useRef(false);
  const [err, setErr] = useState<string | null>(null);
  const waiting = status === "waiting";

  const send = () => {
    if (waiting || sendingRef.current || text.trim() === "") return;
    sendingRef.current = true;
    setSending(true);
    setErr(null);
    const sent = text;
    sendToSession(sessionId, sent)
      .then(() => {
        // Keep only what was typed while the send was on its way.
        setText((cur) => (cur.startsWith(sent) ? cur.slice(sent.length) : cur));
        onSent?.();
      }, (e: unknown) => setErr(t("conv.sendFailed", { err: errorText(e) })))
      .finally(() => { sendingRef.current = false; setSending(false); });
  };

  return (
    <div className="conv-composer">
      <div className="conv-composer-row">
        <textarea value={text} rows={2} disabled={waiting} placeholder={t("conv.placeholder")}
          onChange={(e) => setText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key !== "Enter" || e.shiftKey || e.nativeEvent.isComposing) return;
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
