//! Glue between the snapshot thread, the alert detector, the toasts and the
//! tray. Runs after every snapshot refresh, whether the window is shown or not.
use crate::alerts::{Alert, AlertKind, AlertToggles, Detector};
use crate::errors;
use crate::native_text::{texts, with_name};
use crate::settings::{Settings, SettingsStore};
use crate::toast::{toast_xml, Toaster};
use crate::tray;
use station_core::locate::is_session_id;
use station_core::monitor::Snapshot;
use std::sync::Mutex;
use std::time::Instant;
use tauri::{AppHandle, Manager};

#[derive(Default)]
pub struct Notifier {
    detector: Mutex<Detector>,
    /// Session whose detail or embedded terminal the UI shows (set_focus_context).
    focus: Mutex<Option<String>>,
}

/// Validates the id sent by the UI: None, or a lowercase session UUID.
pub fn parse_focus(session_id: Option<String>) -> Result<Option<String>, String> {
    match session_id {
        None => Ok(None),
        Some(id) if is_session_id(&id) => Ok(Some(id)),
        Some(_) => Err(errors::SESSION_INVALID.into()),
    }
}

/// The session to keep quiet about: the focus context, but only while the
/// window is visible, not minimized and focused.
pub fn suppressed(context: Option<&str>, visible: bool, minimized: bool, focused: bool) -> Option<&str> {
    context.filter(|_| visible && !minimized && focused)
}

pub fn toggles(s: &Settings) -> AlertToggles {
    AlertToggles { waiting: s.notify_waiting, finished: s.notify_finished, subagent: s.notify_subagent }
}

/// Title and body of an alert's toast in `lang`.
pub fn alert_text(a: &Alert, lang: &str) -> (String, Option<String>) {
    let t = texts(lang);
    let template = match a.kind {
        AlertKind::Waiting => t.waiting,
        AlertKind::Finished => t.finished,
        AlertKind::Subagent => t.subagent,
    };
    (with_name(template, &a.name), a.waiting_for.clone())
}

impl Notifier {
    pub fn set_focus(&self, session_id: Option<String>) -> Result<(), String> {
        let id = parse_focus(session_id)?;
        *self.focus.lock().unwrap_or_else(|e| e.into_inner()) = id;
        Ok(())
    }

    fn focused_session(&self, app: &AppHandle) -> Option<String> {
        let ctx = self.focus.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let w = app.get_webview_window("main")?;
        let visible = w.is_visible().unwrap_or(false);
        let minimized = w.is_minimized().unwrap_or(false);
        let focused = w.is_focused().unwrap_or(false);
        suppressed(ctx.as_deref(), visible, minimized, focused).map(str::to_string)
    }
}

/// After each snapshot: toasts for new alerts, then the tray.
pub fn after_snapshot(app: &AppHandle, snap: &Snapshot) {
    let settings = app.state::<SettingsStore>().get();
    let notifier = app.state::<Notifier>();
    let focused = notifier.focused_session(app);
    let alerts = notifier.detector.lock().unwrap_or_else(|e| e.into_inner()).observe(
        &snap.sessions,
        Instant::now(),
        toggles(&settings),
        focused.as_deref(),
    );
    let toaster = app.state::<Toaster>();
    for a in alerts {
        let (title, body) = alert_text(&a, &settings.lang);
        let xml = toast_xml(&title, body.as_deref(), settings.notify_sound);
        let handle = app.clone();
        let id = a.session_id.clone();
        toaster.show(&xml, move || tray::navigate(&handle, Some(id.clone())));
    }
    tray::update(app, snap, &settings.lang);
}

/// Closing the window with "minimize to tray" on: hide it; the first time,
/// tell the user the app keeps running.
pub fn hide_to_tray(app: &AppHandle) -> Result<(), String> {
    let w = app.get_webview_window("main").ok_or(errors::TASK_FAILED)?;
    w.hide().map_err(|e| errors::with(errors::TASK_FAILED, e))?;
    let store = app.state::<SettingsStore>();
    if store.take_tray_notice() {
        let s = store.get();
        let xml = toast_xml(texts(&s.lang).tray_notice, None, s.notify_sound);
        let handle = app.clone();
        app.state::<Toaster>().show(&xml, move || tray::show_main(&handle));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "0b2f6a40-1111-4c3e-9d2a-5f1e2d3c4b5a";

    #[test]
    fn focus_context_must_be_a_session_uuid_or_null() {
        assert_eq!(parse_focus(None), Ok(None));
        assert_eq!(parse_focus(Some(A.into())), Ok(Some(A.into())));
        for bad in ["", "dashboard", "0B2F6A40-1111-4C3E-9D2A-5F1E2D3C4B5A", "../../etc", &"a".repeat(5000)] {
            assert_eq!(parse_focus(Some(bad.into())), Err(errors::SESSION_INVALID.into()), "{bad}");
        }
    }

    #[test]
    fn suppression_needs_a_visible_focused_window() {
        assert_eq!(suppressed(Some(A), true, false, true), Some(A));
        assert_eq!(suppressed(Some(A), false, false, true), None, "hidden in the tray");
        assert_eq!(suppressed(Some(A), true, true, true), None, "minimized");
        assert_eq!(suppressed(Some(A), true, false, false), None, "another app has focus");
        assert_eq!(suppressed(None, true, false, true), None);
    }

    #[test]
    fn toast_texts_follow_the_language() {
        let a = Alert { kind: AlertKind::Waiting, session_id: A.into(), name: "api".into(), waiting_for: Some("Bash".into()) };
        assert_eq!(alert_text(&a, "tr"), ("api onay bekliyor".into(), Some("Bash".into())));
        let f = Alert { kind: AlertKind::Finished, waiting_for: None, ..a };
        assert_eq!(alert_text(&f, "tr"), ("api tamamlandı".into(), None));
        assert_eq!(alert_text(&f, "en").0, "api finished");
    }

    #[test]
    fn toggles_come_from_settings() {
        let s = Settings { notify_waiting: false, ..Settings::default() };
        assert_eq!(toggles(&s), AlertToggles { waiting: false, finished: true, subagent: false });
    }
}
