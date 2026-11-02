#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod commands;
mod platform;

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
    loop {
        let wait = TICK.saturating_sub(last_emit.elapsed()).max(Duration::from_millis(1));
        let changed = match rx.recv_timeout(wait) {
            Ok(()) => {
                while rx.try_recv().is_ok() {}
                true
            }
            Err(RecvTimeoutError::Timeout) => false,
            Err(RecvTimeoutError::Disconnected) => {
                std::thread::sleep(wait);
                false
            }
        };
        let since = last_emit.elapsed();
        if since >= TICK || (changed && since >= MIN_GAP) {
            emit_snapshot(&app);
            last_emit = Instant::now();
        } else if changed {
            // Debounce: wait out the 4 Hz gap, then emit once.
            std::thread::sleep(MIN_GAP - since);
            emit_snapshot(&app);
            last_emit = Instant::now();
        } else {
            // Idle: no lock is held across the emit above; indexing locks only the history.
            app.state::<AppState>().index_step(INDEX_BUDGET);
        }
        if last_save.elapsed() >= SAVE_EVERY {
            if let Err(e) = app.state::<AppState>().save_history() {
                eprintln!("geçmiş önbelleği kaydedilemedi: {e}");
            }
            last_save = Instant::now();
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
            let handle = app.handle().clone();
            std::thread::spawn(move || background_loop(handle, dir));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_snapshot,
            commands::get_session_detail,
            commands::get_history,
            commands::open_folder
        ])
        .run(tauri::generate_context!())
        .expect("Claude Pano başlatılamadı");
}
