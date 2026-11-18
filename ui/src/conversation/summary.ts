import type { ConvStep } from "../api";
import { locale, type useT } from "../i18n";

export type TFn = ReturnType<typeof useT>;

const EDIT = new Set(["Edit", "Write", "MultiEdit", "NotebookEdit"]);
const SEARCH = new Set(["Grep", "Glob"]);

/** One line for a run of tool steps: counts per kind, first-seen order. */
export function stepSummary(steps: ConvStep[], t: TFn, loc: string = locale()): string {
  const order: string[] = [];
  const groups = new Map<string, ConvStep[]>();
  for (const s of steps) {
    const id = s.tool === "Bash" ? "bash" : s.tool === "Read" ? "read" : EDIT.has(s.tool) ? "edit"
      : SEARCH.has(s.tool) ? "search" : `tool:${s.tool}`;
    if (!groups.has(id)) { groups.set(id, []); order.push(id); }
    groups.get(id)!.push(s);
  }
  const parts = order.map((id) => {
    const g = groups.get(id)!;
    switch (id) {
      case "bash": return t("conv.step.commands", { n: g.length });
      case "read": return g.length === 1 ? t("conv.step.readOne", { name: g[0].target }) : t("conv.step.readMany", { n: g.length });
      case "edit": return g.length === 1 ? t("conv.step.editOne", { name: g[0].target }) : t("conv.step.editMany", { n: g.length });
      case "search": return t("conv.step.searched");
      default: return t("conv.step.used", { tool: g[0].tool });
    }
  });
  const text = parts.join(", ");
  // A leading file name keeps its case ("grid.jpg okundu").
  const first = groups.get(order[0]);
  if (first && first.length === 1 && first[0].target && text.startsWith(first[0].target)) return text;
  return text.charAt(0).toLocaleUpperCase(loc) + text.slice(1);
}
