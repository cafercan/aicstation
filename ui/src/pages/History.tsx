import { useEffect, useState } from "react";
import { getHistory, type HistoryRow, type HistoryView } from "../api";
import { fmtCost, fmtElapsed } from "../format";
import { resumeError } from "../gotoTab";
import { locale, useT } from "../i18n";

interface Props {
  onOpen: (sessionId: string) => void;
  /** "▶ Resume": new terminal resuming the session; a rejection is shown on the row. */
  onResume?: (sessionId: string, name: string) => Promise<unknown>;
  /** "Go to tab" when the session still runs in the app terminal that resumed it. */
  onTerminal?: (id: number) => void;
}

const rowCwd = (r: HistoryRow) => r.summary?.cwd ?? r.cwd ?? "";

export default function History({ onOpen, onResume, onTerminal }: Props) {
  const t = useT();
  const [view, setView] = useState<HistoryView | null>(null);
  const [rowErr, setRowErr] = useState<{ id: string; msg: string; terminal: number | null } | null>(null);
  /** Rows whose resume call is still pending (button disabled). */
  const [pending, setPending] = useState<Set<string>>(() => new Set());
  const [folder, setFolder] = useState("");
  const indexing = view?.indexing ?? false;

  useEffect(() => {
    let alive = true;
    const load = () => getHistory().then((h) => alive && setView(h)).catch(() => {});
    load();
    if (!indexing) return () => { alive = false; };
    const t = setInterval(load, 2000);
    return () => { alive = false; clearInterval(t); };
  }, [indexing]);

  if (!view) return <div className="empty"><p>{t("common.loading")}</p></div>;

  const folders = [...new Set(view.rows.map(rowCwd).filter((c) => c))].sort();
  const rows = view.rows.filter((r) => !folder || rowCwd(r) === folder);

  return (
    <div>
      <div className="hd">
        <h3>{t("hist.title")}</h3>
        <select aria-label={t("hist.folderFilter")} value={folder} onChange={(e) => setFolder(e.target.value)}>
          <option value="">{t("hist.allFolders")}</option>
          {folders.map((f) => <option key={f} value={f}>{f}</option>)}
        </select>
      </div>
      {view.indexing && <div className="meta">{t("hist.indexing")}</div>}
      {rows.length === 0 ? (
        <p className="empty">{t("hist.empty")}</p>
      ) : (
        <table className="hist">
          <thead>
            <tr>
              <th>{t("hist.colDate")}</th><th>{t("hist.colName")}</th><th>{t("hist.colFolder")}</th>
              <th>{t("hist.colModel")}</th><th>{t("hist.colDuration")}</th><th>{t("hist.colAgents")}</th>
              <th>{t("hist.colCost")}</th><th />
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => {
              const s = r.summary;
              const name = s?.name ?? r.name;
              return (
                <tr key={r.sessionId} onClick={() => onOpen(r.sessionId)}>
                  <td>{new Date(r.modifiedMs).toLocaleString(locale())}</td>
                  <td>{name}</td>
                  <td title={rowCwd(r)}>{rowCwd(r) || "?"}</td>
                  <td>{s ? s.model ?? "?" : "?"}</td>
                  <td>{s ? fmtElapsed(s.lastAtMs - s.firstAtMs) : "?"}</td>
                  <td>{s ? s.agentsTotal : "?"}</td>
                  <td>{s ? fmtCost(s.costUsd) : "?"}</td>
                  <td>
                    {onResume && (
                      <button className="btn" disabled={pending.has(r.sessionId)} onClick={(e) => {
                        e.stopPropagation();
                        if (pending.has(r.sessionId)) return;
                        setRowErr(null);
                        setPending((p) => new Set(p).add(r.sessionId));
                        onResume(r.sessionId, name)
                          .catch((err) => {
                            const e = resumeError(err);
                            setRowErr({ id: r.sessionId, msg: e.text, terminal: e.terminal });
                          })
                          .finally(() => setPending((p) => { const n = new Set(p); n.delete(r.sessionId); return n; }));
                      }}>{t("hist.resume")}</button>
                    )}
                    {rowErr?.id === r.sessionId && (
                      <div className="r" role="alert">
                        {rowErr.msg}
                        {rowErr.terminal !== null && onTerminal && (
                          <>{" "}<button className="btn" onClick={(e) => {
                            e.stopPropagation();
                            onTerminal(rowErr.terminal!);
                          }}>{t("detail.goto")}</button></>
                        )}
                      </div>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
    </div>
  );
}
