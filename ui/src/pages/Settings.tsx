import { useState } from "react";
import { errorText, useT, type Key } from "../i18n";
import { setToggle, useSettings, type Toggle } from "../settings";

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
  if (!s) return <div className="settings"><p className="empty">{t("settings.loading")}</p></div>;

  const toggle = (key: Toggle, value: boolean) => {
    setError(null);
    setToggle(key, value).catch((e: unknown) =>
      setError(typeof e === "string" || e instanceof Error ? errorText(e) : t("err.settings_save_failed")));
  };
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
        <legend>{t("settings.window")}</legend>
        {row("closeToTray", "settings.closeToTray", "settings.closeToTrayHint")}
      </fieldset>
      {error && <div className="settings-err" role="alert">{error}</div>}
    </div>
  );
}
