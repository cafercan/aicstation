use std::fs;
use std::path::PathBuf;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(name: &str) -> String {
    fs::read_to_string(manifest_dir().join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}

const ALLOWED_SOURCES: [&str; 6] = [
    "'self'",
    "'unsafe-inline'",
    "'none'",
    "data:",
    "ipc:",
    "http://ipc.localhost",
];

/// Every source token of every directive must be on the allow-list.
/// Directive names are the first token of each `;`-separated segment.
fn check_csp(csp: &str) -> Result<(), String> {
    let mut directives = 0;
    for segment in csp.split(';') {
        let mut tokens = segment.split_whitespace();
        let Some(name) = tokens.next() else { continue };
        directives += 1;
        for tok in tokens {
            if !ALLOWED_SOURCES.contains(&tok) {
                return Err(format!("disallowed source `{tok}` in directive `{name}`"));
            }
        }
    }
    if directives == 0 {
        return Err("empty CSP".into());
    }
    Ok(())
}

#[test]
fn csp_has_no_remote_origins() {
    let conf: serde_json::Value = serde_json::from_str(&read("tauri.conf.json")).unwrap();
    let csp = conf["app"]["security"]["csp"]
        .as_str()
        .expect("app.security.csp must be set");
    check_csp(csp).unwrap();
    assert_eq!(csp, PLAN_CSP, "CSP must equal the plan string exactly");
}

/// The CSP fixed by the plan; any change must be a deliberate plan change.
const PLAN_CSP: &str = "default-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self'; connect-src ipc: http://ipc.localhost; img-src 'self' data:";

#[test]
fn csp_checker_rejects_bad_inputs() {
    for bad in [
        "default-src 'self'; connect-src http://ipc.localhost.evil.com",
        "default-src 'self'; connect-src wss://x",
        "default-src 'self'; connect-src *",
        "default-src 'self'; img-src evil.com",
        "default-src 'self'; img-src https://x",
        "default-src 'self'; img-src ftp://x",
        "",
    ] {
        assert!(check_csp(bad).is_err(), "should reject: {bad}");
    }
    let real = "default-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self'; connect-src ipc: http://ipc.localhost; img-src 'self' data:";
    assert!(check_csp(real).is_ok());
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

/// The main window is not created from config (`create: false`); main.rs
/// builds it so WebView2 clipboard-read is enabled without a plugin. Both
/// halves must stay together or the app starts without a window.
#[test]
fn main_window_built_in_code_with_clipboard_access() {
    let conf: serde_json::Value = serde_json::from_str(&read("tauri.conf.json")).unwrap();
    let main = conf["app"]["windows"]
        .as_array()
        .and_then(|w| w.iter().find(|w| w["label"] == "main"))
        .expect("main window config");
    assert_eq!(main["create"], serde_json::Value::Bool(false));
    let src = read("src/main.rs");
    assert!(src.contains("WebviewWindowBuilder::from_config"));
    assert!(src.contains(".enable_clipboard_access()"));
    assert!(!read("Cargo.toml").contains("tauri-plugin-clipboard"), "no clipboard plugin");
}

/// Commands registered with `generate_handler!` in main.rs, in order.
fn handler_commands() -> Vec<String> {
    let src = read("src/main.rs");
    let start = src.find("generate_handler![").expect("generate_handler! in main.rs") + "generate_handler![".len();
    let end = start + src[start..].find(']').unwrap();
    src[start..end]
        .split(',')
        .map(|s| s.trim().trim_start_matches("commands::").to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Commands declared in build.rs's AppManifest (`COMMANDS`).
fn manifest_commands() -> Vec<String> {
    let src = read("build.rs");
    let start = src.find("COMMANDS: &[&str] = &[").expect("COMMANDS in build.rs") + "COMMANDS: &[&str] = &[".len();
    let end = start + src[start..].find(']').unwrap();
    src[start..end]
        .split(',')
        .map(|s| s.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Every app command is declared in the AppManifest, so the ACL gates it.
#[test]
fn app_commands_are_declared_in_the_app_manifest() {
    let mut handlers = handler_commands();
    let mut manifest = manifest_commands();
    assert!(!handlers.is_empty());
    handlers.sort();
    manifest.sort();
    assert_eq!(manifest, handlers);
    assert!(read("build.rs").contains(".app_manifest("), "build.rs passes the manifest to tauri_build");
}

/// The capability grants exactly what the UI uses: event listen/unlisten,
/// window destroy (close button) and the app's own commands. No `core:default`.
#[test]
fn capability_is_narrowed_to_what_the_ui_uses() {
    let cap: serde_json::Value = serde_json::from_str(&read("capabilities/default.json")).unwrap();
    assert_eq!(cap["windows"], serde_json::json!(["main"]));
    let mut got: Vec<String> = cap["permissions"]
        .as_array()
        .expect("permissions array")
        .iter()
        .map(|p| p.as_str().expect("plain permission identifiers").to_string())
        .collect();
    let mut want: Vec<String> = ["core:event:allow-listen", "core:event:allow-unlisten", "core:window:allow-destroy"]
        .iter()
        .map(|s| s.to_string())
        .chain(handler_commands().iter().map(|c| format!("allow-{}", c.replace('_', "-"))))
        .collect();
    got.sort();
    want.sort();
    assert_eq!(got, want);
}

/// Product identity: "SessionStation", never a "Claude" product name.
#[test]
fn product_is_session_station() {
    let conf: serde_json::Value = serde_json::from_str(&read("tauri.conf.json")).unwrap();
    assert_eq!(conf["productName"], "SessionStation");
    assert_eq!(conf["identifier"], "com.cafercan.sessionstation");
    let main = conf["app"]["windows"].as_array().unwrap().iter().find(|w| w["label"] == "main").unwrap();
    assert_eq!(main["title"], "SessionStation");
    let cargo = read("Cargo.toml");
    assert!(cargo.contains("name = \"session-station\""), "binary is session-station.exe");
    assert!(cargo.contains("station-core = { path = \"../core\" }"));
    for text in [conf.to_string(), cargo] {
        assert!(!text.to_lowercase().contains("claude pano") && !text.contains("claude-pano") && !text.contains("claudepano"));
    }
}
