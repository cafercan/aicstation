import { useState, type KeyboardEvent, type MouseEvent } from "react";
import type { HistoryView } from "../api";
import {
  CHART_TOKENS, RANGES, dailyCosts, modelSplit, rangeDays, topProjects, totals, withLive, type DayBucket,
  type RangeDays,
} from "../costs";
import { fmtCost } from "../format";
import { locale, useT } from "../i18n";
import { createStore } from "../prefs";
import { cssVar } from "../theme";

/** Chosen range; kept in memory for the app session only. */
const rangeStore = createStore<RangeDays>(30);

const H = 120;
const W = 600;

function dateLabel(date: string): string {
  const [y, m, d] = date.split("-").map(Number);
  return new Date(y, m - 1, d).toLocaleDateString(locale(), { day: "numeric", month: "short", year: "numeric" });
}

function DailyChart({ buckets }: { buckets: DayBucket[] }) {
  const t = useT();
  const [active, setActive] = useState<number | null>(null);
  const max = Math.max(...buckets.map((b) => b.cost), 0);
  const n = buckets.length;
  const bw = W / n;
  const gap = n > 40 ? 1 : 2;
  const pick = (e: MouseEvent<SVGSVGElement>) => {
    const r = e.currentTarget.getBoundingClientRect();
    if (r.width <= 0) return;
    setActive(Math.min(n - 1, Math.max(0, Math.floor(((e.clientX - r.left) / r.width) * n))));
  };
  const onKey = (e: KeyboardEvent<SVGSVGElement>) => {
    const cur = active ?? n - 1;
    const next = e.key === "ArrowLeft" ? cur - 1 : e.key === "ArrowRight" ? cur + 1
      : e.key === "Home" ? 0 : e.key === "End" ? n - 1 : null;
    if (next === null) return;
    e.preventDefault();
    setActive(Math.min(n - 1, Math.max(0, next)));
  };
  const tip = active !== null ? buckets[active] : null;
  const tipText = tip
    ? `${dateLabel(tip.date)}${tip.today ? ` (${t("cost.today")})` : ""}: ${fmtCost(tip.cost)}`
    : "";
  return (
    <div className="cost-daily">
      <div className="cap">{t("cost.daily")}</div>
      <div className="cost-plot">
        <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" width="100%" height={H} tabIndex={0}
          role="img" aria-label={t("cost.dailyLabel", { n, max: fmtCost(max) })}
          onMouseMove={pick} onMouseLeave={() => setActive(null)} onKeyDown={onKey}
          onFocus={() => setActive((a) => a ?? n - 1)} onBlur={() => setActive(null)}>
          <line x1="0" y1={H - 0.5} x2={W} y2={H - 0.5} style={{ stroke: "var(--border-strong)" }} strokeWidth="1" />
          {buckets.map((b, i) => {
            const h = max > 0 ? Math.max(b.cost > 0 ? 2 : 0, (b.cost / max) * (H - 6)) : 0;
            const tok = b.today ? CHART_TOKENS.today : CHART_TOKENS.bar;
            return (
              <rect key={b.date} data-date={b.date} data-today={b.today || undefined}
                x={i * bw + gap / 2} y={H - 1 - h} width={Math.max(1, bw - gap)} height={h}
                style={{ fill: `var(${cssVar(tok)})`, opacity: active === null || active === i ? 1 : 0.55 }}>
                <title>{`${dateLabel(b.date)}: ${fmtCost(b.cost)}`}</title>
              </rect>
            );
          })}
        </svg>
        {tip && (
          <div className="cost-tip" style={{ left: `${((active! + 0.5) / n) * 100}%` }}>{tipText}</div>
        )}
      </div>
      <div className="cost-axis">
        <span>{dateLabel(buckets[0].date)}</span>
        <span>{t("cost.max", { cost: fmtCost(max) })}</span>
        <span>{t("cost.today")}</span>
      </div>
      <div className="sr-only" role="status" aria-live="polite">{tipText}</div>
    </div>
  );
}

function TopProjects({ list }: { list: ReturnType<typeof topProjects> }) {
  const t = useT();
  const max = list[0]?.cost ?? 0;
  return (
    <div className="cost-top">
      <div className="cap">{t("cost.topProjects")}</div>
      {list.length === 0 ? <div className="meta">{t("cost.noData")}</div> : (
        <ol className="hbars">
          {list.map((p) => (
            <li key={p.cwd} title={p.cwd}>
              <span className="lbl">{p.name}</span>
              <span className="bar"><i style={{ width: `${max > 0 ? (p.cost / max) * 100 : 0}%`,
                background: `var(${cssVar(CHART_TOKENS.bar)})` }} /></span>
              <b>{fmtCost(p.cost)}</b>
            </li>
          ))}
        </ol>
      )}
    </div>
  );
}

function ModelSplit({ list }: { list: ReturnType<typeof modelSplit> }) {
  const t = useT();
  const color = (i: number, other?: boolean) => {
    const s = CHART_TOKENS.series;
    return `var(${cssVar(other ? s[s.length - 1] : s[Math.min(i, s.length - 1)])})`;
  };
  const name = (m: (typeof list)[number]) => m.other ? t("cost.otherModels") : m.model ?? t("cost.unknownModel");
  let x = 0;
  return (
    <div className="cost-models">
      <div className="cap">{t("cost.models")}</div>
      {list.length === 0 ? <div className="meta">{t("cost.noData")}</div> : (
        <>
          <svg viewBox="0 0 100 10" preserveAspectRatio="none" width="100%" height="14" role="img"
            aria-label={list.map((m) => `${name(m)} ${Math.round(m.share * 100)}%`).join(", ")}>
            {list.map((m, i) => {
              const w = m.share * 100;
              const el = <rect key={`${m.model}-${i}`} data-model={m.model ?? ""} x={x} y={0} width={w} height={10}
                style={{ fill: color(i, m.other) }}><title>{`${name(m)}: ${fmtCost(m.cost)}`}</title></rect>;
              x += w;
              return el;
            })}
          </svg>
          <ul className="legend">
            {list.map((m, i) => (
              <li key={`${m.model}-${i}`}>
                <i style={{ background: color(i, m.other) }} />
                <span className="lbl" title={name(m)}>{name(m)}</span>
                <b>{Math.round(m.share * 100)}%</b>
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}

/** "Maliyet" section of the Geçmiş page: daily bars, top projects, model split, totals. */
export default function CostCharts({ view, now }: { view: HistoryView; now: number }) {
  const t = useT();
  const range = rangeStore.use();
  const dates = rangeDays(now, range);
  const days = new Set(dates);
  // Live sessions count in all three charts (the today bar's total has them too).
  const rows = withLive(view.rows, view.live);
  const buckets = dailyCosts(rows, dates, now, view.todayCost);
  const { total, avg } = totals(buckets);
  const computing = view.indexing || !view.todayComplete;
  return (
    <section className="costs" aria-label={t("cost.title")}>
      <div className="hd">
        <h3>{t("cost.title")} <span className="est">{t("cost.estimated")} · {t("cost.inclLive")}</span></h3>
        <div className="seg" role="group" aria-label={t("cost.range")}>
          {RANGES.map((r) => (
            <button key={r} type="button" className={r === range ? "on" : undefined} aria-pressed={r === range}
              onClick={() => rangeStore.set(r)}>{t("cost.days", { n: r })}</button>
          ))}
        </div>
      </div>
      {computing && <div className="meta" role="status">{t("cost.computing")}</div>}
      <div className="kpis cost-kpis">
        <div className="kpi"><b>{fmtCost(total)}</b><span>{t("cost.total")}</span></div>
        <div className="kpi"><b>{fmtCost(avg)}</b><span>{t("cost.avg")}</span></div>
      </div>
      <div className="cost-grid">
        <DailyChart buckets={buckets} />
        <TopProjects list={topProjects(rows, days)} />
        <ModelSplit list={modelSplit(rows, days)} />
      </div>
    </section>
  );
}
