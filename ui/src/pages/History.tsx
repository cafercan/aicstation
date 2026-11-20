import { pressable } from "../a11y";
import { useEffect, useRef, useState, type FormEvent } from "react";
import {
  cancelSearch, deleteSession, getHistory, openFolder, openSystemTerminalIn, searchHistory,
  type HistoryRow, type HistorySummary, type HistoryView, type SearchOutcome, type SessionCard,
} from "../api";
import { askConfirm } from "../components/ConfirmDialog";
import ContextMenu, { type MenuItem } from "../components/ContextMenu";
import CostCharts from "../components/CostCharts";
import { filterHistory, rowCwd, type HistoryFilter, type HistorySort } from "../filter";
import { fmtBreakdown, fmtElapsed, fmtEstCost, fmtTokens, usageSum } from "../format";
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
  /** The running sessions: a content-search hit in one shows its name. */
  live?: SessionCard[];
  /** "Open in terminal" from a row's menu: a new app terminal in the folder. */
  onOpenTerminalAt?: (cwd: string) => Promise<unknown>;
}

/** The keyboard's way to a row's menu: the Menu key or Shift+F10. */
const menuKey = (e: React.KeyboardEvent) => e.key === "ContextMenu" || (e.shiftKey && e.key === "F10");

const NO_LIVE: SessionCard[] = [];

/** Filters survive page switches for the app session only (memory, not storage). */
const filterStore = createStore<HistoryFilter & { folder: string }>({ query: "", from: "", to: "", sort: "date", folder: "" });

/** Latest content-search generation; a reply for an older one is dropped. */
let searchGen = 0;

/** How often the page reloads history (cost charts) while open and indexed. */
export const HISTORY_REFRESH_MS = 30_000;

function ContentSearch({ view, live, onOpen }: { view: HistoryView; live: SessionCard[]; onOpen: (id: string) => void }) {
  const t = useT();
  const [q, setQ] = useState("");
  const [busy, setBusy] = useState(false);
  const [result, setResult] = useState<SearchOutcome | null>(null);
  const [err, setErr] = useState<string | null>(null);
  /** Generation of the search in flight in the backend (it keeps reading until stopped), or null. */
  const running = useRef<number | null>(null);
  /** Stops the backend search and drops its late reply. Only that search:
   *  a cancel arriving after a new search started leaves the new one alone. */
  const stop = () => {
    searchGen++;
    if (running.current !== null) {
      cancelSearch(running.current).catch(() => {});
      running.current = null;
    }
  };
  // Leaving the page stops a search still running.
  useEffect(() => stop, []);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    const query = q.trim();
    setErr(null);
    if ([...query].length < 2 || [...query].length > 200) {
      setErr(errorText("search_query_invalid"));
      return;
    }
    // A new generation already stops the previous search in the backend (no
    // separate cancel_search here: it could arrive after this search starts
    // and stop it instead).
    const gen = ++searchGen;
    setBusy(true);
    running.current = gen;
    searchHistory(query, gen)
      .then((out) => { if (gen === searchGen && !out.cancelled) setResult(out); })
      .catch((x) => { if (gen === searchGen) setErr(errorText(x)); })
      .finally(() => { if (gen === searchGen) { setBusy(false); running.current = null; } });
  };
  const clear = () => { stop(); setBusy(false); setResult(null); setErr(null); setQ(""); };
  const nameOf = (id: string) => {
    const r = view.rows.find((x) => x.sessionId === id);
    if (r) return r.summary?.name ?? r.name;
    // History lists closed sessions; the search also reads the running ones.
    return live.find((c) => c.sessionId === id)?.name ?? id.slice(0, 8);
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
        result.hits.length === 0 ? (
          <div className="meta">
            {t("hist.contentNone")}
            {result.truncated && <> · {t("hist.contentTruncated")}</>}
          </div>
        ) : (
          <>
            <div className="meta">
              {t("hist.contentHits", { n: result.hits.length })}
              {result.truncated && <> · {t("hist.contentTruncated")}</>}
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

function HistoryTokens({ s }: { s: HistorySummary }) {
  const est = fmtEstCost(s.costUsd);
  return (
    <>
      <span title={fmtBreakdown(s.tokens)}>{fmtTokens(usageSum(s.tokens))}</span>
      {est && <span className="est">{est}</span>}
    </>
  );
}

export default function History({ onOpen, onResume, onTerminal, live = NO_LIVE, onOpenTerminalAt }: Props) {
  const t = useT();
  const [view, setView] = useState<HistoryView | null>(null);
  const [rowErr, setRowErr] = useState<{ id: string; msg: string; terminal: number | null } | null>(null);
  /** Rows whose resume call is still pending (button disabled). */
  const [pending, setPending] = useState<Set<string>>(() => new Set());
  /** A row's right-click menu: where, and the row it acts on. */
  const [menu, setMenu] = useState<{ x: number; y: number; row: HistoryRow } | null>(null);
  const f = filterStore.use();
  const set = (p: Partial<typeof f>) => filterStore.set({ ...filterStore.get(), ...p });
  const searchRef = useRef<HTMLInputElement>(null);
  useSearchFocus(searchRef);
  const indexing = view?.indexing ?? false;

  useEffect(() => {
    let alive = true;
    const load = () => getHistory().then((h) => alive && setView(h)).catch(() => {});
    load();
    // Fast while indexing; afterwards every 30 s so today's cost stays current.
    const t = setInterval(load, indexing ? 2000 : HISTORY_REFRESH_MS);
    return () => { alive = false; clearInterval(t); };
  }, [indexing]);

  if (!view) return <div className="empty"><p>{t("common.loading")}</p></div>;

  const rowError = (r: HistoryRow, msg: string) => setRowErr({ id: r.sessionId, msg, terminal: null });
  const resume = (r: HistoryRow, name: string) => {
    if (!onResume || pending.has(r.sessionId)) return;
    setRowErr(null);
    setPending((p) => new Set(p).add(r.sessionId));
    onResume(r.sessionId, name)
      .catch((err) => {
        const e = resumeError(err);
        setRowErr({ id: r.sessionId, msg: e.text, terminal: e.terminal });
      })
      .finally(() => setPending((p) => { const n = new Set(p); n.delete(r.sessionId); return n; }));
  };
  /** Moves the conversation to the recycle bin after asking; the list reloads without it. */
  const remove = async (r: HistoryRow, name: string) => {
    if (!(await askConfirm(t("detail.deleteConfirm", { name })))) return;
    setRowErr(null);
    deleteSession(r.sessionId)
      .then(() => getHistory().then(setView), (e) => rowError(r, t("detail.deleteFailed", { err: errorText(e) })))
      .catch(() => {});
  };
  const menuItems = (r: HistoryRow): MenuItem[] => {
    const name = r.summary?.name ?? r.name;
    const cwd = rowCwd(r);
    const items: MenuItem[] = [];
    if (onResume) items.push({ label: t("hist.resume"), disabled: pending.has(r.sessionId), onSelect: () => resume(r, name) });
    items.push({
      label: t("detail.openFolder"), disabled: !cwd,
      onSelect: () => { openFolder(cwd).catch((e) => rowError(r, t("detail.folderFailed", { err: errorText(e) }))); },
    });
    if (onOpenTerminalAt) {
      items.push({
        label: t("detail.openTerminal"), disabled: !cwd,
        onSelect: () => { onOpenTerminalAt(cwd).catch((e) => rowError(r, t("detail.terminalFailed", { err: errorText(e) }))); },
      });
    }
    items.push({
      label: t("detail.openSystemTerminal"), disabled: !cwd,
      onSelect: () => { openSystemTerminalIn(cwd).catch((e) => rowError(r, t("detail.terminalFailed", { err: errorText(e) }))); },
    });
    items.push({ label: t("detail.delete"), danger: true, separated: true, onSelect: () => { void remove(r, name); } });
    return items;
  };

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
          <option value="tokens">{t("hist.sortTokens")}</option>
        </select></label>
      </div>
      <ContentSearch view={view} live={live} onOpen={onOpen} />
      {view.indexing && <div className="meta">{t("hist.indexing")}</div>}
      {rows.length === 0 ? (
        <p className="empty">{view.rows.length === 0 ? t("hist.empty") : t("hist.noMatch")}</p>
      ) : (
        <table className="hist">
          <thead>
            <tr>
              <th>{t("hist.colDate")}</th><th>{t("hist.colName")}</th><th>{t("hist.colFolder")}</th>
              <th>{t("hist.colModel")}</th><th>{t("hist.colDuration")}</th><th>{t("hist.colAgents")}</th>
              <th>{t("hist.colTokens")}</th><th />
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => {
              const s = r.summary;
              const name = s?.name ?? r.name;
              const press = pressable(() => onOpen(r.sessionId), null);
              return (
                <tr key={r.sessionId} {...press}
                  onContextMenu={(e) => { e.preventDefault(); setMenu({ x: e.clientX, y: e.clientY, row: r }); }}
                  onKeyDown={(e) => {
                    if (e.target === e.currentTarget && menuKey(e)) {
                      e.preventDefault();
                      const box = e.currentTarget.getBoundingClientRect();
                      setMenu({ x: box.left + 24, y: box.bottom, row: r });
                      return;
                    }
                    press.onKeyDown(e);
                  }}>
                  <td>{new Date(r.modifiedMs).toLocaleString(locale())}</td>
                  <td>
                    {name}
                    {s?.gitBranch && <span className="branch" title={s.gitBranch}>⎇ {s.gitBranch}</span>}
                  </td>
                  <td title={rowCwd(r)}>{rowCwd(r) || "?"}</td>
                  <td>{s ? s.model ?? "?" : "?"}</td>
                  <td>{s ? fmtElapsed(s.lastAtMs - s.firstAtMs) : "?"}</td>
                  <td>{s ? s.agentsTotal : "?"}</td>
                  <td>{s ? <HistoryTokens s={s} /> : "?"}</td>
                  <td>
                    {onResume && (
                      <button className="btn" disabled={pending.has(r.sessionId)} onClick={(e) => {
                        e.stopPropagation();
                        resume(r, name);
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
      {menu && (
        <ContextMenu x={menu.x} y={menu.y} items={menuItems(menu.row)} label={t("hist.menu")} onClose={() => setMenu(null)} />
      )}
    </div>
  );
}
