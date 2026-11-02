#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod commands;
mod platform;
mod pty;
mod wt;

use commands::AppState;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use pano_core::locate::claude_dir;
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

fn emit_snapshot(app: &tauri::AppHandle) {
    let snap = app.state::<AppState>().refresh();
    let _ = app.emit("snapshot", snap);
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

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            // The main window is built here (config has `create: false`) only
            // to enable WebView2 clipboard-read access, so right-click paste
            // via navigator.clipboard.readText works without a prompt. No
            // clipboard plugin and no new IPC permission are involved.
            let main_conf = app
                .config()
                .app
                .windows
                .iter()
                .find(|w| w.label == "main")
                .cloned()
                .ok_or("main penceresi yapılandırmada yok")?;
            tauri::WebviewWindowBuilder::from_config(app.handle(), &main_conf)?
                .enable_clipboard_access()
                .build()?;
            let data = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data)?;
            let dir = claude_dir();
            app.manage(AppState::new(
                dir.clone(),
                data.join("history-cache.json"),
                platform::probe(),
            ));
            app.manage(pty::PtyManager::new());
            let handle = app.handle().clone();
            std::thread::spawn(move || snapshot_loop(handle, dir));
            let handle = app.handle().clone();
            std::thread::spawn(move || indexer_loop(handle));
            Ok(())
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
            commands::open_windows_terminal
        ])
        .build(tauri::generate_context!())
        .expect("Claude Pano başlatılamadı")
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
