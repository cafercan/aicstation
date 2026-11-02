use std::fs;
use std::path::PathBuf;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(name: &str) -> String {
    fs::read_to_string(manifest_dir().join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn csp_has_no_remote_origins() {
    let conf: serde_json::Value = serde_json::from_str(&read("tauri.conf.json")).unwrap();
    let csp = conf["app"]["security"]["csp"]
        .as_str()
        .expect("app.security.csp must be set");
    // Only the Tauri IPC origin may appear with a scheme.
    let stripped = csp.replace("http://ipc.localhost", "");
    assert!(!stripped.contains("http://"), "remote http origin in CSP: {csp}");
    assert!(!stripped.contains("https://"), "remote https origin in CSP: {csp}");
    assert!(csp.contains("connect-src ipc: http://ipc.localhost"));
}

#[test]
fn no_updater_or_network_plugins() {
    let cargo = read("Cargo.toml");
    for banned in [
        "tauri-plugin-updater",
        "tauri-plugin-http",
        "tauri-plugin-shell",
        "reqwest",
    ] {
        assert!(!cargo.contains(banned), "banned dependency: {banned}");
    }
    let conf: serde_json::Value = serde_json::from_str(&read("tauri.conf.json")).unwrap();
    assert!(conf["plugins"]["updater"].is_null(), "plugins.updater must not exist");
}
