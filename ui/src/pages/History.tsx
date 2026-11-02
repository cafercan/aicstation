import { useEffect, useState } from "react";
import { getHistory, type HistoryRow, type HistoryView } from "../api";
import { fmtCost, fmtElapsed } from "../format";

interface Props {
  onOpen: (sessionId: string) => void;
  /** "▶ Devam et": new terminal resuming the session; a rejection is shown on the row. */
  onResume?: (sessionId: string, name: string) => Promise<unknown>;
}

const rowCwd = (r: HistoryRow) => r.summary?.cwd ?? r.cwd ?? "";

export default function History({ onOpen, onResume }: Props) {
  const [view, setView] = useState<HistoryView | null>(null);
  const [rowErr, setRowErr] = useState<{ id: string; msg: string } | null>(null);
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

  if (!view) return <div className="empty"><p>Yükleniyor…</p></div>;

  const folders = [...new Set(view.rows.map(rowCwd).filter((c) => c))].sort();
  const rows = view.rows.filter((r) => !folder || rowCwd(r) === folder);

  return (
    <div>
      <div className="hd">
        <h3>Geçmiş</h3>
        <select aria-label="Klasör filtresi" value={folder} onChange={(e) => setFolder(e.target.value)}>
          <option value="">Tüm klasörler</option>
          {folders.map((f) => <option key={f} value={f}>{f}</option>)}
        </select>
      </div>
      {view.indexing && <div className="meta">Dizinleniyor…</div>}
      {rows.length === 0 ? (
        <p className="empty">Kayıt yok.</p>
      ) : (
        <table className="hist">
          <thead>
            <tr><th>Tarih</th><th>Ad</th><th>Klasör</th><th>Model</th><th>Süre</th><th>Ajan</th><th>Maliyet</th><th /></tr>
          </thead>
          <tbody>
            {rows.map((r) => {
              const s = r.summary;
              const name = s?.name ?? r.name;
              return (
                <tr key={r.sessionId} onClick={() => onOpen(r.sessionId)}>
                  <td>{new Date(r.modifiedMs).toLocaleString("tr-TR")}</td>
                  <td>{name}</td>
                  <td title={rowCwd(r)}>{rowCwd(r) || "?"}</td>
                  <td>{s ? s.model ?? "?" : "?"}</td>
                  <td>{s ? fmtElapsed(s.lastAtMs - s.firstAtMs) : "?"}</td>
                  <td>{s ? s.agentsTotal : "?"}</td>
                  <td>{s ? fmtCost(s.costUsd) : "?"}</td>
                  <td>
                    {onResume && (
                      <button className="btn" onClick={(e) => {
                        e.stopPropagation();
                        setRowErr(null);
                        onResume(r.sessionId, name).catch((err) => setRowErr({ id: r.sessionId, msg: String(err) }));
                      }}>▶ Devam et</button>
                    )}
                    {rowErr?.id === r.sessionId && <div className="r" role="alert">{rowErr.msg}</div>}
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
