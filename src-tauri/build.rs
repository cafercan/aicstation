/// The app's own IPC commands. Declared in the AppManifest so each one gets
/// an `allow-<command>` permission and the ACL gates it like a plugin command;
/// capabilities/default.json grants exactly these to the main window.
/// Must list every command of `generate_handler!` in main.rs (config_guard).
const COMMANDS: &[&str] = &[
    "get_snapshot",
    "get_session_detail",
    "get_history",
    "open_folder",
    "pty_spawn",
    "pty_spawn_resume",
    "pty_write",
    "pty_resize",
    "pty_kill",
    "pty_list",
    "open_windows_terminal",
    "focus_session_tab",
    "get_settings",
    "set_settings",
    "set_focus_context",
    "hide_to_tray",
    "search_history",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new().app_manifest(tauri_build::AppManifest::new().commands(COMMANDS)),
    )
    .expect("tauri build script failed")
}
