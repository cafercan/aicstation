import { useEffect, useRef, useState } from "react";
import { getSessionDetail, onSnapshot, openFolder, type AgentNode, type SessionDetail, type ToolCall } from "../api";
import { fmtContext, fmtCost, fmtElapsed, fmtTokens, statusColor, statusKind, statusLabel } from "../format";
import ClaudeMark from "../components/ClaudeMark";
import { terminalForPid } from "../terminal/store";
import { goToTab, gotoTarget } from "../gotoTab";

interface Props {
  sessionId: string;
  onBack: () => void;
  onTerminal?: (id: number) => void;
  /** The session is in the current snapshot (its process runs). */
  live?: boolean;
  /** "▶ Devam et" on a closed session; a rejection is shown inline. */
  onResume?: (sessionId: string, name: string) => Promise<unknown>;
}

const MAIN = "__main__";

function hhmmss(at: string): string {
  const d = new Date(at);
  if (isNaN(d.getTime())) return "";
  return d.toLocaleTimeString("tr-TR", { hour12: false });
}

function nodeLabel(n: AgentNode): string {
  switch (n.kind.kind) {
    case "workflow": return `Workflow ${n.kind.id}`;
    case "phase": return `Aşama ${n.kind.name}`;
    default: return n.agentType ?? "ajan";
  }
}

function nodeAge(n: AgentNode): string {
  const a = n.stats.firstAt ? Date.parse(n.stats.firstAt) : NaN;
  const b = n.stats.lastAt ? Date.parse(n.stats.lastAt) : NaN;
  if (isNaN(a) || isNaN(b)) return "";
  return fmtElapsed(b - a);
}

function findNode(nodes: AgentNode[], id: string): AgentNode | null {
  for (const n of nodes) {
    if (n.id === id) return n;
    const c = findNode(n.children, id);
    if (c) return c;
  }
  return null;
}

function TreeRows({ nodes, depth, sel, onSel }: {
  nodes: AgentNode[]; depth: number; sel: string; onSel: (id: string) => void;
}) {
  return (
    <>
      {nodes.map((n) => (
        <div key={n.id}>
          <div className={n.id === sel ? "node sel" : "node"} style={{ paddingLeft: depth * 14 }}
            data-testid={`node-${n.id}`} onClick={() => onSel(n.id)}>
            {n.state === "running" ? <span className="g">●</span> : <span className="x">✓</span>}{" "}
            {nodeLabel(n)}
            {n.description ? ` · "${n.description}"` : ""}{" "}
            <span className="x">{nodeAge(n)}</span>
          </div>
          <div><TreeRows nodes={n.children} depth={depth + 1} sel={sel} onSel={onSel} /></div>
        </div>
      ))}
    </>
  );
}

function ToolResult({ state }: { state: ToolCall["state"] }) {
  if (state === "Ok") return <span className="g">✓</span>;
  if (state === "Error") return <span className="r">✗</span>;
  return <span className="y">… çalışıyor</span>;
}

export default function SessionDetailPage({ sessionId, onBack, onTerminal, live = false, onResume }: Props) {
  const [detail, setDetail] = useState<SessionDetail | null | undefined>(undefined);
  const [sel, setSel] = useState(MAIN);
  const [folderErr, setFolderErr] = useState<string | null>(null);
  const [resumeErr, setResumeErr] = useState<string | null>(null);
  const [gotoErr, setGotoErr] = useState<string | null>(null);
  const [gotoNote, setGotoNote] = useState<string | null>(null);
  const liveRef = useRef(live);
  /** Re-opens loading after it was considered final, and loads once. */
  const wakeRef = useRef<() => void>(() => {});

  useEffect(() => {
    liveRef.current = live;
    if (live) wakeRef.current();
  }, [live]);

  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;
    // At most one request in flight; events arriving meanwhile cause a single
    // follow-up. A closed session (pid 0) cannot change, so it is loaded once,
    // but pid 0 is final only while the session is not in the current
    // snapshot: a live session answered as closed keeps being refreshed.
    let inFlight = false;
    let again = false;
    let done = false;
    let seq = 0;
    const load = () => {
      if (!alive || done) return;
      if (inFlight) { again = true; return; }
      inFlight = true;
      const my = ++seq;
      getSessionDetail(sessionId)
        .then((d) => {
          if (!alive || my !== seq) return; // stale
          setDetail(d);
          if (d && d.card.pid === 0 && !liveRef.current) done = true;
        })
        .catch(() => { if (alive && my === seq) setDetail((p) => p ?? null); })
        .finally(() => {
          inFlight = false;
          if (again) { again = false; load(); }
        });
    };
    wakeRef.current = () => { done = false; load(); };
    load();
    onSnapshot((s) => {
      liveRef.current = s.sessions.some((c) => c.sessionId === sessionId);
      if (liveRef.current) done = false;
      load();
    })
      .then((u) => { if (alive) unlisten = u; else u(); })
      .catch(() => {});
    return () => { alive = false; unlisten?.(); };
  }, [sessionId]);

  const back = <button className="btn" onClick={onBack}>← Geri</button>;
  if (detail === undefined) return <div className="empty"><p>Yükleniyor…</p></div>;
  if (detail === null) return <div className="empty"><p>Oturum bulunamadı.</p>{back}</div>;

  const { card, tree } = detail;
  const t = detail.tokens;
  const tokens = t.input + t.output + t.cacheRead + t.cacheCreation;
  const selNode = sel === MAIN ? null : findNode(tree.roots, sel);
  const calls = selNode ? selNode.stats.toolCalls : detail.mainToolCalls;
  const caption = selNode ? nodeLabel(selNode) : "ana oturum";
  const kind = statusKind(card.status, card.pid);
  const target = kind === "closed" ? null : gotoTarget(card, terminalForPid(card.parentPid));
  const started = new Date(card.startedAtMs).toLocaleTimeString("tr-TR", { hour: "2-digit", minute: "2-digit" });

  return (
    <div>
      <div className="hd">
        <h3>
          <span className="back" role="button" aria-label="Geri" onClick={onBack}>←</span>
          <ClaudeMark color={statusColor(kind)} size={16} />
          {card.name}{" "}
          <span className={`badge ${kind}`}>{statusLabel(card.status, card.pid)}</span>
        </h3>
        <div>
          {target && (
            <button className="btn" onClick={() => {
              setGotoErr(null);
              setGotoNote(null);
              goToTab(card.sessionId, target, onTerminal)
                .then(setGotoNote, (e: unknown) => setGotoErr(e instanceof Error ? e.message : String(e)));
            }}>Sekmeye git</button>
          )}
          {kind === "closed" && !live && onResume && (
            <button className="btn" onClick={() => {
              setResumeErr(null);
              onResume(card.sessionId, card.name).catch((e) => setResumeErr(`Devam edilemedi: ${String(e)}`));
            }}>▶ Devam et</button>
          )}
          <button className="btn" onClick={() => {
            setFolderErr(null);
            openFolder(card.cwd).catch((e) => setFolderErr(`Klasör açılamadı: ${String(e)}`));
          }}>Klasörü aç</button>
        </div>
      </div>
      {folderErr && <div className="meta r" role="alert">{folderErr}</div>}
      {resumeErr && <div className="meta r" role="alert">{resumeErr}</div>}
      {gotoErr && <div className="meta r" role="alert">{gotoErr}</div>}
      {gotoNote && <div className="meta" role="status">{gotoNote}</div>}
      <div className="meta">
        {card.cwd} · başladı {started} · son hareket {fmtElapsed(Date.now() - card.lastActivityMs)}
      </div>
      <div className="kpis six">
        <div className="kpi"><b>{card.model ?? "?"}</b><span>model</span></div>
        <div className="kpi"><b>{fmtContext(card.contextPct)}</b><span>bağlam</span></div>
        <div className="kpi"><b>{fmtTokens(tokens)}</b><span>token</span></div>
        <div className="kpi"><b>{fmtCost(card.costUsd)}</b><span>maliyet</span></div>
        <div className="kpi"><b>{tree.total} / {tree.finished}</b><span>ajan / biten</span></div>
        <div className="kpi"><b>{detail.toolCallCount}</b><span>tool çağrısı</span></div>
      </div>
      <div className="cols">
        <div className="tree">
          <div className="cap">AJAN AĞACI</div>
          <div className={sel === MAIN ? "node sel" : "node"} data-testid="node-main" onClick={() => setSel(MAIN)}>
            <span className="g">●</span> Ana oturum
          </div>
          <TreeRows nodes={tree.roots} depth={1} sel={sel} onSel={setSel} />
        </div>
        <div className="log">
          <div className="cap">SON TOOL ÇAĞRILARI · {caption}</div>
          {calls.length === 0 && <div className="x">çağrı yok</div>}
          {calls.map((c) => (
            <div key={c.id} data-testid="tool-row">
              <span className="x">{hhmmss(c.at)}</span> <span className="c2">{c.name}</span>{" "}
              {c.target} <ToolResult state={c.state} />
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
