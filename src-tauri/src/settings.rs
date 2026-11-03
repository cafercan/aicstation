//! App settings in `<app data>/settings.json`: notification toggles, close
//! behavior and the UI language (for the texts Rust shows natively: toasts,
//! tray). Unknown keys are ignored; a missing, mistyped or invalid key takes
//! its own default while the valid keys are kept; an unreadable file gives
//! the defaults. Writes are atomic.
use crate::errors;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Language codes the UI offers (ui/src/i18n LANGS).
pub const LANGS: [&str; 8] = ["tr", "az", "en", "de", "ru", "es", "fr", "zh"];
/// Largest settings.json read; anything bigger is treated as corrupt.
const MAX_FILE: u64 = 64 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    /// A session enters "waiting" (onay bekliyor).
    pub notify_waiting: bool,
    /// A session that was busy for >= 10 s becomes ready.
    pub notify_finished: bool,
    /// A subagent of a session finishes.
    pub notify_subagent: bool,
    /// Toasts play the system default notification sound.
    pub notify_sound: bool,
    /// Closing the window hides it to the tray instead of exiting.
    pub close_to_tray: bool,
    /// The one-time "still running in the tray" toast was shown. Owned by
    /// Rust: a value sent by the UI is ignored.
    pub tray_notice_shown: bool,
    pub lang: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            notify_waiting: true,
            notify_finished: true,
            notify_subagent: false,
            notify_sound: false,
            close_to_tray: true,
            tray_notice_shown: false,
            lang: "tr".into(),
        }
    }
}

impl Settings {
    /// Ok when every field is acceptable (only `lang` can be wrong).
    pub fn validate(&self) -> Result<(), String> {
        if LANGS.contains(&self.lang.as_str()) {
            Ok(())
        } else {
            Err(errors::SETTINGS_INVALID.into())
        }
    }
}

/// Settings from a parsed settings.json, field by field: each field of the
/// right type and value is taken, any other falls back to its own default.
/// A non-object gives the defaults.
pub fn from_json(v: &Value) -> Settings {
    let d = Settings::default();
    let Some(o) = v.as_object() else { return d };
    let flag = |key: &str, default: bool| o.get(key).and_then(Value::as_bool).unwrap_or(default);
    Settings {
        notify_waiting: flag("notifyWaiting", d.notify_waiting),
        notify_finished: flag("notifyFinished", d.notify_finished),
        notify_subagent: flag("notifySubagent", d.notify_subagent),
        notify_sound: flag("notifySound", d.notify_sound),
        close_to_tray: flag("closeToTray", d.close_to_tray),
        tray_notice_shown: flag("trayNoticeShown", d.tray_notice_shown),
        lang: o
            .get("lang")
            .and_then(Value::as_str)
            .filter(|l| LANGS.contains(l))
            .map_or(d.lang, str::to_string),
    }
}

/// The language stored in settings.json at `path`, only when the file has a
/// valid one (None for a missing, unreadable or invalid file or field).
#[cfg_attr(not(windows), allow(dead_code))]
pub fn stored_lang(path: &Path) -> Option<String> {
    let ok_size = std::fs::metadata(path).map(|m| m.len() <= MAX_FILE).unwrap_or(false);
    if !ok_size {
        return None;
    }
    let v: Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    v.get("lang").and_then(Value::as_str).filter(|l| LANGS.contains(l)).map(str::to_string)
}

/// Settings from `path`: defaults when missing, too large or unparsable;
/// otherwise validated per field ([`from_json`]).
pub fn load(path: &Path) -> Settings {
    let ok_size = std::fs::metadata(path).map(|m| m.len() <= MAX_FILE).unwrap_or(false);
    if !ok_size {
        return Settings::default();
    }
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .map(|v| from_json(&v))
        .unwrap_or_default()
}

/// Atomic write: temp file next to `path`, then rename over it.
pub fn save(path: &Path, s: &Settings) -> std::io::Result<()> {
    let json = serde_json::to_string_pretty(s).map_err(std::io::Error::other)?;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, json).and_then(|()| std::fs::rename(&tmp, path)).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// The settings shared by commands, the snapshot thread and the tray.
pub struct SettingsStore {
    path: PathBuf,
    current: Mutex<Settings>,
}

impl SettingsStore {
    pub fn open(path: PathBuf) -> Self {
        let current = Mutex::new(load(&path));
        Self { path, current }
    }

    pub fn get(&self) -> Settings {
        self.current.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Validates and stores settings sent by the UI; `tray_notice_shown`
    /// keeps its stored value.
    pub fn set_from_ui(&self, mut s: Settings) -> Result<Settings, String> {
        s.validate()?;
        let mut cur = self.current.lock().unwrap_or_else(|e| e.into_inner());
        s.tray_notice_shown = cur.tray_notice_shown;
        save(&self.path, &s).map_err(|e| errors::with(errors::SETTINGS_SAVE_FAILED, e))?;
        *cur = s.clone();
        Ok(s)
    }

    /// Marks the tray notice as shown. True when it had not been shown yet
    /// (the caller shows it now).
    pub fn take_tray_notice(&self) -> bool {
        let mut cur = self.current.lock().unwrap_or_else(|e| e.into_inner());
        if cur.tray_notice_shown {
            return false;
        }
        cur.tray_notice_shown = true;
        if let Err(e) = save(&self.path, &cur) {
            eprintln!("ayarlar kaydedilemedi: {e}");
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_the_brief() {
        let s = Settings::default();
        assert!(s.notify_waiting && s.notify_finished && s.close_to_tray);
        assert!(!s.notify_subagent && !s.notify_sound && !s.tray_notice_shown);
        assert_eq!(s.lang, "tr");
    }

    #[test]
    fn missing_and_unknown_keys() {
        let s: Settings = serde_json::from_str(r#"{"notifySound":true,"evil":"x","nested":{"a":1}}"#).unwrap();
        assert!(s.notify_sound);
        assert_eq!(Settings { notify_sound: false, ..s }, Settings::default());
    }

    #[test]
    fn validation_rejects_unknown_language() {
        for bad in ["", "xx", "TR", "en-US", "tr\0"] {
            let s = Settings { lang: bad.into(), ..Settings::default() };
            assert_eq!(s.validate(), Err(errors::SETTINGS_INVALID.to_string()), "{bad:?}");
        }
        for ok in LANGS {
            assert!(Settings { lang: ok.into(), ..Settings::default() }.validate().is_ok());
        }
    }

    #[test]
    fn load_falls_back_to_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("settings.json");
        assert_eq!(load(&p), Settings::default(), "missing file");
        std::fs::write(&p, "{not json").unwrap();
        assert_eq!(load(&p), Settings::default(), "corrupt file");
        std::fs::write(&p, "[true, false]").unwrap();
        assert_eq!(load(&p), Settings::default(), "not an object");
        std::fs::write(&p, " ".repeat(MAX_FILE as usize + 1)).unwrap();
        assert_eq!(load(&p), Settings::default(), "oversized file");
    }

    #[test]
    fn an_invalid_field_takes_its_own_default_and_valid_fields_are_kept() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("settings.json");
        std::fs::write(&p, r#"{"lang":"klingon","notifySound":true,"closeToTray":false}"#).unwrap();
        let s = load(&p);
        assert_eq!(s, Settings { notify_sound: true, close_to_tray: false, ..Settings::default() }, "invalid lang");
        std::fs::write(&p, r#"{"notifyWaiting":"yes","notifySubagent":true,"lang":"de"}"#).unwrap();
        let s = load(&p);
        assert_eq!(s, Settings { notify_subagent: true, lang: "de".into(), ..Settings::default() }, "wrong type");
        std::fs::write(&p, r#"{"lang":7,"notifyFinished":null,"trayNoticeShown":true}"#).unwrap();
        assert_eq!(load(&p), Settings { tray_notice_shown: true, ..Settings::default() });
    }

    #[test]
    fn stored_lang_only_from_a_valid_file_field() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("settings.json");
        assert_eq!(stored_lang(&p), None);
        std::fs::write(&p, r#"{"lang":"ru"}"#).unwrap();
        assert_eq!(stored_lang(&p).as_deref(), Some("ru"));
        std::fs::write(&p, r#"{"lang":"xx"}"#).unwrap();
        assert_eq!(stored_lang(&p), None);
        std::fs::write(&p, "{bozuk").unwrap();
        assert_eq!(stored_lang(&p), None);
    }

    #[test]
    fn save_is_atomic_and_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("settings.json");
        let s = Settings { notify_waiting: false, lang: "de".into(), ..Settings::default() };
        save(&p, &s).unwrap();
        assert_eq!(load(&p), s);
        let s2 = Settings { close_to_tray: false, ..s };
        save(&p, &s2).unwrap(); // replaces an existing file
        assert_eq!(load(&p), s2);
        let names: Vec<_> = std::fs::read_dir(tmp.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, ["settings.json"], "no temp file left behind");
    }

    #[test]
    fn store_validates_persists_and_owns_the_tray_notice_flag() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("settings.json");
        let store = SettingsStore::open(p.clone());
        assert!(store.set_from_ui(Settings { lang: "xx".into(), ..Settings::default() }).is_err());
        assert!(!p.exists(), "an invalid value is not written");

        assert!(store.take_tray_notice());
        assert!(!store.take_tray_notice(), "only once");
        // The UI cannot reset the flag.
        let out = store.set_from_ui(Settings { notify_sound: true, tray_notice_shown: false, ..Settings::default() }).unwrap();
        assert!(out.tray_notice_shown && out.notify_sound);
        let reopened = SettingsStore::open(p);
        assert!(reopened.get().tray_notice_shown && reopened.get().notify_sound);
        assert!(!reopened.take_tray_notice(), "remembered across runs");
    }
}
