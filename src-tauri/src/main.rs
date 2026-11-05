#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod alerts;
mod butaglyph;
mod commands;
mod errors;
mod gitstat;
mod integration;
mod native_text;
mod notifier;
mod platform;
mod pty;
mod settings;
mod shell;
mod statusline;
mod toast;
mod tray;
#[cfg(windows)]
mod wt;

use commands::AppState;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use station_core::locate::claude_dir;
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};
use tauri::{Emitter, Manager};

const TICK: Duration = Duration::from_secs(1);
const MIN_GAP: Duration = Duration::from_millis(250); // max 4 Hz
const SAVE_EVERY: Duration = Duration::from_secs(30);
const RESCAN_EVERY: Duration = Duration::from_secs(15);
const INDEX_BUDGET: Duration = Duration::from_millis(50);
/// Idle wait of the indexer when there is nothing to index.
const INDEX_IDLE: Duration = Duration::from_secs(1);
const MAX_INDEX_PANICS: u32 = 3;

/// Refreshes, lets the resume holds follow the live set, emits, then runs
/// the notifications and the tray off the same snapshot.
fn emit_snapshot(app: &tauri::AppHandle) {
    let snap = app.state::<AppState>().refresh();
    let live: std::collections::HashSet<String> = snap.sessions.iter().map(|s| s.session_id.clone()).collect();
    app.state::<pty::PtyManager>().observe_live(&live);
    let _ = app.emit("snapshot", &snap);
    notifier::after_snapshot(app, &snap);
}

/// What the snapshot thread does next.
#[derive(Debug, PartialEq)]
enum Action {
    Emit,
    Wait(Duration),
}

/// Pure scheduling decision of the snapshot thread. Emits are due every TICK,
/// or at most every MIN_GAP when notify reported a change; otherwise it waits
/// for the next event. Indexing runs on its own thread and never delays this.
fn decide(since_emit: Duration, changed: bool) -> Action {
    if since_emit >= TICK || (changed && since_emit >= MIN_GAP) {
        Action::Emit
    } else if changed {
        Action::Wait(MIN_GAP - since_emit)
    } else {
        Action::Wait(TICK - since_emit)
    }
}

/// What the indexer thread does next.
#[derive(Debug, PartialEq)]
enum IndexAction {
    Rescan,
    Step,
    Idle(Duration),
}

/// Pure scheduling decision of the indexer thread: rescan every RESCAN_EVERY,
/// index while work remains, otherwise idle until the next rescan is due.
fn decide_index(since_rescan: Duration, indexing: bool) -> IndexAction {
    if since_rescan >= RESCAN_EVERY {
        IndexAction::Rescan
    } else if indexing {
        IndexAction::Step
    } else {
        IndexAction::Idle(INDEX_IDLE.min(RESCAN_EVERY - since_rescan))
    }
}

/// Panic bookkeeping of the indexer: true when indexing must be given up.
fn index_panicked(panics: &mut u32) -> bool {
    *panics += 1;
    *panics >= MAX_INDEX_PANICS
}

fn snapshot_loop(app: tauri::AppHandle, dir: PathBuf) {
    let (tx, rx) = mpsc::channel::<()>();
    let mut watcher: Option<RecommendedWatcher> =
        notify::recommended_watcher(move |_res: notify::Result<notify::Event>| {
            let _ = tx.send(());
        })
        .ok();
    if let Some(w) = watcher.as_mut() {
        for sub in ["sessions", "projects"] {
            let _ = w.watch(&dir.join(sub), RecursiveMode::Recursive);
        }
    }
    let mut last_emit = Instant::now() - TICK;
    let mut changed = false;
    loop {
        while rx.try_recv().is_ok() {
            changed = true;
        }
        // A panic in one iteration must not silently kill the thread.
        let step = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            match decide(last_emit.elapsed(), changed) {
                Action::Emit => {
                    emit_snapshot(&app);
                    last_emit = Instant::now();
                    changed = false;
                }
                Action::Wait(d) => match rx.recv_timeout(d) {
                    Ok(()) => changed = true,
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => std::thread::sleep(d),
                },
            }
        }));
        if step.is_err() {
            eprintln!("anlık görüntü döngüsünde hata; devam ediliyor");
            std::thread::sleep(MIN_GAP);
        }
    }
}

/// History indexing (rescan, step, publish, periodic save) on its own thread
/// so a large transcript never delays snapshot emits.
fn indexer_loop(app: tauri::AppHandle) {
    let mut last_rescan = Instant::now() - RESCAN_EVERY;
    let mut last_save = Instant::now();
    let mut panics = 0u32;
    loop {
        let st = app.state::<AppState>();
        let step = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            match decide_index(last_rescan.elapsed(), st.is_indexing()) {
                IndexAction::Rescan => {
                    st.rescan_history();
                    last_rescan = Instant::now();
                }
                IndexAction::Step => {
                    st.index_step(INDEX_BUDGET);
                }
                IndexAction::Idle(d) => std::thread::sleep(d),
            }
            if last_save.elapsed() >= SAVE_EVERY {
                st.publish_history(); // also refreshes "today" across midnight
                if let Err(e) = st.save_history_if_dirty() {
                    eprintln!("geçmiş önbelleği kaydedilemedi: {e}");
                }
                last_save = Instant::now();
            }
        }));
        match step {
            Ok(()) => panics = 0,
            Err(_) => {
                eprintln!("indeksleme hatası; devam ediliyor");
                if index_panicked(&mut panics) {
                    eprintln!("indeksleme tekrar tekrar çöktü; bu oturumda durduruldu");
                    st.disable_indexing();
                    return;
                }
                std::thread::sleep(MIN_GAP);
            }
        }
    }
}

/// The dev server URL the webview may navigate to: only under `tauri dev`
/// (`is_dev`, i.e. tauri's `custom-protocol` feature is off). `tauri build
/// --debug` (debug-preview) loads the bundled assets like a release build, so
/// `debug_assertions` alone must not open the dev origin.
fn dev_origin(is_dev: bool, dev_url: Option<tauri::Url>) -> Option<tauri::Url> {
    dev_url.filter(|_| is_dev)
}

/// `window.open` / `target=_blank` never open a window, whatever the URL.
/// wry 0.57 on WebView2 already marks a new-window request handled (no
/// window) when no handler is set; the explicit deny keeps that from
/// depending on a runtime default (WebView2's own default is a popup that
/// bypasses the navigation guard).
fn new_window_allowed(_url: &tauri::Url) -> bool {
    false
}

/// The app's own origin (`tauri://localhost`, `http(s)://tauri.localhost`)
/// and, under `tauri dev`, the dev server's origin. Everything else is refused.
fn navigation_allowed(url: &tauri::Url, dev: Option<&tauri::Url>) -> bool {
    let app_origin = match url.scheme() {
        "tauri" => url.host_str() == Some("localhost"),
        "http" | "https" => url.host_str() == Some("tauri.localhost") && url.port().is_none(),
        _ => false,
    };
    app_origin || dev.is_some_and(|d| url.origin() == d.origin())
}

/// Icon of the toasts (registered with the AppUserModelID).
const TOAST_ICON: &[u8] = include_bytes!("../icons/64x64.png");

/// Writes the toast icon into the app data dir when missing or different.
fn toast_icon(data: &std::path::Path) -> Option<PathBuf> {
    let p = data.join("toast-icon.png");
    if std::fs::read(&p).ok().as_deref() != Some(TOAST_ICON) {
        std::fs::write(&p, TOAST_ICON).ok()?;
    }
    Some(p)
}

/// Bundle identifier (tauri.conf.json); also names the single-instance objects.
#[cfg(windows)]
const APP_ID: &str = "com.cafercan.sessionstation";

/// settings.json before Tauri runs: Tauri's app data dir on Windows is
/// `%APPDATA%\<identifier>` (the roaming known folder). Only used to pick the
/// message box language, so a wrong guess just means English.
#[cfg(windows)]
fn settings_path_guess() -> Option<PathBuf> {
    let appdata = PathBuf::from(std::env::var_os("APPDATA")?);
    appdata.is_absolute().then(|| appdata.join(APP_ID).join("settings.json"))
}

fn main() {
    // `aicstation statusline`: Claude Code's status line command (no window,
    // no single-instance check; it runs once per status line update).
    // `statusline --off` is run by the Windows uninstaller.
    if std::env::args_os().nth(1).is_some_and(|a| a == "statusline") {
        let off = std::env::args_os().nth(2).is_some_and(|a| a == "--off");
        std::process::exit(if off { statusline::run_off() } else { statusline::run_bridge() });
    }
    // A second launch brings the running instance forward and exits; when
    // that instance does not answer, the user is told instead.
    #[cfg(windows)]
    let activation = match platform::single::acquire(APP_ID) {
        platform::single::Instance::First(a) => a,
        platform::single::Instance::Second { responded: true } => return,
        platform::single::Instance::Second { responded: false } => {
            let lang = settings_path_guess().and_then(|p| settings::stored_lang(&p));
            platform::single::message_box("AI C-Station", native_text::already_running(lang.as_deref()));
            return;
        }
    };
    let builder = tauri::Builder::default();
    // A second launch hands over to the running instance (over D-Bus), which
    // shows its window; the new process then exits.
    #[cfg(target_os = "linux")]
    let builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| tray::show_main(app)));
    builder
        .setup(move |app| {
            let data = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data)?;
            let settings = settings::SettingsStore::open(data.join("settings.json"));
            butaglyph::set_preset_dir(data.join("prompts"));
            statusline::refresh_on_start(&data);
            // "Show Claude usage limits" is on by default: turned on once, at
            // the first start of a release build (a dev build would point
            // Claude Code at target/debug). A settings.json that does not
            // parse is left alone and tried again next time.
            if !cfg!(debug_assertions) && !settings.get().usage_bridge_offered {
                match statusline::set_bridge(&data, true) {
                    Ok(_) => settings.mark_usage_bridge_offered(),
                    Err(e) => eprintln!("status line bridge: {e}"),
                }
            }
            if let Err(e) = integration::install(&data.join("shell-integration")) {
                eprintln!("kabuk entegrasyonu yazılamadı: {e}");
            }
            let lang = settings.get().lang;
            app.manage(settings);
            app.manage(notifier::Notifier::default());
            app.manage(tray::TrayState::default());
            app.manage(toast::Toaster::new(
                &app.config().identifier,
                "AI C-Station", // display name; productName stays AICStation for bundle file names
                toast_icon(&data).as_deref(),
            ));
            let dir = claude_dir();
            app.manage(AppState::new(
                dir.clone(),
                data.join("history-cache.json"),
                platform::probe(),
            ));
            app.manage(pty::PtyManager::new());
            // The main window is built here (config has `create: false`) only
            // to enable WebView2 clipboard-read access, so right-click paste
            // via navigator.clipboard.readText works without a prompt. No
            // clipboard plugin and no new IPC permission are involved. It is
            // built after the state is managed (commands may run at once) and
            // may only navigate within the app (it can read the clipboard).
            let main_conf = app
                .config()
                .app
                .windows
                .iter()
                .find(|w| w.label == "main")
                .cloned()
                .ok_or("main penceresi yapılandırmada yok")?;
            let dev_url = dev_origin(tauri::is_dev(), app.config().build.dev_url.clone());
            tauri::WebviewWindowBuilder::from_config(app.handle(), &main_conf)?
                .enable_clipboard_access()
                .on_navigation(move |url| navigation_allowed(url, dev_url.as_ref()))
                .on_new_window(|url, _features| {
                    if new_window_allowed(&url) {
                        tauri::webview::NewWindowResponse::Allow
                    } else {
                        tauri::webview::NewWindowResponse::Deny
                    }
                })
                .build()?;
            tray::create(app.handle(), &lang)?;
            #[cfg(windows)]
            {
                let handle = app.handle().clone();
                activation.listen(move |ack| {
                    // Answer from the event loop, so a hung UI does not count as alive.
                    let h = handle.clone();
                    let _ = handle.run_on_main_thread(move || {
                        tray::show_main(&h);
                        ack.done();
                    });
                });
            }
            let handle = app.handle().clone();
            std::thread::spawn(move || snapshot_loop(handle, dir));
            let handle = app.handle().clone();
            std::thread::spawn(move || indexer_loop(handle));
            Ok(())
        })
        .on_window_event(|w, ev| {
            if let (tauri::WindowEvent::Focused(f), "main") = (ev, w.label()) {
                tray::on_main_focus(w.app_handle(), *f);
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::get_session_detail,
            commands::get_history,
            commands::open_folder,
            commands::pty_spawn,
            commands::pty_spawn_resume,
            commands::pty_write,
            commands::pty_resize,
            commands::pty_kill,
            commands::pty_list,
            commands::open_system_terminal,
            commands::list_shells,
            commands::butaglyph_info,
            commands::butaglyph_preview,
            commands::open_butaglyph_editor,
            commands::focus_session_tab,
            commands::get_settings,
            commands::set_settings,
            commands::set_focus_context,
            commands::hide_to_tray,
            commands::search_history,
            commands::cancel_search,
            commands::claude_status,
            commands::set_usage_bridge,
            commands::git_dirty
        ])
        .build(tauri::generate_context!())
        .expect("AI C-Station başlatılamadı")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                app.state::<pty::PtyManager>().kill_all();
                app.state::<AppState>().save_history_on_exit();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(s: &str) -> tauri::Url {
        tauri::Url::parse(s).unwrap()
    }

    #[test]
    fn navigation_only_within_the_app() {
        for ok in ["tauri://localhost/", "tauri://localhost/index.html", "http://tauri.localhost/", "https://tauri.localhost/x"] {
            assert!(navigation_allowed(&u(ok), None), "{ok}");
        }
        for bad in [
            "https://example.com/",
            "http://tauri.localhost.example.com/",
            "http://tauri.localhost:8080/",
            "tauri://evil/",
            "file:///C:/Windows/win.ini",
            "http://localhost:1420/",
            "javascript:alert(1)",
        ] {
            assert!(!navigation_allowed(&u(bad), None), "{bad}");
        }
        let dev = u("http://localhost:1420");
        assert!(navigation_allowed(&u("http://localhost:1420/src/main.tsx"), Some(&dev)));
        assert!(!navigation_allowed(&u("http://localhost:1421/"), Some(&dev)));
    }

    #[test]
    fn dev_server_origin_only_under_tauri_dev() {
        let dev = Some(u("http://localhost:1420"));
        // `tauri dev`: the UI comes from the dev server.
        assert_eq!(dev_origin(true, dev.clone()), dev);
        // `tauri build` (release or --debug, e.g. debug-preview): bundled
        // assets, so the dev origin is refused even with debug_assertions.
        assert_eq!(dev_origin(false, dev), None);
        assert_eq!(dev_origin(true, None), None);
        assert!(!navigation_allowed(&u("http://localhost:1420/"), dev_origin(false, Some(u("http://localhost:1420"))).as_ref()));
    }

    #[test]
    fn new_windows_are_never_opened() {
        for url in [
            "https://example.com/",
            "tauri://localhost/",
            "http://tauri.localhost/",
            "http://localhost:1420/",
            "about:blank",
            "javascript:alert(1)",
        ] {
            assert!(!new_window_allowed(&u(url)), "{url}");
        }
    }

    #[test]
    fn loop_emits_when_tick_elapsed() {
        assert_eq!(decide(TICK, false), Action::Emit);
        assert_eq!(decide(Duration::from_millis(1500), false), Action::Emit);
    }

    #[test]
    fn loop_waits_out_the_tick_without_changes() {
        assert_eq!(decide(Duration::from_millis(100), false), Action::Wait(Duration::from_millis(900)));
    }

    #[test]
    fn loop_caps_emits_at_4hz_while_changes_keep_arriving() {
        assert_eq!(decide(MIN_GAP, true), Action::Emit);
        assert_eq!(decide(Duration::from_millis(100), true), Action::Wait(Duration::from_millis(150)));
    }

    #[test]
    fn indexer_rescans_first_then_steps_while_work_remains() {
        // First iteration: last_rescan starts RESCAN_EVERY in the past.
        assert_eq!(decide_index(RESCAN_EVERY, false), IndexAction::Rescan);
        assert_eq!(decide_index(RESCAN_EVERY, true), IndexAction::Rescan);
        assert_eq!(decide_index(Duration::from_secs(1), true), IndexAction::Step);
    }

    #[test]
    fn indexer_idles_until_next_rescan_when_done() {
        assert_eq!(decide_index(Duration::from_secs(1), false), IndexAction::Idle(INDEX_IDLE));
        assert_eq!(
            decide_index(RESCAN_EVERY - Duration::from_millis(300), false),
            IndexAction::Idle(Duration::from_millis(300))
        );
    }

    #[test]
    fn indexer_gives_up_after_max_panics() {
        let mut p = 0;
        for _ in 1..MAX_INDEX_PANICS {
            assert!(!index_panicked(&mut p));
        }
        assert!(index_panicked(&mut p));
    }
}
