import { useEffect, useState } from "react";
import { butaglyphInfo, listShells, openButaglyphEditor, type ButaglyphInfo } from "../api";
import { errorText, useT, type Key } from "../i18n";
import { isWindows } from "../platform";
import { setChoice, setToggle, useSettings, type Choice, type Toggle } from "../settings";

/** Display names of the shell ids (settings.rs SHELLS). */
const SHELL_NAMES: Record<string, string> = {
  bash: "bash", zsh: "zsh", fish: "fish", pwsh: "PowerShell 7 (pwsh)", nu: "Nushell", powershell: "Windows PowerShell",
};

const NOTIFY: [Toggle, Key][] = [
  ["notifyWaiting", "settings.notifyWaiting"],
  ["notifyFinished", "settings.notifyFinished"],
  ["notifySubagent", "settings.notifySubagent"],
  ["notifySound", "settings.notifySound"],
];

/** Notification and window toggles, stored by the backend in settings.json. */
export default function SettingsPage() {
  const t = useT();
  const s = useSettings();
  const [error, setError] = useState<string | null>(null);
  const [shells, setShells] = useState<string[]>(["auto"]);
  const [buta, setButa] = useState<ButaglyphInfo | null>(null);
  useEffect(() => {
    let live = true;
    listShells().then((ids) => { if (live && ids.length) setShells(ids); }).catch(() => {});
    butaglyphInfo().then((i) => { if (live && i) setButa(i); }).catch(() => {});
    return () => { live = false; };
  }, []);
  if (!s) return <div className="settings"><p className="empty">{t("settings.loading")}</p></div>;

  const fail = (e: unknown) =>
    setError(typeof e === "string" || e instanceof Error ? errorText(e) : t("err.settings_save_failed"));
  const toggle = (key: Toggle, value: boolean) => {
    setError(null);
    setToggle(key, value).catch(fail);
  };
  const choose = (key: Choice, value: string) => {
    setError(null);
    setChoice(key, value).catch(fail);
  };
  // A stored shell that is no longer installed stays selectable (the
  // backend falls back to the default for it).
  const shellIds = shells.includes(s.shell) ? shells : [...shells, s.shell];
  const openEditor = () => {
    setError(null);
    openButaglyphEditor().catch(fail);
  };
  const presets = buta?.presets ?? [];
  const presetIds = s.promptPreset === "" || presets.some((p) => p.name === s.promptPreset)
    ? presets.map((p) => p.name) : [...presets.map((p) => p.name), s.promptPreset];
  const shellName = (id: string) =>
    id === "auto" ? t(isWindows() ? "settings.shellAutoWindows" : "settings.shellAuto") : SHELL_NAMES[id] ?? id;
  const row = (key: Toggle, label: Key, hint?: Key) => (
    <label key={key} className="settings-row">
      <input type="checkbox" checked={s[key]} onChange={(e) => toggle(key, e.target.checked)} />
      <span>
        {t(label)}
        {hint && <span className="settings-hint">{t(hint)}</span>}
      </span>
    </label>
  );

  return (
    <div className="settings">
      <h2>{t("settings.title")}</h2>
      <fieldset>
        <legend>{t("settings.notifications")}</legend>
        {NOTIFY.map(([key, label]) => row(key, label))}
      </fieldset>
      <fieldset>
        <legend>{t("settings.terminal")}</legend>
        <label className="settings-row settings-select">
          <span>{t("settings.shell")}</span>
          <select value={s.shell} onChange={(e) => choose("shell", e.target.value)}>
            {shellIds.map((id) => <option key={id} value={id}>{shellName(id)}</option>)}
          </select>
        </label>
        <label className="settings-row settings-select">
          <span>{t("settings.prompt")}</span>
          <select value={s.prompt} onChange={(e) => choose("prompt", e.target.value)}>
            <option value="own">{t("settings.promptOwn")}</option>
            <option value="butaglyph">butaglyph</option>
          </select>
        </label>
        {s.prompt === "butaglyph" && buta && !buta.installed && (
          <span className="settings-hint settings-warn">{t("settings.butaglyphMissing")}</span>
        )}
        {s.prompt === "butaglyph" && buta?.installed && (
          <>
            <label className="settings-row settings-select">
              <span>{t("settings.promptPreset")}</span>
              <select value={s.promptPreset} onChange={(e) => choose("promptPreset", e.target.value)}>
                <option value="">{t("settings.presetOwn")}</option>
                {presetIds.map((name) => (
                  <option key={name} value={name} title={presets.find((p) => p.name === name)?.description}>{name}</option>
                ))}
              </select>
            </label>
            {s.promptPreset !== "" && (
              <span className="settings-hint">{presets.find((p) => p.name === s.promptPreset)?.description}</span>
            )}
            {s.promptPreset === "" && (
              <button type="button" className="settings-btn" onClick={openEditor}>{t("settings.butaglyphEditor")}</button>
            )}
            {buta.version && <span className="settings-hint">{buta.version}</span>}
          </>
        )}
        <span className="settings-hint">{t("settings.terminalHint")}</span>
      </fieldset>
      <fieldset>
        <legend>{t("settings.window")}</legend>
        {row("closeToTray", "settings.closeToTray", "settings.closeToTrayHint")}
      </fieldset>
      {error && <div className="settings-err" role="alert">{error}</div>}
    </div>
  );
}
