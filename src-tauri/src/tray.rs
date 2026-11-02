//! System tray: app icon (with an attention dot while a session waits),
//! translated tooltip and menu (show/hide, dashboard, up to 5 waiting
//! sessions, quit). Also the "bring the window forward and open a view"
//! helpers shared with toasts and the single-instance signal.
use crate::native_text::{self, texts};
use serde::Serialize;
use station_core::locate::is_session_id;
use station_core::monitor::Snapshot;
use station_core::registry::SessionStatus;
use std::sync::Mutex;
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager};

const TRAY_ID: &str = "main";
/// Waiting sessions listed in the tray menu.
pub const MAX_WAITING_ITEMS: usize = 5;
const ICON: Image<'static> = tauri::include_image!("./icons/32x32.png");

/// Event to the UI: open a view. `session: None` = dashboard.
pub const NAVIGATE_EVENT: &str = "navigate";
/// Event to the UI: the user chose "Quit" in the tray; the UI runs its close
/// confirmation and then closes the window (which exits the app).
pub const QUIT_EVENT: &str = "tray-quit";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Navigate {
    pub session: Option<String>,
}

/// `rgba` (w x h, 4 bytes per pixel) with a red dot and a light ring in the
/// bottom-right corner. Same size as the input.
pub fn attention_dot(rgba: &[u8], w: u32, h: u32) -> Vec<u8> {
    let mut out = rgba.to_vec();
    let (w, h) = (w as i64, h as i64);
    if out.len() as i64 != w * h * 4 || w < 8 || h < 8 {
        return out;
    }
    let r = (w.min(h) as f64 * 0.22).max(2.0);
    let ring = r + (w as f64 / 32.0).max(1.0);
    let (cx, cy) = (w as f64 - ring - 0.5, h as f64 - ring - 0.5);
    for y in 0..h {
        for x in 0..w {
            let d = ((x as f64 + 0.5 - cx).powi(2) + (y as f64 + 0.5 - cy).powi(2)).sqrt();
            let px = if d <= r {
                [0xE5, 0x48, 0x4D, 0xFF]
            } else if d <= ring {
                [0xFF, 0xFF, 0xFF, 0xFF]
            } else {
                continue;
            };
            let i = ((y * w + x) * 4) as usize;
            out[i..i + 4].copy_from_slice(&px);
        }
    }
    out
}

/// What a tray menu item does.
#[derive(Debug, PartialEq, Eq)]
pub enum MenuAction {
    ShowHide,
    Dashboard,
    Session(String),
    Quit,
}

pub fn menu_action(id: &str) -> Option<MenuAction> {
    match id {
        "show" => Some(MenuAction::ShowHide),
        "dashboard" => Some(MenuAction::Dashboard),
        "quit" => Some(MenuAction::Quit),
        _ => id.strip_prefix("s:").filter(|s| is_session_id(s)).map(|s| MenuAction::Session(s.into())),
    }
}

/// What the tray shows; the menu is rebuilt only when it changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayModel {
    pub lang: String,
    pub open: usize,
    pub waiting_total: usize,
    /// (id, name) of the first MAX_WAITING_ITEMS waiting sessions.
    pub waiting: Vec<(String, String)>,
}

pub fn tray_model(snap: &Snapshot, lang: &str) -> TrayModel {
    let waiting: Vec<_> = snap.sessions.iter().filter(|s| s.status == SessionStatus::Waiting).collect();
    TrayModel {
        lang: lang.to_string(),
        open: snap.sessions.len(),
        waiting_total: waiting.len(),
        waiting: waiting
            .iter()
            .take(MAX_WAITING_ITEMS)
            .map(|s| (s.session_id.clone(), s.name.clone()))
            .collect(),
    }
}

/// Last model applied to the tray.
#[derive(Default)]
pub struct TrayState {
    last: Mutex<Option<TrayModel>>,
}

fn build_menu(app: &AppHandle, m: &TrayModel) -> tauri::Result<Menu<tauri::Wry>> {
    let t = texts(&m.lang);
    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(app, "show", t.show_hide, true, None::<&str>)?)?;
    menu.append(&MenuItem::with_id(app, "dashboard", t.dashboard, true, None::<&str>)?)?;
    if !m.waiting.is_empty() {
        menu.append(&PredefinedMenuItem::separator(app)?)?;
        for (id, name) in &m.waiting {
            menu.append(&MenuItem::with_id(app, format!("s:{id}"), native_text::waiting_item(name), true, None::<&str>)?)?;
        }
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(app, "quit", t.quit, true, None::<&str>)?)?;
    Ok(menu)
}

fn icon_for(waiting: bool) -> Image<'static> {
    if waiting {
        Image::new_owned(attention_dot(ICON.rgba(), ICON.width(), ICON.height()), ICON.width(), ICON.height())
    } else {
        ICON.clone()
    }
}

/// Creates the tray icon (call once in setup).
pub fn create(app: &AppHandle, lang: &str) -> tauri::Result<()> {
    let model = TrayModel { lang: lang.into(), open: 0, waiting_total: 0, waiting: Vec::new() };
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon_for(false))
        .tooltip(native_text::tooltip(lang, 0, 0))
        .menu(&build_menu(app, &model)?)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| match menu_action(ev.id().as_ref()) {
            Some(MenuAction::ShowHide) => toggle_main(app),
            Some(MenuAction::Dashboard) => navigate(app, None),
            Some(MenuAction::Session(id)) => navigate(app, Some(id)),
            Some(MenuAction::Quit) => quit(app),
            None => {}
        })
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                show_main(tray.app_handle());
            }
        })
        .build(app)?;
    *app.state::<TrayState>().last.lock().unwrap_or_else(|e| e.into_inner()) = Some(model);
    Ok(())
}

/// Brings the tray in line with the snapshot (no-op when nothing changed).
pub fn update(app: &AppHandle, snap: &Snapshot, lang: &str) {
    let model = tray_model(snap, lang);
    let state = app.state::<TrayState>();
    let mut last = state.last.lock().unwrap_or_else(|e| e.into_inner());
    if last.as_ref() == Some(&model) {
        return;
    }
    let Some(tray) = app.tray_by_id(TRAY_ID) else { return };
    let icon_changed = last.as_ref().is_none_or(|l| (l.waiting_total > 0) != (model.waiting_total > 0));
    if icon_changed {
        let _ = tray.set_icon(Some(icon_for(model.waiting_total > 0)));
    }
    let _ = tray.set_tooltip(Some(native_text::tooltip(lang, model.open, model.waiting_total)));
    let menu_changed = last.as_ref().is_none_or(|l| l.lang != model.lang || l.waiting != model.waiting);
    if menu_changed {
        if let Ok(menu) = build_menu(app, &model) {
            let _ = tray.set_menu(Some(menu));
        }
    }
    *last = Some(model);
}

/// Shows, restores and focuses the main window.
pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

fn toggle_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let shown = w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false);
        if shown {
            let _ = w.hide();
        } else {
            show_main(app);
        }
    }
}

/// Brings the window forward and asks the UI to open a session's detail
/// (or the dashboard).
pub fn navigate(app: &AppHandle, session: Option<String>) {
    show_main(app);
    let _ = app.emit_to("main", NAVIGATE_EVENT, Navigate { session });
}

/// "Quit" from the tray: the UI confirms (embedded Claude sessions), kills
/// its terminals and closes the window; RunEvent::Exit then kills the
/// remaining PTYs and saves the history cache. Without a window there is
/// nothing to confirm, so the app exits directly.
fn quit(app: &AppHandle) {
    if app.get_webview_window("main").is_some() {
        show_main(app);
        let _ = app.emit_to("main", QUIT_EVENT, ());
    } else {
        app.exit(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use station_core::host::HostKind;
    use station_core::monitor::SessionCard;

    const A: &str = "0b2f6a40-1111-4c3e-9d2a-5f1e2d3c4b5a";

    fn card(id: String, status: SessionStatus) -> SessionCard {
        SessionCard {
            session_id: id,
            pid: 1,
            parent_pid: None,
            name: "demo".into(),
            cwd: r"C:\work".into(),
            status,
            waiting_for: None,
            model: None,
            context_pct: None,
            cost_usd: 0.0,
            agents_total: 0,
            agents_finished: 0,
            tool_ok: 0,
            tool_err: 0,
            last_tool: None,
            started_at_ms: 0,
            last_activity_ms: 0,
            git_branch: None,
            host_kind: HostKind::Other,
        }
    }

    #[test]
    fn dot_is_drawn_bottom_right_only() {
        let (w, h) = (32u32, 32u32);
        let src = vec![0x10u8; (w * h * 4) as usize];
        let out = attention_dot(&src, w, h);
        assert_eq!(out.len(), src.len());
        let px = |x: u32, y: u32| out[((y * w + x) * 4) as usize..][..4].to_vec();
        assert_eq!(px(0, 0), [0x10; 4], "top-left untouched");
        assert_eq!(px(31, 0), [0x10; 4], "top-right untouched");
        assert_eq!(px(24, 24), [0xE5, 0x48, 0x4D, 0xFF], "red centre");
        assert_ne!(out, src);
        // A malformed buffer is returned unchanged.
        assert_eq!(attention_dot(&[1, 2, 3], 32, 32), [1, 2, 3]);
    }

    #[test]
    fn the_shipped_icon_gets_a_dot() {
        let out = attention_dot(ICON.rgba(), ICON.width(), ICON.height());
        assert_eq!(out.len(), ICON.rgba().len());
        assert_ne!(out, ICON.rgba());
    }

    #[test]
    fn menu_ids_are_validated() {
        assert_eq!(menu_action("show"), Some(MenuAction::ShowHide));
        assert_eq!(menu_action("dashboard"), Some(MenuAction::Dashboard));
        assert_eq!(menu_action("quit"), Some(MenuAction::Quit));
        assert_eq!(menu_action(&format!("s:{A}")), Some(MenuAction::Session(A.into())));
        for bad in ["", "s:", "s:../x", "s:0B2F6A40-1111-4C3E-9D2A-5F1E2D3C4B5A", "exit"] {
            assert_eq!(menu_action(bad), None, "{bad}");
        }
    }

    #[test]
    fn model_lists_at_most_five_waiting_sessions() {
        let mut sessions: Vec<SessionCard> =
            (0..7).map(|i| card(format!("0b2f6a40-1111-4c3e-9d2a-5f1e2d3c4b5{i}"), SessionStatus::Waiting)).collect();
        sessions.push(card(A.into(), SessionStatus::Busy));
        let snap = Snapshot { sessions, running_agents: 0, waiting_count: 7 };
        let m = tray_model(&snap, "en");
        assert_eq!((m.open, m.waiting_total, m.waiting.len()), (8, 7, MAX_WAITING_ITEMS));
        assert!(m.waiting.iter().all(|(id, _)| id != A));
    }
}
