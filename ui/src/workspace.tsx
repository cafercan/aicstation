/**
 * Split terminal area, window-level shortcuts and the palette / help
 * overlays, kept out of App.tsx so App only wires state through.
 */
import { useEffect, useRef, type Dispatch, type SetStateAction } from "react";
import type { SessionCard } from "./api";
import type { View } from "./components/Sidebar";
import CommandPalette, { type PaletteItem } from "./components/CommandPalette";
import ShortcutsHelp, { ACTION_LABEL } from "./components/ShortcutsHelp";
import { SplitDivider, TermBar } from "./components/SplitControls";
import TerminalView from "./pages/TerminalView";
import { t } from "./i18n";
import { focusSearch } from "./shortcuts/bus";
import { KEYMAP, cycle, matchChord, type Binding } from "./shortcuts/keymap";
import { inSplit, openSplit, paneLayout, splitAfterClose, type Split } from "./terminal/split";
import { sessionsIn, type TerminalEntry } from "./terminal/store";

export type Overlay = "palette" | "help" | null;

export const termOf = (v: View): number | null => (typeof v === "object" && "terminal" in v ? v.terminal : null);

/** Switches to terminal `id` unless it already is the view (no re-render loop on focus events). */
export const showTerminal = (setView: Dispatch<SetStateAction<View>>, id: number) =>
  setView((v) => (termOf(v) === id ? v : { terminal: id }));

interface AreaProps {
  terminals: TerminalEntry[];
  /** Live Claude sessions: each terminal running one shows its bar. */
  sessions: SessionCard[];
  labels: Map<number, string>;
  termId: number | null;
  split: Split | null;
  setSplit: Dispatch<SetStateAction<Split | null>>;
  setView: Dispatch<SetStateAction<View>>;
  /** Opens a new terminal and resolves with its id. */
  newTerminal: () => Promise<number>;
}

/** Splits the current terminal with `right` (an open terminal or a new one). */
export function splitWith(p: AreaProps, right: number | "new"): void {
  const left = p.termId;
  if (left === null) return;
  const apply = (id: number) => {
    const s = openSplit(left, id);
    if (!s) return;
    p.setSplit(s);
    showTerminal(p.setView, id);
  };
  if (right === "new") p.newTerminal().then(apply).catch(() => {});
  else apply(right);
}

/** Toolbar, the visible terminal(s) and the divider. Every TerminalView stays mounted. */
export function TerminalArea(p: AreaProps) {
  const { terminals, termId, split } = p;
  const shown = termId !== null && terminals.some((x) => x.id === termId);
  const panes = shown ? paneLayout(split, termId) : [];
  const splitShown = panes.length === 2;
  const box = new Map(panes.map((x) => [x.id, x]));
  return (
    <>
      {shown && (
        <TermBar splitShown={splitShown}
          others={terminals.filter((x) => x.id !== termId).map((x) => ({ id: x.id, label: p.labels.get(x.id) ?? "" }))}
          onSplit={(r) => splitWith(p, r)} onJoin={() => p.setSplit(null)} />
      )}
      <div className="term-area" style={{ display: shown ? "block" : "none" }}>
        {terminals.map((x) => {
          const b = box.get(x.id);
          return (
            <TerminalView key={x.id} entry={x} visible={!!b} active={b?.active ?? false}
              session={sessionsIn(x, p.sessions)[0]}
              pane={splitShown && b ? { left: b.left, width: b.width } : undefined}
              onActivate={splitShown && b && !b.active ? () => showTerminal(p.setView, x.id) : undefined} />
          );
        })}
        {splitShown && split && (
          <SplitDivider ratio={split.ratio} onRatio={(ratio) => p.setSplit((s) => (s ? { ...s, ratio } : s))} />
        )}
      </div>
    </>
  );
}

/**
 * Keeps the split consistent with the open terminals: when a pane's terminal
 * is gone the split ends and the other pane is shown if it was the view.
 */
export function useSplitCleanup(
  terminals: TerminalEntry[], split: Split | null, setSplit: Dispatch<SetStateAction<Split | null>>,
  setView: Dispatch<SetStateAction<View>>,
): void {
  useEffect(() => {
    if (!split) return;
    const ids = new Set(terminals.map((x) => x.id));
    const gone = [split.left, split.right].find((id) => !ids.has(id));
    if (gone === undefined) return;
    const { show } = splitAfterClose(split, gone);
    setSplit(null);
    if (show !== null && ids.has(show)) setView((v) => (termOf(v) === gone ? { terminal: show } : v));
  }, [terminals, split, setSplit, setView]);
}

export interface ShortcutCtx extends AreaProps {
  overlay: Overlay;
  setOverlay: Dispatch<SetStateAction<Overlay>>;
  onNew: () => void;
  onClose: (id: number) => void;
}

/** Runs a binding; false when it does not apply here (the key then passes through). */
export function runBinding(b: Binding, c: ShortcutCtx): boolean {
  const ids = c.terminals.map((x) => x.id);
  switch (b.id) {
    case "newTerminal": c.onNew(); return true;
    case "closeTerminal":
      if (c.termId === null) return false;
      c.onClose(c.termId);
      return true;
    case "switchTerminal": {
      const id = ids[(b.n ?? 0) - 1];
      if (id === undefined) return false;
      showTerminal(c.setView, id);
      return true;
    }
    case "nextTerminal": case "prevTerminal": {
      const id = cycle(ids, c.termId, b.id === "nextTerminal" ? 1 : -1);
      if (id === null) return false;
      showTerminal(c.setView, id);
      return true;
    }
    case "dashboard": c.setView("dashboard"); return true;
    case "history": c.setView("history"); return true;
    case "focusSearch": return focusSearch();
    case "palette": c.setOverlay((o) => (o === "palette" ? null : "palette")); return true;
    case "shortcuts": c.setOverlay((o) => (o === "help" ? null : "help")); return true;
    case "toggleSplit": {
      if (c.termId === null) return false;
      if (inSplit(c.split, c.termId)) { c.setSplit(null); return true; }
      const other = ids.find((id) => id !== c.termId);
      splitWith(c, other ?? "new");
      return true;
    }
  }
}

/**
 * Window-level chords, in the capture phase so xterm never sees them. Any
 * other key is left alone and reaches the focused element (and the shell).
 */
export function useShortcuts(ctx: ShortcutCtx): void {
  const ref = useRef(ctx);
  ref.current = ctx;
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      const b = matchChord({
        type: e.type, key: e.key, code: e.code, ctrlKey: e.ctrlKey, shiftKey: e.shiftKey, altKey: e.altKey,
        metaKey: e.metaKey, altGraph: typeof e.getModifierState === "function" && e.getModifierState("AltGraph"),
      });
      if (!b || !runBinding(b, ref.current)) return;
      e.preventDefault();
      e.stopPropagation();
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, []);
}

/** Palette entries: actions (with their chord), open sessions, terminals. */
export function paletteItems(c: ShortcutCtx): PaletteItem[] {
  const actions: PaletteItem[] = KEYMAP
    .filter((b) => b.id !== "switchTerminal" && b.id !== "palette")
    .map((b) => ({
      key: `a-${b.id}`, label: t(ACTION_LABEL[b.id]), detail: b.chord, kind: "action" as const,
      run: () => { runBinding(b, c); },
    }));
  // Settings has no chord; the palette is its keyboard route.
  actions.push({ key: "a-settings", label: t("nav.settings"), kind: "action", run: () => c.setView("settings") });
  const sessions: PaletteItem[] = c.sessions.map((s) => ({
    key: `s-${s.sessionId}`, label: s.name, detail: s.cwd, kind: "session" as const,
    run: () => c.setView({ session: s.sessionId }),
  }));
  const terms: PaletteItem[] = c.terminals.map((x, i) => ({
    key: `t-${x.id}`, label: c.labels.get(x.id) ?? "", detail: i < 9 ? `Ctrl+Alt+${i + 1}` : undefined,
    kind: "terminal" as const, run: () => showTerminal(c.setView, x.id),
  }));
  return [...actions, ...sessions, ...terms];
}

/** The open overlay; focus returns to where it was when it closes. */
export function Overlays(c: ShortcutCtx) {
  const before = useRef<Element | null>(null);
  useEffect(() => {
    if (c.overlay) {
      before.current ??= document.activeElement;
      return;
    }
    const el = before.current as HTMLElement | null;
    before.current = null;
    // Only when focus was lost with the overlay (Esc); an action may have moved it on purpose.
    const lost = !document.activeElement || document.activeElement === document.body;
    if (lost && el && typeof el.focus === "function" && el.isConnected) el.focus();
  }, [c.overlay]);
  const close = () => c.setOverlay(null);
  if (c.overlay === "palette") return <CommandPalette items={paletteItems(c)} onClose={close} />;
  if (c.overlay === "help") return <ShortcutsHelp onClose={close} />;
  return null;
}
