import { Fragment, type ReactNode } from "react";
import type { SessionCard } from "../api";
import { fmtContext, fmtEstCost, fmtTokens, folderName, usageSum } from "../format";
import { useT, type Key } from "../i18n";
import { fmtReset, limitView, meter, shortModel, useClaudeStatus, useGitDirty } from "./claudeBar";

interface Props {
  session: SessionCard;
  /** Current time (ms); passed in by tests. */
  now?: number;
}

/**
 * One line under a terminal running Claude Code: model and effort, folder
 * and branch, context, the 5-hour and weekly limits (with the status line
 * bridge on), tokens and cost, lines changed. Unknown parts are left out.
 */
export default function ClaudeBar({ session: c, now = Date.now() }: Props) {
  const t = useT();
  const status = useClaudeStatus();
  const dirty = useGitDirty(c.cwd);
  const line = status?.sessions[c.sessionId];
  const parts: [string, ReactNode][] = [];

  const model = shortModel(c.model);
  const effort = line?.effort ?? null;
  if (model || effort) {
    parts.push(["model", <>
      {model && <span className="c2">{model}</span>}
      {model && effort && " "}
      {effort && <span className="cbar-effort">{effort}</span>}
    </>]);
  }
  const folder = folderName(c.cwd);
  if (folder || c.gitBranch) {
    parts.push(["folder", <>
      {folder}
      {c.gitBranch && <>{folder && " "}<span className="g">({c.gitBranch}{dirty && <span title={t("cbar.dirty")}>*</span>})</span></>}
    </>]);
  }
  if (c.contextPct !== null) parts.push(["ctx", t("sb.context", { pct: fmtContext(c.contextPct) })]);

  const windows: [string, Key, Key][] = [["fiveHour", "cbar.fiveHour", "cbar.fiveHourTitle"], ["sevenDay", "cbar.sevenDay", "cbar.sevenDayTitle"]];
  for (const [key, label, title] of windows) {
    const v = status?.bridge ? limitView(status.limits?.[key as "fiveHour" | "sevenDay"], now) : null;
    if (!v) continue;
    const reset = v.resetIn !== null ? fmtReset(v.resetIn) : null;
    parts.push([key, (
      <span className={`cbar-limit ${v.level}`} data-testid={`cbar-${key}`}
        title={t(title, { pct: v.left, reset: reset ?? "?" })}>
        {t(label)} <span className="cbar-meter">{meter(v.filled)}</span> {v.left}%
        {reset && <span className="cbar-reset"> ⟳ {reset}</span>}
      </span>
    )]);
  }

  const tokens = usageSum(c.tokens);
  const est = fmtEstCost(c.costUsd);
  if (tokens > 0 || est) {
    parts.push(["tokens", <>{tokens > 0 && t("cbar.tokens", { n: fmtTokens(tokens) })}{tokens > 0 && est && " "}{est && <span className="est">{est}</span>}</>]);
  }
  if (line && line.linesAdded !== null && line.linesRemoved !== null) {
    parts.push(["lines", <span title={t("cbar.lines")}><span className="g">+{line.linesAdded}</span>/<span className="cbar-del">-{line.linesRemoved}</span></span>]);
  }

  return (
    <div className="cbar" role="status" aria-label={t("cbar.label")} data-testid="claude-bar">
      {parts.map(([k, node], i) => <Fragment key={k}>{i > 0 && <span className="cbar-sep"> │ </span>}{node}</Fragment>)}
    </div>
  );
}
