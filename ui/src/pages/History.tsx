import { useEffect, useRef, useState, type FormEvent } from "react";
import { getHistory, searchHistory, type HistoryView, type SearchOutcome } from "../api";
import CostCharts, { resetCostRange } from "../components/CostCharts";
import { filterHistory, rowCwd, type HistoryFilter, type HistorySort } from "../filter";
import { fmtCost, fmtElapsed } from "../format";
import { resumeError } from "../gotoTab";
import { errorText, locale, useT } from "../i18n";
import { createStore } from "../prefs";
import { useSearchFocus } from "../shortcuts/bus";

interface Props {
  onOpen: (sessionId: string) => void;
  /** "▶ Resume": new terminal resuming the session; a rejection is shown on the row. */
  onResume?: (sessionId: string, name: string) => Promise<unknown>;
  /** "Go to tab" when the session still runs in the app terminal that resumed it. */
  onTerminal?: (id: number) => void;
}

/** Filters survive page switches for the app session only (memory, not storage). */
const filterStore = createStore<HistoryFilter & { folder: string }>({ query: "", from: "", to: "", sort: "date", folder: "" });

/** Latest content-search generation; a reply for an older one is dropped. */
let searchGen = 0;

function ContentSearch({ view, onOpen }: { view: HistoryView; onOpen: (id: string) => void }) {
  const t = useT();
  const [q, setQ] = useState("");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<SearchOutcome | null>(null);
  const [err, setErr] = useState<string | null>(null);
  // A search still running when the page goes away is superseded.
  useEffect(() => () => { searchGen++; }, []);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    const query = q.trim();
    setErr(null);
    if ([...query].length < 2 || [...query].length > 200) {
      setErr(errorText("search_query_invalid"));
      return;
    }
    const gen = ++searchGen;
    setBusy(true);
    searchHistory(query, gen)
      .then((out) => { if (gen === searchGen && !out.cancelled) setResult(out); })
      .catch((x) => { if (gen === searchGen) setErr(errorText(x)); })
      .finally(() => { if (gen === searchGen) setBusy(false); });
  };
  const clear = () => { searchGen++; setBusy(false); setResult(null); setErr(null); setQ(""); };
  const nameOf = (id: string) => {
    const r = view.rows.find((x) => x.sessionId === id);
    return r ? r.summary?.name ?? r.name : id.slice(0, 8);
  };

  return (
    <div className="content-search">
      <form className="filters" role="search" onSubmit={submit}>
        <input type="search" value={q} maxLength={200} aria-label={t("hist.contentSearch")}
          placeholder={t("hist.contentPlaceholder")} onChange={(e) => setQ(e.target.value)} />
        <button type="submit" className="btn" disabled={busy}>{t("hist.contentSearch")}</button>
        {(result || busy || err) && <button type="button" className="btn" onClick={clear}>{t("hist.contentClear")}</button>}
      </form>
      {busy && <div className="meta" role="status">{t("hist.contentSearching")}</div>}
      {err && <div className="r" role="alert">{err}</div>}
      {result && !busy && (
        result.hits.length === 0 ? <div className="meta">{t("hist.contentNone")}</div> : (
          <>
            <div className="meta">
              {t("hist.contentHits", { n: result.hits.length })}
              {result.truncated && <> · {t("hist.contentTruncated", { n: result.hits.length })}</>}
            </div>
            <ul className="hits">
              {result.hits.map((h, i) => (
                <li key={`${h.sessionId}-${i}`}>
                  <button type="button" onClick={() => onOpen(h.sessionId)}>
                    <span className="hit-hd">
                      <b>{nameOf(h.sessionId)}</b>
                      <span>{h.role === "user" ? t("hist.roleUser") : t("hist.roleAssistant")}</span>
                      {h.atMs > 0 && <span>{new Date(h.atMs).toLocaleString(locale())}</span>}
                    </span>
                    <span className="snip">{h.snippet.before}<mark>{h.snippet.hit}</mark>{h.snippet.after}</span>
                  </button>
                </li>
              ))}
            </ul>
          </>
        )
      )}
    </div>
  );
}

export default function History({ onOpen, onResume, onTerminal }: Props) {
  const t = useT();
  const [view, setView] = useState<HistoryView | null>(null);
  const [rowErr, setRowErr] = useState<{ id: string; msg: string; terminal: number | null } | null>(null);
  /** Rows whose resume call is still pending (button disabled). */
  const [pending, setPending] = useState<Set<string>>(() => new Set());
  const f = filterStore.use();
  const set = (p: Partial<typeof f>) => filterStore.set({ ...filterStore.get(), ...p });
  const searchRef = useRef<HTMLInputElement>(null);
  useSearchFocus(searchRef);
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
  const rows = filterHistory(view.rows.filter((r) => !f.folder || rowCwd(r) === f.folder), f);

  return (
    <div>
      <CostCharts view={view} now={Date.now()} />
      <div className="hd">
        <h3>{t("hist.title")}</h3>
        <select aria-label={t("hist.folderFilter")} value={f.folder} onChange={(e) => set({ folder: e.target.value })}>
          <option value="">{t("hist.allFolders")}</option>
          {folders.map((x) => <option key={x} value={x}>{x}</option>)}
        </select>
      </div>
      <div className="filters">
        <input ref={searchRef} type="search" value={f.query} aria-label={t("filter.searchLabel")}
          placeholder={t("filter.searchPlaceholder")} onChange={(e) => set({ query: e.target.value })} />
        <label>{t("hist.from")} <input type="date" value={f.from} max={f.to || undefined}
          onChange={(e) => set({ from: e.target.value })} /></label>
        <label>{t("hist.to")} <input type="date" value={f.to} min={f.from || undefined}
          onChange={(e) => set({ to: e.target.value })} /></label>
        <label>{t("hist.sort")} <select value={f.sort} onChange={(e) => set({ sort: e.target.value as HistorySort })}>
          <option value="date">{t("hist.sortDate")}</option>
          <option value="cost">{t("hist.sortCost")}</option>
        </select></label>
      </div>
      <ContentSearch view={view} onOpen={onOpen} />
      {view.indexing && <div className="meta">{t("hist.indexing")}</div>}
      {rows.length === 0 ? (
        <p className="empty">{view.rows.length === 0 ? t("hist.empty") : t("hist.noMatch")}</p>
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

/** Test helper: reset the in-memory filters and the cost range. */
export function resetHistoryFilters(): void {
  resetCostRange();
  filterStore.set({ query: "", from: "", to: "", sort: "date", folder: "" });
}
