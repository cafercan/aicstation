//! Glue between the snapshot thread, the alert detector, the toasts and the
//! tray. Runs after every snapshot refresh, whether the window is shown or not.
use crate::alerts::{Alert, AlertKind, AlertToggles, Detector};
use crate::errors;
use crate::native_text::{texts, with_name, with_name_n};
use crate::settings::{Settings, SettingsStore};
use crate::toast::{Toast, Toaster};
use crate::tray;
use station_core::locate::is_session_id;
use station_core::monitor::Snapshot;
use std::sync::Mutex;
use std::time::Instant;
use tauri::{AppHandle, Manager};

#[derive(Default)]
pub struct Notifier {
    detector: Mutex<Detector>,
    /// Sessions whose detail or embedded terminal panes the UI shows
    /// (set_focus_context): at most [`MAX_FOCUS`].
    focus: Mutex<Vec<String>>,
}

/// Sessions the UI can show at once: the detail view, or two split panes.
pub const MAX_FOCUS: usize = 2;

/// Validates the ids sent by the UI: at most two lowercase session UUIDs
/// (duplicates collapse). Anything else is refused as a whole.
pub fn parse_focus(session_ids: Vec<String>) -> Result<Vec<String>, String> {
    if session_ids.len() > MAX_FOCUS || !session_ids.iter().all(|id| is_session_id(id)) {
        return Err(errors::SESSION_INVALID.into());
    }
    let mut out: Vec<String> = Vec::with_capacity(session_ids.len());
    for id in session_ids {
        if !out.contains(&id) {
            out.push(id);
        }
    }
    Ok(out)
}

/// The sessions to keep quiet about: the focus context, but only while the
/// window is visible, not minimized and focused.
pub fn suppressed(context: &[String], visible: bool, minimized: bool, focused: bool) -> &[String] {
    if visible && !minimized && focused { context } else { &[] }
}

pub fn toggles(s: &Settings) -> AlertToggles {
    AlertToggles {
        waiting: s.notify_waiting,
        finished: s.notify_finished,
        subagent: s.notify_subagent,
        context: s.notify_context.then_some(s.context_threshold),
        stalled: s.notify_stalled.then(|| std::time::Duration::from_secs(u64::from(s.stalled_minutes) * 60)),
    }
}

/// Title and body of an alert's toast in `lang`.
pub fn alert_text(a: &Alert, lang: &str) -> (String, Option<String>) {
    let t = texts(lang);
    let template = match a.kind {
        AlertKind::Waiting => t.waiting,
        AlertKind::Finished => t.finished,
        AlertKind::Subagent => t.subagent,
        AlertKind::Context => t.context,
        AlertKind::Stalled => t.stalled,
    };
    let title = match a.n {
        Some(n) => with_name_n(template, &a.name, n),
        None => with_name(template, &a.name),
    };
    (title, a.waiting_for.clone())
}

impl Notifier {
    pub fn set_focus(&self, session_ids: Vec<String>) -> Result<(), String> {
        let ids = parse_focus(session_ids)?;
        *self.focus.lock().unwrap_or_else(|e| e.into_inner()) = ids;
        Ok(())
    }

    fn focused_sessions(&self, app: &AppHandle) -> Vec<String> {
        let ctx = self.focus.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let Some(w) = app.get_webview_window("main") else { return Vec::new() };
        let visible = w.is_visible().unwrap_or(false);
        let minimized = w.is_minimized().unwrap_or(false);
        let focused = w.is_focused().unwrap_or(false);
        suppressed(&ctx, visible, minimized, focused).to_vec()
    }
}

/// After each snapshot: toasts for new alerts, then the tray.
pub fn after_snapshot(app: &AppHandle, snap: &Snapshot) {
    let settings = app.state::<SettingsStore>().get();
    let notifier = app.state::<Notifier>();
    let focused = notifier.focused_sessions(app);
    let focused: Vec<&str> = focused.iter().map(String::as_str).collect();
    let alerts = notifier.detector.lock().unwrap_or_else(|e| e.into_inner()).observe(
        &snap.sessions,
        Instant::now(),
        toggles(&settings),
        &focused,
    );
    let toaster = app.state::<Toaster>();
    for a in alerts {
        let (title, body) = alert_text(&a, &settings.lang);
        let toast = Toast::new(&title, body.as_deref(), settings.notify_sound);
        let handle = app.clone();
        let id = a.session_id.clone();
        toaster.show(&toast, move || tray::navigate(&handle, Some(id.clone())));
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
        let toast = Toast::new(texts(&s.lang).tray_notice, None, s.notify_sound);
        let handle = app.clone();
        app.state::<Toaster>().show(&toast, move || tray::show_main(&handle));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "0b2f6a40-1111-4c3e-9d2a-5f1e2d3c4b5a";

    const B: &str = "7c9e1d20-2222-4a8b-8e3f-1a2b3c4d5e6f";

    #[test]
    fn focus_context_is_up_to_two_session_uuids() {
        assert_eq!(parse_focus(vec![]), Ok(vec![]));
        assert_eq!(parse_focus(vec![A.into()]), Ok(vec![A.into()]));
        assert_eq!(parse_focus(vec![A.into(), B.into()]), Ok(vec![A.into(), B.into()]));
        assert_eq!(parse_focus(vec![A.into(), A.into()]), Ok(vec![A.into()]), "duplicates collapse");
        let three = vec![A.into(), B.into(), A.into()];
        assert_eq!(parse_focus(three), Err(errors::SESSION_INVALID.into()), "at most two");
        for bad in ["", "dashboard", "0B2F6A40-1111-4C3E-9D2A-5F1E2D3C4B5A", "../../etc", &"a".repeat(5000)] {
            assert_eq!(parse_focus(vec![bad.into()]), Err(errors::SESSION_INVALID.into()), "{bad}");
            // One bad id refuses the whole list.
            assert_eq!(parse_focus(vec![A.into(), bad.into()]), Err(errors::SESSION_INVALID.into()), "{bad}");
        }
    }

    #[test]
    fn suppression_needs_a_visible_focused_window() {
        let both = vec![A.to_string(), B.to_string()];
        assert_eq!(suppressed(&both, true, false, true), &both[..]);
        assert!(suppressed(&both, false, false, true).is_empty(), "hidden in the tray");
        assert!(suppressed(&both, true, true, true).is_empty(), "minimized");
        assert!(suppressed(&both, true, false, false).is_empty(), "another app has focus");
        assert!(suppressed(&[], true, false, true).is_empty());
    }

    #[test]
    fn toast_texts_follow_the_language() {
        let a = Alert { kind: AlertKind::Waiting, session_id: A.into(), name: "api".into(), waiting_for: Some("Bash".into()), n: None };
        assert_eq!(alert_text(&a, "tr"), ("api onay bekliyor".into(), Some("Bash".into())));
        let f = Alert { kind: AlertKind::Finished, waiting_for: None, ..a };
        assert_eq!(alert_text(&f, "tr"), ("api tamamlandı".into(), None));
        assert_eq!(alert_text(&f, "en").0, "api finished");
        let c = Alert { kind: AlertKind::Context, n: Some(87), ..f };
        assert_eq!(alert_text(&c, "tr").0, "api: bağlam 87% dolu");
        assert_eq!(alert_text(&c, "en").0, "api: context is 87% full");
        let q = Alert { kind: AlertKind::Stalled, n: Some(10), ..c };
        assert_eq!(alert_text(&q, "tr").0, "api 10 dakikadır sessiz");
    }

    #[test]
    fn toggles_come_from_settings() {
        let s = Settings { notify_waiting: false, ..Settings::default() };
        assert_eq!(
            toggles(&s),
            AlertToggles { waiting: false, finished: true, subagent: false, context: Some(85), stalled: None }
        );
        let q = Settings { notify_stalled: true, stalled_minutes: 15, ..s.clone() };
        assert_eq!(toggles(&q).stalled, Some(std::time::Duration::from_secs(900)));
        let s = Settings { notify_context: false, context_threshold: 70, ..s };
        assert_eq!(toggles(&s).context, None);
        let s = Settings { notify_context: true, ..s };
        assert_eq!(toggles(&s).context, Some(70));
    }
}
