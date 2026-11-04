import type { CSSProperties } from "react";
import { LANGS, LANG_NAMES, setLang, useLang, useT, type Lang } from "../i18n";
import { THEME_IDS, THEMES, setTheme, useTheme } from "../theme";

/**
 * Language select and app theme swatches. Both live in the browser (no
 * backend settings), so they work while the backend settings still load.
 */
export default function GeneralPrefs() {
  const t = useT();
  const lang = useLang();
  const theme = useTheme();
  return (
    <fieldset>
      <legend>{t("settings.general")}</legend>
      <label className="settings-row settings-select">
        <span>{t("nav.language")}</span>
        <select value={lang} onChange={(e) => setLang(e.target.value as Lang)}>
          {LANGS.map((l) => <option key={l} value={l} lang={l}>{LANG_NAMES[l]}</option>)}
        </select>
      </label>
      <div className="settings-row settings-select">
        <span id="prefs-theme-label">{t("nav.theme")}</span>
        <div className="theme-switch" role="group" aria-labelledby="prefs-theme-label">
          {THEME_IDS.map((id) => {
            const name = t(`theme.${id}`);
            const tk = THEMES[id].tokens;
            return (
              <button key={id} type="button" className={id === theme ? "theme-swatch on" : "theme-swatch"}
                aria-pressed={id === theme} aria-label={name} title={name}
                style={{ "--sw-bg": tk.bg, "--sw-ac": tk.accent } as CSSProperties}
                onClick={() => setTheme(id)} />
            );
          })}
        </div>
      </div>
    </fieldset>
  );
}
