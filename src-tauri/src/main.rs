#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod commands;
mod platform;
mod pty;

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
const INDEX_BUDGET: Duration = Duration::from_millis(50);

fn emit_snapshot(app: &tauri::AppHandle) {
    let snap = app.state::<AppState>().refresh();
    let _ = app.emit("snapshot", snap);
}

/// What the background thread does next.
#[derive(Debug, PartialEq)]
enum Action {
    Emit,
    Index,
    Wait(Duration),
}

/// Pure scheduling decision. Emits are due every TICK, or at most every
/// MIN_GAP when notify reported a change; any other time is spent indexing
/// history while work remains, otherwise waiting for the next event.
fn decide(since_emit: Duration, changed: bool, indexing: bool) -> Action {
    if since_emit >= TICK || (changed && since_emit >= MIN_GAP) {
        Action::Emit
    } else if indexing {
        Action::Index
    } else if changed {
        Action::Wait(MIN_GAP - since_emit)
    } else {
        Action::Wait(TICK - since_emit)
    }
}

fn background_loop(app: tauri::AppHandle, dir: PathBuf) {
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
    let mut last_save = Instant::now();
    let mut changed = false;
    loop {
        // Collect pending notify events without blocking.
        while rx.try_recv().is_ok() {
            changed = true;
        }
        let indexing = app.state::<AppState>().is_indexing();
        // A panic in one iteration must not silently kill the thread.
        let step = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            match decide(last_emit.elapsed(), changed, indexing) {
                Action::Emit => {
                    emit_snapshot(&app);
                    last_emit = Instant::now();
                    changed = false;
                }
                Action::Index => {
                    app.state::<AppState>().index_step(INDEX_BUDGET);
                }
                Action::Wait(d) => match rx.recv_timeout(d) {
                    Ok(()) => changed = true,
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => std::thread::sleep(d),
                },
            }
            if last_save.elapsed() >= SAVE_EVERY {
                let st = app.state::<AppState>();
                st.publish_history(); // also refreshes "today" across midnight
                if let Err(e) = st.save_history() {
                    eprintln!("geçmiş önbelleği kaydedilemedi: {e}");
                }
                last_save = Instant::now();
            }
        }));
        if step.is_err() {
            eprintln!("arka plan döngüsünde hata; devam ediliyor");
            last_emit = Instant::now(); // avoid a hot panic loop
            std::thread::sleep(Duration::from_millis(250));
        }
    }
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
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
            std::thread::spawn(move || background_loop(handle, dir));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::get_session_detail,
            commands::get_history,
            commands::open_folder,
            commands::pty_spawn,
            commands::pty_write,
            commands::pty_resize,
            commands::pty_kill,
            commands::pty_list
        ])
        .run(tauri::generate_context!())
        .expect("Claude Pano başlatılamadı");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loop_emits_when_tick_elapsed() {
        assert_eq!(decide(TICK, false, true), Action::Emit);
        assert_eq!(decide(Duration::from_millis(1500), false, false), Action::Emit);
    }

    #[test]
    fn loop_indexes_between_ticks() {
        assert_eq!(decide(Duration::from_millis(100), false, true), Action::Index);
        // Nothing to index: wait out the rest of the tick.
        assert_eq!(
            decide(Duration::from_millis(100), false, false),
            Action::Wait(Duration::from_millis(900))
        );
    }

    #[test]
    fn loop_indexes_while_changes_keep_arriving() {
        // Continuous notify events: emits are capped at 4 Hz and the time in
        // between goes to indexing, so it never starves.
        assert_eq!(decide(Duration::from_millis(100), true, true), Action::Index);
        assert_eq!(decide(MIN_GAP, true, true), Action::Emit);
        assert_eq!(
            decide(Duration::from_millis(100), true, false),
            Action::Wait(Duration::from_millis(150))
        );
    }
}
