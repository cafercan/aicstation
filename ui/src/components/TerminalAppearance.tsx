import { useT, type Key } from "../i18n";
import {
  CURSOR_STYLES, FONTS, FONT_WEIGHTS, resetAppearance, updateAppearance, useAppearance, validFontName,
  type Appearance, type CursorStyle, type FontChoice,
} from "../terminal/appearance";
import TerminalPreview from "./TerminalPreview";
import { useTheme } from "../theme";
import { MATCH } from "../terminal/match";
import { ANSI_KEYS, SCHEMES, SCHEME_IDS, type SchemeId } from "../terminal/schemes";

const SCROLLBACK = [1000, 5000, 10000, 50000, 100000];
const CURSOR_KEYS: Record<CursorStyle, Key> = {
  block: "look.cursorBlock", bar: "look.cursorBar", underline: "look.cursorUnderline",
};

/** One scheme as a clickable card: its background with the 8 normal colors. */
function SchemeCard({ id, on, onPick }: { id: SchemeId; on: boolean; onPick: (id: SchemeId) => void }) {
  const s = SCHEMES[id];
  return (
    <button type="button" className={on ? "scheme-card on" : "scheme-card"} aria-pressed={on}
      onClick={() => onPick(id)} style={{ background: s.theme.background, color: s.theme.foreground }}>
      <span className="scheme-name">{s.name}</span>
      <span className="scheme-dots" aria-hidden="true">
        {ANSI_KEYS.slice(1, 7).map((k) => <i key={k} style={{ background: s.theme[k] }} />)}
      </span>
    </button>
  );
}

/** A labelled range input showing its value. */
function Slider({ label, value, min, max, step, unit = "", onChange }: {
  label: string; value: number; min: number; max: number; step: number; unit?: string;
  onChange: (v: number) => void;
}) {
  return (
    <label className="settings-row settings-select">
      <span>{label}</span>
      <span className="settings-range">
        <input type="range" min={min} max={max} step={step} value={value} aria-label={label}
          onChange={(e) => onChange(Number(e.target.value))} />
        <output>{value}{unit}</output>
      </span>
    </label>
  );
}

/** Settings → Terminal appearance: every option of `terminal/appearance.ts`. */
export default function TerminalAppearance() {
  const t = useT();
  const a = useAppearance();
  const appTheme = useTheme();
  const followed = MATCH[appTheme].scheme;
  const set = (patch: Partial<Appearance>) => updateAppearance(patch);
  const dark = SCHEME_IDS.filter((id) => SCHEMES[id].dark);
  const light = SCHEME_IDS.filter((id) => !SCHEMES[id].dark);
  const cards = (ids: SchemeId[]) => (
    <div className="scheme-grid">
      {ids.map((id) => (
        <SchemeCard key={id} id={id} on={(a.followApp ? followed : a.scheme) === id}
          onPick={(scheme) => set({ scheme, followApp: false })} />
      ))}
    </div>
  );

  return (
    <fieldset>
      <legend>{t("look.title")}</legend>
      <TerminalPreview />

      <label className="settings-row">
        <input type="checkbox" checked={a.followApp} onChange={(e) => set({ followApp: e.target.checked })} />
        <span>
          {t("look.followApp")}
          <span className="settings-hint">{t("look.followAppHint", { scheme: SCHEMES[followed].name })}</span>
        </span>
      </label>
      <div className="settings-sub">{t("look.schemeDark")}</div>
      {cards(dark)}
      <div className="settings-sub">{t("look.schemeLight")}</div>
      {cards(light)}

      <div className="settings-sub">{t("look.text")}</div>
      <label className="settings-row settings-select">
        <span>{t("look.font")}</span>
        <select value={a.font} onChange={(e) => set({ font: e.target.value as FontChoice })}>
          {FONTS.map((f) => (
            <option key={f} value={f}>
              {f === "default" ? t("look.fontDefault") : f === "custom" ? t("look.fontCustom") : f}
            </option>
          ))}
        </select>
      </label>
      {a.font === "custom" && (
        <label className="settings-row settings-select">
          <span>{t("look.fontName")}</span>
          <input type="text" className="settings-text" value={a.customFont} maxLength={64}
            placeholder="Maple Mono NF" spellCheck={false}
            onChange={(e) => { if (validFontName(e.target.value)) set({ customFont: e.target.value }); }} />
        </label>
      )}
      <Slider label={t("look.fontSize")} value={a.fontSize} min={8} max={32} step={1} unit=" px"
        onChange={(fontSize) => set({ fontSize })} />
      <label className="settings-row settings-select">
        <span>{t("look.fontWeight")}</span>
        <select value={a.fontWeight} onChange={(e) => set({ fontWeight: Number(e.target.value) as Appearance["fontWeight"] })}>
          {FONT_WEIGHTS.map((w) => <option key={w} value={w}>{w}</option>)}
        </select>
      </label>
      <Slider label={t("look.lineHeight")} value={a.lineHeight} min={1} max={2} step={0.05}
        onChange={(lineHeight) => set({ lineHeight })} />
      <Slider label={t("look.letterSpacing")} value={a.letterSpacing} min={0} max={4} step={0.5} unit=" px"
        onChange={(letterSpacing) => set({ letterSpacing })} />
      <Slider label={t("look.padding")} value={a.padding} min={0} max={24} step={1} unit=" px"
        onChange={(padding) => set({ padding })} />

      <div className="settings-sub">{t("look.cursor")}</div>
      <div className="settings-row seg" role="radiogroup" aria-label={t("look.cursor")}>
        {CURSOR_STYLES.map((c) => (
          <button key={c} type="button" role="radio" aria-checked={a.cursorStyle === c}
            className={a.cursorStyle === c ? "seg-btn on" : "seg-btn"} onClick={() => set({ cursorStyle: c })}>
            <span className={`cursor-demo ${c}`} aria-hidden="true" />{t(CURSOR_KEYS[c])}
          </button>
        ))}
      </div>
      <label className="settings-row">
        <input type="checkbox" checked={a.cursorBlink} onChange={(e) => set({ cursorBlink: e.target.checked })} />
        <span>{t("look.cursorBlink")}</span>
      </label>

      <div className="settings-sub">{t("look.advanced")}</div>
      <label className="settings-row settings-select">
        <span>{t("look.scrollback")}</span>
        <select value={a.scrollback} onChange={(e) => set({ scrollback: Number(e.target.value) })}>
          {(SCROLLBACK.includes(a.scrollback) ? SCROLLBACK : [...SCROLLBACK, a.scrollback].sort((x, y) => x - y))
            .map((n) => <option key={n} value={n}>{n.toLocaleString()}</option>)}
        </select>
      </label>
      <label className="settings-row">
        <input type="checkbox" checked={a.boldBright} onChange={(e) => set({ boldBright: e.target.checked })} />
        <span>{t("look.boldBright")}</span>
      </label>
      <Slider label={t("look.minContrast")} value={a.minContrast} min={1} max={7} step={0.5}
        onChange={(minContrast) => set({ minContrast })} />
      <span className="settings-hint">{t("look.minContrastHint")}</span>

      <button type="button" className="settings-btn" onClick={resetAppearance}>{t("look.reset")}</button>
    </fieldset>
  );
}
