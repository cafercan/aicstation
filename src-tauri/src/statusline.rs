//! Status line bridge: Claude Code passes the 5-hour and weekly usage limits
//! only to its `statusLine` command, so `aicstation statusline` can be set as
//! that command. It reads the status line JSON from stdin, keeps the few
//! fields the app shows in a small file in the app data dir, and then runs the
//! user's previous status line command (if any) with the same input, printing
//! its output unchanged.
//!
//! The input is untrusted: its size is capped, unknown fields are ignored,
//! every kept value is checked, and nothing from it is ever executed. The
//! only command run is the user's own previous `statusLine` command, read
//! from the bridge file the app wrote when the bridge was turned on.
use chrono::DateTime;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Bundle identifier: the app data dir is `<data dir>/<identifier>`, as Tauri
/// resolves it (the bridge runs without Tauri).
const IDENTIFIER: &str = "com.cafercan.sessionstation";
/// Largest status line input that is read.
pub const MAX_INPUT: u64 = 256 * 1024;
/// What the app shows, written by the bridge.
pub const STATUS_FILE: &str = "claude-status.json";
/// The bridge's own command and the user's previous `statusLine`, written by the app.
pub const BRIDGE_FILE: &str = "statusline-bridge.json";
/// Sessions kept in the status file (newest first).
const MAX_SESSIONS: usize = 64;
/// A session not seen for this long is dropped.
const SESSION_TTL_MS: i64 = 24 * 3600 * 1000;
/// The chained command is stopped after this long (Claude Code waits for us).
const CHAIN_TIMEOUT: Duration = Duration::from_secs(10);

/// One usage window: percent used and when it resets.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Window {
    pub used_pct: f64,
    #[serde(default)]
    pub resets_at_ms: Option<i64>,
}

/// Account-wide limits: the newest data from any session applies everywhere.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Limits {
    pub five_hour: Option<Window>,
    pub seven_day: Option<Window>,
    pub updated_ms: i64,
}

/// What only one session's status line knows.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SessionLine {
    pub effort: Option<String>,
    pub lines_added: Option<u64>,
    pub lines_removed: Option<u64>,
    pub updated_ms: i64,
}

/// Contents of [`STATUS_FILE`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Status {
    pub limits: Option<Limits>,
    pub sessions: BTreeMap<String, SessionLine>,
}

/// Contents of [`BRIDGE_FILE`].
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Bridge {
    /// The `statusLine.command` the app wrote.
    pub command: String,
    /// The user's `statusLine` before the bridge (`None`: there was none).
    pub previous: Option<Value>,
}

/// The app data dir, resolved like Tauri's `app_data_dir`.
pub fn data_dir() -> Option<PathBuf> {
    Some(dirs::data_dir()?.join(IDENTIFIER))
}

fn session_id_ok(s: &str) -> bool {
    (1..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn effort_ok(s: &str) -> bool {
    (1..=16).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// `resets_at`: Unix seconds (or milliseconds), or an RFC 3339 time.
fn reset_ms(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => {
            let x = n.as_f64().filter(|x| x.is_finite() && *x > 0.0)?;
            let ms = if x >= 1e12 { x } else { x * 1000.0 };
            (ms < 1e15).then_some(ms as i64)
        }
        Value::String(s) if s.len() <= 64 => DateTime::parse_from_rfc3339(s).ok().map(|t| t.timestamp_millis()),
        _ => None,
    }
}

fn window(v: Option<&Value>) -> Option<Window> {
    let w = v?.as_object()?;
    let used = w.get("used_percentage")?.as_f64().filter(|x| x.is_finite())?;
    Some(Window { used_pct: used.clamp(0.0, 100.0), resets_at_ms: w.get("resets_at").and_then(reset_ms) })
}

fn count(v: Option<&Value>) -> Option<u64> {
    v?.as_u64().filter(|n| *n < 1 << 40)
}

/// The fields kept from one status line input: the limits (when it carries
/// any) and the session's own line (when it names a valid session).
pub fn parse(input: &str, now_ms: i64) -> (Option<Limits>, Option<(String, SessionLine)>) {
    let Ok(Value::Object(d)) = serde_json::from_str::<Value>(input) else { return (None, None) };
    let limits = d.get("rate_limits").and_then(Value::as_object).and_then(|rl| {
        let l = Limits { five_hour: window(rl.get("five_hour")), seven_day: window(rl.get("seven_day")), updated_ms: now_ms };
        (l.five_hour.is_some() || l.seven_day.is_some()).then_some(l)
    });
    let session = d.get("session_id").and_then(Value::as_str).filter(|s| session_id_ok(s)).map(|id| {
        let cost = d.get("cost").and_then(Value::as_object);
        let effort = d.get("effort").and_then(|e| e.get("level")).and_then(Value::as_str).filter(|s| effort_ok(s));
        (id.to_string(), SessionLine {
            effort: effort.map(str::to_string),
            lines_added: count(cost.and_then(|c| c.get("total_lines_added"))),
            lines_removed: count(cost.and_then(|c| c.get("total_lines_removed"))),
            updated_ms: now_ms,
        })
    });
    (limits, session)
}

/// Adds one input's fields to `status`; drops old and surplus sessions.
pub fn merge(status: &mut Status, parsed: (Option<Limits>, Option<(String, SessionLine)>), now_ms: i64) {
    let (limits, session) = parsed;
    if let Some(l) = limits {
        status.limits = Some(l);
    }
    if let Some((id, line)) = session {
        status.sessions.insert(id, line);
    }
    status.sessions.retain(|_, s| now_ms - s.updated_ms < SESSION_TTL_MS);
    if status.sessions.len() > MAX_SESSIONS {
        let mut by_age: Vec<(i64, String)> = status.sessions.iter().map(|(k, s)| (s.updated_ms, k.clone())).collect();
        by_age.sort();
        for (_, k) in by_age.iter().take(status.sessions.len() - MAX_SESSIONS) {
            status.sessions.remove(k);
        }
    }
}

pub fn read_status(dir: &Path) -> Status {
    station_core::small_file::read_small_file(&dir.join(STATUS_FILE))
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn read_bridge(dir: &Path) -> Option<Bridge> {
    station_core::small_file::read_small_file(&dir.join(BRIDGE_FILE)).and_then(|s| serde_json::from_str(&s).ok())
}

/// Temp file next to `path` (named per process, so concurrent writers do not
/// share one), then renamed over it.
pub fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".{}.tmp", std::process::id()));
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, data).and_then(|()| std::fs::rename(&tmp, path)).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

/// Records one status line input in the status file.
pub fn record(dir: &Path, input: &str, now_ms: i64) -> std::io::Result<()> {
    let parsed = parse(input, now_ms);
    if parsed.0.is_none() && parsed.1.is_none() {
        return Ok(());
    }
    let mut status = read_status(dir);
    merge(&mut status, parsed, now_ms);
    std::fs::create_dir_all(dir)?;
    write_atomic(&dir.join(STATUS_FILE), &serde_json::to_vec(&status).map_err(std::io::Error::other)?)
}

/// The previous status line's command, if it was a command status line.
pub fn previous_command(b: &Bridge) -> Option<&str> {
    let p = b.previous.as_ref()?.as_object()?;
    if p.get("type").and_then(Value::as_str).is_some_and(|t| t != "command") {
        return None;
    }
    p.get("command").and_then(Value::as_str).filter(|c| !c.trim().is_empty())
}

/// The shell Claude Code would run a status line command with.
fn shell_command(command: &str) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        // Claude Code on Windows runs commands with Git Bash.
        let bash = std::env::var_os("CLAUDE_CODE_GIT_BASH_PATH").map(PathBuf::from).filter(|p| p.is_file()).or_else(|| {
            ["ProgramFiles", "ProgramW6432", "LOCALAPPDATA"].iter().find_map(|v| {
                let base = PathBuf::from(std::env::var_os(v)?);
                let sub = if *v == "LOCALAPPDATA" { "Programs\\Git\\bin\\bash.exe" } else { "Git\\bin\\bash.exe" };
                Some(base.join(sub)).filter(|p| p.is_file())
            })
        });
        let mut c = match bash {
            Some(b) => {
                let mut c = Command::new(b);
                c.arg("-c").arg(command);
                c
            }
            None => {
                let mut c = Command::new("cmd");
                c.arg("/C").raw_arg(command);
                c
            }
        };
        c.creation_flags(CREATE_NO_WINDOW);
        c
    }
    #[cfg(not(windows))]
    {
        let mut c = Command::new("sh");
        c.arg("-c").arg(command);
        c
    }
}

/// Runs `command` with `input` on stdin; returns its stdout and exit code.
/// Stopped after [`CHAIN_TIMEOUT`].
pub fn run_chained(command: &str, input: &[u8]) -> (Vec<u8>, i32) {
    let Ok(mut child) = shell_command(command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
    else {
        return (Vec::new(), 1);
    };
    // Write and read on their own threads: a command that does not read its
    // stdin must not block us, nor one that writes much before reading.
    let mut stdin = child.stdin.take();
    let data = input.to_vec();
    let writer = std::thread::spawn(move || {
        if let Some(s) = stdin.as_mut() {
            let _ = s.write_all(&data);
        }
    });
    let mut stdout = child.stdout.take();
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        if let Some(s) = stdout.as_mut() {
            let _ = s.take(MAX_INPUT).read_to_end(&mut out);
        }
        out
    });
    let start = Instant::now();
    let code = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st.code().unwrap_or(1),
            Ok(None) if start.elapsed() < CHAIN_TIMEOUT => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break 1;
            }
        }
    };
    let _ = writer.join();
    (reader.join().unwrap_or_default(), code)
}

/// `aicstation statusline --off` (run by the Windows uninstaller): puts the
/// user's previous status line back. Returns the exit code.
pub fn run_off() -> i32 {
    let Some(dir) = data_dir() else { return 1 };
    match disable(&claude_settings(), &dir) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("aicstation statusline --off: {e}");
            1
        }
    }
}

/// `aicstation statusline`: returns the exit code.
pub fn run_bridge() -> i32 {
    let mut input = Vec::new();
    if std::io::stdin().take(MAX_INPUT).read_to_end(&mut input).is_err() {
        return 1;
    }
    let dir = data_dir();
    let now = chrono::Utc::now().timestamp_millis();
    if let (Some(d), Ok(text)) = (&dir, std::str::from_utf8(&input)) {
        if let Err(e) = record(d, text, now) {
            eprintln!("aicstation statusline: {e}");
        }
    }
    let bridge = dir.as_deref().and_then(read_bridge).unwrap_or_default();
    let Some(cmd) = previous_command(&bridge) else { return 0 };
    let (out, code) = run_chained(cmd, &input);
    let mut so = std::io::stdout().lock();
    let _ = so.write_all(&out).and_then(|()| so.flush());
    code
}

// ---- Claude Code's settings.json -------------------------------------------

/// `command` string that runs this executable as the bridge. The path is
/// quoted for the shell Claude Code uses (sh on Linux, Git Bash or cmd on
/// Windows); a path that cannot be quoted safely for those is refused.
///
/// On Linux, when the program is gone (package removed, AppImage deleted)
/// the command runs the user's previous status line itself, so it keeps
/// working without the app. On Windows the uninstaller turns the bridge off
/// (`statusline --off`).
pub fn bridge_command(exe: &Path, previous: Option<&str>) -> Result<String, String> {
    let s = exe.to_str().ok_or("program path is not valid UTF-8")?;
    if cfg!(windows) {
        if s.contains(['"', '$', '`', '%', '!']) {
            return Err(format!("program path cannot be quoted: {s}"));
        }
        Ok(format!("\"{s}\" statusline"))
    } else {
        let q = format!("'{}'", s.replace('\'', r"'\''"));
        Ok(match previous {
            Some(p) => format!("if [ -x {q} ]; then exec {q} statusline; fi; {p}"),
            None => format!("if [ -x {q} ]; then exec {q} statusline; fi"),
        })
    }
}

/// The installed program: the AppImage file itself rather than its
/// temporary mount, otherwise this executable.
pub fn program_path() -> Option<PathBuf> {
    if let Some(a) = std::env::var_os("APPIMAGE").map(PathBuf::from).filter(|p| p.is_absolute() && p.is_file()) {
        return Some(a);
    }
    std::env::current_exe().ok()
}

/// Claude Code's settings file, followed through a symlink (dotfile
/// managers link it), so the link is kept and its target is edited.
fn real_path(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// Parses settings.json; a missing or empty file is an empty object. A file
/// that does not parse, or is not an object, is refused (never overwritten).
fn read_settings(p: &Path) -> Result<(Map<String, Value>, bool), String> {
    let text = match std::fs::read_to_string(p) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("{}: {e}", p.display())),
    };
    if text.trim().is_empty() {
        return Ok((Map::new(), true));
    }
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(m)) => Ok((m, text.ends_with('\n'))),
        Ok(_) => Err(format!("{}: not a JSON object", p.display())),
        Err(e) => Err(format!("{}: {e}", p.display())),
    }
}

/// Writes settings.json back (2-space indent like Claude Code), keeping the
/// key order and the file's permissions.
fn write_settings(p: &Path, m: Map<String, Value>, newline: bool) -> Result<(), String> {
    let mut text = serde_json::to_string_pretty(&Value::Object(m)).map_err(|e| e.to_string())?;
    if newline {
        text.push('\n');
    }
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let perms = std::fs::metadata(p).ok().map(|m| m.permissions());
    write_atomic(p, text.as_bytes()).map_err(|e| format!("{}: {e}", p.display()))?;
    if let Some(perms) = perms {
        let _ = std::fs::set_permissions(p, perms);
    }
    Ok(())
}

fn current_command(m: &Map<String, Value>) -> Option<&str> {
    m.get("statusLine")?.get("command")?.as_str()
}

/// Whether Claude Code's status line is the bridge.
pub fn is_enabled(settings: &Path, data: &Path) -> bool {
    let Some(b) = read_bridge(data) else { return false };
    read_settings(&real_path(settings)).is_ok_and(|(m, _)| current_command(&m) == Some(b.command.as_str()))
}

/// Turns the bridge on for program `exe`; see [`enable_with`].
pub fn enable(settings: &Path, data: &Path, exe: &Path) -> Result<(), String> {
    enable_with(settings, data, |prev| bridge_command(exe, prev))
}

/// Turns the bridge on: keeps the user's `statusLine` in the bridge file,
/// then points `statusLine` at the command `make` builds from the previous
/// status line's command (keeping its padding and refresh interval).
/// Already on: only the command is updated (the program moved).
pub fn enable_with(
    settings: &Path,
    data: &Path,
    make: impl Fn(Option<&str>) -> Result<String, String>,
) -> Result<(), String> {
    let settings = real_path(settings);
    let (mut m, newline) = read_settings(&settings)?;
    let known = read_bridge(data);
    let ours = known.as_ref().is_some_and(|b| current_command(&m) == Some(b.command.as_str()));
    let previous = if ours { known.and_then(|b| b.previous) } else { m.get("statusLine").cloned() };
    let command = make(previous_command(&Bridge { command: String::new(), previous: previous.clone() }))?;
    // The bridge file first: once settings.json points at the bridge, the
    // previous value must already be safe.
    std::fs::create_dir_all(data).map_err(|e| e.to_string())?;
    let b = Bridge { command: command.clone(), previous: previous.clone() };
    write_atomic(&data.join(BRIDGE_FILE), &serde_json::to_vec_pretty(&b).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let mut line = Map::new();
    line.insert("type".into(), "command".into());
    line.insert("command".into(), command.into());
    for k in ["padding", "refreshInterval"] {
        if let Some(v) = previous.as_ref().and_then(|p| p.get(k)) {
            line.insert(k.into(), v.clone());
        }
    }
    m.insert("statusLine".into(), Value::Object(line));
    write_settings(&settings, m, newline)
}

/// Turns the bridge off: puts the previous `statusLine` back (or removes
/// the key). A status line the user changed since is left alone.
pub fn disable(settings: &Path, data: &Path) -> Result<(), String> {
    let settings = real_path(settings);
    let Some(b) = read_bridge(data) else { return Ok(()) };
    let (mut m, newline) = read_settings(&settings)?;
    if current_command(&m) == Some(b.command.as_str()) {
        match b.previous {
            Some(p) => {
                m.insert("statusLine".into(), p);
            }
            None => {
                m.remove("statusLine");
            }
        }
        write_settings(&settings, m, newline)?;
    }
    std::fs::remove_file(data.join(BRIDGE_FILE)).map_err(|e| e.to_string())
}

/// The bridge is on but its command names another program path (the app was
/// moved, or a new AppImage): point it at `exe`.
pub fn refresh(settings: &Path, data: &Path, exe: &Path) -> Result<(), String> {
    let Some(b) = read_bridge(data) else { return Ok(()) };
    if bridge_command(exe, previous_command(&b))? == b.command || !is_enabled(settings, data) {
        return Ok(());
    }
    enable(settings, data, exe)
}

// ---- Commands ---------------------------------------------------------------

/// What the per-terminal Claude bars show from the status line.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeStatus {
    /// The bridge is Claude Code's status line.
    pub bridge: bool,
    pub limits: Option<Limits>,
    /// Per session; the effort comes only from here: `effortLevel` in
    /// settings.json is just the default, and a session may run another.
    pub sessions: BTreeMap<String, SessionLine>,
}

fn claude_settings() -> PathBuf {
    station_core::locate::claude_dir().join("settings.json")
}

/// Reads what the bars show (the limits only while the bridge is on, so
/// stale data is not shown after it is turned off).
pub fn status_now(data: &Path) -> ClaudeStatus {
    let settings = claude_settings();
    let bridge = is_enabled(&settings, data);
    let st = if bridge { read_status(data) } else { Status::default() };
    ClaudeStatus { bridge, limits: st.limits, sessions: st.sessions }
}

/// Turns the bridge on or off; returns whether it is on afterwards.
pub fn set_bridge(data: &Path, on: bool) -> Result<bool, String> {
    let settings = claude_settings();
    if on {
        let exe = program_path().ok_or("program path unknown")?;
        enable(&settings, data, &exe)?;
    } else {
        disable(&settings, data)?;
    }
    Ok(is_enabled(&settings, data))
}

/// At startup: an enabled bridge follows the program to its current path.
pub fn refresh_on_start(data: &Path) {
    let Some(exe) = program_path() else { return };
    if let Err(e) = refresh(&claude_settings(), data, &exe) {
        eprintln!("status line bridge: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW: i64 = 1_800_000_000_000;

    fn sample() -> String {
        json!({
            "session_id": "0b5c2c1e-6f2a-4c8e-9d1a-2f3b4c5d6e7f",
            "model": { "id": "claude-opus-5-5" },
            "effort": { "level": "high" },
            "cost": { "total_cost_usd": 1.2, "total_lines_added": 120, "total_lines_removed": 30 },
            "rate_limits": {
                "five_hour": { "used_percentage": 42.5, "resets_at": 1_800_007_800 },
                "seven_day": { "used_percentage": 59, "resets_at": "2027-01-15T10:00:00Z" },
                "future_window": { "used_percentage": 1 }
            },
            "unknown": { "nested": [1, 2, 3] }
        })
        .to_string()
    }

    #[test]
    fn parses_limits_and_the_session_line() {
        let (l, s) = parse(&sample(), NOW);
        let l = l.unwrap();
        assert_eq!(l.five_hour, Some(Window { used_pct: 42.5, resets_at_ms: Some(1_800_007_800_000) }));
        let jan15 = DateTime::parse_from_rfc3339("2027-01-15T10:00:00Z").unwrap().timestamp_millis();
        assert_eq!(l.seven_day.unwrap().resets_at_ms, Some(jan15));
        assert_eq!(l.updated_ms, NOW);
        let (id, line) = s.unwrap();
        assert_eq!(id, "0b5c2c1e-6f2a-4c8e-9d1a-2f3b4c5d6e7f");
        assert_eq!(line, SessionLine { effort: Some("high".into()), lines_added: Some(120), lines_removed: Some(30), updated_ms: NOW });
    }

    #[test]
    fn untrusted_input_is_checked() {
        assert_eq!(parse("not json", NOW), (None, None));
        assert_eq!(parse("[1,2]", NOW), (None, None));
        let bad = json!({
            "session_id": "../../etc/passwd",
            "rate_limits": { "five_hour": { "used_percentage": "50" }, "seven_day": { "used_percentage": 250, "resets_at": "soon" } },
        })
        .to_string();
        let (l, s) = parse(&bad, NOW);
        assert_eq!(s, None);
        let l = l.unwrap();
        assert_eq!(l.five_hour, None);
        assert_eq!(l.seven_day, Some(Window { used_pct: 100.0, resets_at_ms: None }));
        let odd = json!({ "session_id": "abc", "effort": { "level": "$(rm -rf ~)" }, "cost": { "total_lines_added": -5 } }).to_string();
        let (_, s) = parse(&odd, NOW);
        assert_eq!(s.unwrap().1, SessionLine { updated_ms: NOW, ..Default::default() });
        // No windows: no limits (older Claude Code, API key users).
        assert_eq!(parse(&json!({ "rate_limits": {} }).to_string(), NOW).0, None);
    }

    #[test]
    fn merge_keeps_newest_limits_and_bounds_sessions() {
        let mut st = Status::default();
        merge(&mut st, parse(&sample(), NOW), NOW);
        // An input without limits keeps the last ones.
        merge(&mut st, parse(&json!({ "session_id": "b" }).to_string(), NOW + 1), NOW + 1);
        assert_eq!(st.limits.as_ref().unwrap().updated_ms, NOW);
        assert_eq!(st.sessions.len(), 2);
        // A day later the old sessions are gone.
        merge(&mut st, parse(&json!({ "session_id": "c" }).to_string(), NOW + SESSION_TTL_MS + 1), NOW + SESSION_TTL_MS + 1);
        assert_eq!(st.sessions.keys().collect::<Vec<_>>(), ["c"]);
        for i in 0..MAX_SESSIONS + 5 {
            let t = NOW + SESSION_TTL_MS + 10 + i as i64;
            merge(&mut st, parse(&json!({ "session_id": format!("s{i}") }).to_string(), t), t);
        }
        assert_eq!(st.sessions.len(), MAX_SESSIONS);
        assert!(!st.sessions.contains_key("c") && !st.sessions.contains_key("s0") && st.sessions.contains_key(&format!("s{}", MAX_SESSIONS + 4)));
    }

    #[test]
    fn record_writes_the_status_file() {
        let d = tempfile::tempdir().unwrap();
        record(d.path(), &sample(), NOW).unwrap();
        let st = read_status(d.path());
        assert_eq!(st.limits.unwrap().five_hour.unwrap().used_pct, 42.5);
        // Nothing useful: the file is not touched.
        let e = tempfile::tempdir().unwrap();
        record(e.path(), "{}", NOW).unwrap();
        assert!(!e.path().join(STATUS_FILE).exists());
    }

    #[cfg(unix)]
    #[test]
    fn the_previous_command_gets_the_same_input_and_its_output_is_kept() {
        let (out, code) = run_chained("tr a-z A-Z; exit 3", b"hello");
        assert_eq!((out.as_slice(), code), (&b"HELLO"[..], 3));
        // A command that does not read stdin still finishes.
        let (out, code) = run_chained("printf ok", &vec![b'x'; 200_000]);
        assert_eq!((out.as_slice(), code), (&b"ok"[..], 0));
    }

    #[test]
    fn previous_command_only_for_command_status_lines() {
        let b = |p: Value| Bridge { command: "x".into(), previous: Some(p) };
        assert_eq!(previous_command(&b(json!({ "type": "command", "command": "a.sh" }))), Some("a.sh"));
        assert_eq!(previous_command(&b(json!({ "command": "a.sh" }))), Some("a.sh"));
        assert_eq!(previous_command(&b(json!({ "type": "other", "command": "a.sh" }))), None);
        assert_eq!(previous_command(&b(json!({ "type": "command", "command": "  " }))), None);
        assert_eq!(previous_command(&Bridge::default()), None);
    }

    #[test]
    fn bridge_command_quotes_the_path() {
        if cfg!(windows) {
            assert_eq!(bridge_command(Path::new(r"C:\Program Files\AICStation\aicstation.exe"), Some("x")).unwrap(),
                r#""C:\Program Files\AICStation\aicstation.exe" statusline"#);
            assert!(bridge_command(Path::new(r"C:\a$b\x.exe"), None).is_err());
        } else {
            let q = r"'/opt/my apps/it'\''s/aicstation'";
            assert_eq!(bridge_command(Path::new("/opt/my apps/it's/aicstation"), None).unwrap(),
                format!("if [ -x {q} ]; then exec {q} statusline; fi"));
            assert_eq!(bridge_command(Path::new("/opt/my apps/it's/aicstation"), Some("~/line.sh")).unwrap(),
                format!("if [ -x {q} ]; then exec {q} statusline; fi; ~/line.sh"));
        }
    }

    /// The real command through sh: the bridge runs while the program is
    /// there; once it is gone, the previous status line runs by itself.
    #[cfg(unix)]
    #[test]
    fn without_the_program_the_previous_status_line_still_runs() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let exe = d.path().join("fake app");
        std::fs::write(&exe, "#!/bin/sh\ncat >/dev/null; echo bridge\n").unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        let cmd = bridge_command(&exe, Some("tr a-z A-Z")).unwrap();
        assert_eq!(run_chained(&cmd, b"mine").0, b"bridge\n");
        std::fs::remove_file(&exe).unwrap();
        assert_eq!(run_chained(&cmd, b"mine"), (b"MINE".to_vec(), 0));
        let alone = bridge_command(&exe, None).unwrap();
        assert_eq!(run_chained(&alone, b"x").0, b"");
    }

    fn settings_with(text: &str) -> (tempfile::TempDir, PathBuf, PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let s = d.path().join("claude").join("settings.json");
        std::fs::create_dir_all(s.parent().unwrap()).unwrap();
        std::fs::write(&s, text).unwrap();
        let data = d.path().join("data");
        (d, s, data)
    }

    const USER: &str = "{\n  \"model\": \"opus\",\n  \"statusLine\": {\n    \"type\": \"command\",\n    \"command\": \"~/.claude/line.sh\",\n    \"refreshInterval\": 60\n  },\n  \"alwaysThinkingEnabled\": true\n}\n";

    #[test]
    fn enable_and_disable_restore_the_users_status_line_exactly() {
        let (_d, s, data) = settings_with(USER);
        enable_with(&s, &data, |_| Ok("'/usr/bin/aicstation' statusline".into())).unwrap();
        assert!(is_enabled(&s, &data));
        let on: Value = serde_json::from_str(&std::fs::read_to_string(&s).unwrap()).unwrap();
        assert_eq!(on["statusLine"], json!({ "type": "command", "command": "'/usr/bin/aicstation' statusline", "refreshInterval": 60 }));
        // Other keys and their order stay.
        assert_eq!(on.as_object().unwrap().keys().collect::<Vec<_>>(), ["model", "statusLine", "alwaysThinkingEnabled"]);
        assert_eq!(previous_command(&read_bridge(&data).unwrap()), Some("~/.claude/line.sh"));
        // Turning it on again does not lose the previous value.
        enable_with(&s, &data, |_| Ok("'/usr/bin/aicstation' statusline".into())).unwrap();
        disable(&s, &data).unwrap();
        assert_eq!(std::fs::read_to_string(&s).unwrap(), USER);
        assert!(!is_enabled(&s, &data) && read_bridge(&data).is_none());
    }

    #[test]
    fn without_a_previous_status_line_disable_removes_the_key() {
        let (_d, s, data) = settings_with("{\"model\":\"opus\"}");
        enable_with(&s, &data, |_| Ok("bridge".into())).unwrap();
        assert_eq!(read_bridge(&data).unwrap().previous, None);
        disable(&s, &data).unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&s).unwrap()).unwrap();
        assert_eq!(v, json!({ "model": "opus" }));
        // No settings file at all.
        let d = tempfile::tempdir().unwrap();
        let s = d.path().join("settings.json");
        enable_with(&s, &d.path().join("data"), |_| Ok("bridge".into())).unwrap();
        assert!(is_enabled(&s, &d.path().join("data")));
    }

    #[test]
    fn a_settings_file_that_does_not_parse_is_never_touched() {
        let (_d, s, data) = settings_with("{ \"model\": \"opus\", }");
        assert!(enable_with(&s, &data, |_| Ok("bridge".into())).is_err());
        assert_eq!(std::fs::read_to_string(&s).unwrap(), "{ \"model\": \"opus\", }");
        assert!(read_bridge(&data).is_none());
        let (_d, s, data) = settings_with("[]");
        assert!(enable_with(&s, &data, |_| Ok("bridge".into())).is_err());
    }

    #[test]
    fn a_status_line_changed_by_the_user_is_left_alone() {
        let (_d, s, data) = settings_with(USER);
        enable_with(&s, &data, |_| Ok("bridge".into())).unwrap();
        let mine = "{\n  \"statusLine\": {\n    \"type\": \"command\",\n    \"command\": \"new.sh\"\n  }\n}\n";
        std::fs::write(&s, mine).unwrap();
        assert!(!is_enabled(&s, &data));
        disable(&s, &data).unwrap();
        assert_eq!(std::fs::read_to_string(&s).unwrap(), mine);
        assert!(read_bridge(&data).is_none());
    }

    #[test]
    fn refresh_follows_a_moved_program() {
        let (_d, s, data) = settings_with(USER);
        let (old, new) = (Path::new("/old/aicstation"), Path::new("/new/aicstation"));
        enable(&s, &data, old).unwrap();
        refresh(&s, &data, old).unwrap();
        assert_eq!(read_bridge(&data).unwrap().command, bridge_command(old, Some("~/.claude/line.sh")).unwrap());
        refresh(&s, &data, new).unwrap();
        assert_eq!(read_bridge(&data).unwrap().command, bridge_command(new, Some("~/.claude/line.sh")).unwrap());
        assert!(is_enabled(&s, &data));
        disable(&s, &data).unwrap();
        assert_eq!(std::fs::read_to_string(&s).unwrap(), USER);
        // Off: refresh does nothing.
        refresh(&s, &data, new).unwrap();
        assert!(read_bridge(&data).is_none());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_settings_file_stays_a_link() {
        let (d, real, data) = settings_with(USER);
        let link = d.path().join("link.json");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        enable_with(&link, &data, |_| Ok("bridge".into())).unwrap();
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert!(is_enabled(&link, &data));
        disable(&link, &data).unwrap();
        assert_eq!(std::fs::read_to_string(&real).unwrap(), USER);
    }
}
