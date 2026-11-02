//! ConPTY terminal manager. Independent of the webview: output and exit are
//! delivered through plain callbacks; the Tauri commands wrap them in Channels.

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use pano_core::locate::is_session_id;
use std::ffi::{OsStr, OsString};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

pub const MAX_PTYS: usize = 32;
pub const MAX_WRITE: usize = 64 * 1024;
const MIN_DIM: u16 = 1;
const MAX_DIM: u16 = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PtyInfo {
    pub id: u32,
    pub pid: u32,
}

struct Entry {
    pid: u32,
    master: Box<dyn MasterPty + Send>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    killer: Box<dyn ChildKiller + Send + Sync>,
}

#[derive(Default)]
struct Inner {
    next_id: u32,
    ptys: HashMap<u32, Entry>,
}

#[derive(Default)]
pub struct PtyManager {
    inner: Arc<Mutex<Inner>>,
    /// Session ids resumed by a terminal that has not exited yet: a second
    /// resume before the new `claude` registers would run the session twice.
    resumed: Arc<Mutex<HashSet<String>>>,
}

fn size(cols: u16, rows: u16) -> PtySize {
    PtySize {
        rows: rows.clamp(MIN_DIM, MAX_DIM),
        cols: cols.clamp(MIN_DIM, MAX_DIM),
        pixel_width: 0,
        pixel_height: 0,
    }
}

fn unknown(id: u32) -> String {
    format!("bilinmeyen terminal: {id}")
}

fn poisoned<T>(_: T) -> String {
    "iç kilit hatası".to_string()
}

/// Absolute path of Windows PowerShell (never resolved through PATH).
fn powershell_exe() -> PathBuf {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    PathBuf::from(root).join("System32").join("WindowsPowerShell").join("v1.0").join("powershell.exe")
}

/// PowerShell arguments for resuming a session. Only a lowercase UUID is
/// accepted, so the `-Command` text can never carry anything else.
pub fn resume_args(session_id: &str) -> Result<Vec<String>, String> {
    if !is_session_id(session_id) {
        return Err("Geçersiz oturum kimliği".into());
    }
    Ok(vec![
        "-NoLogo".into(),
        "-NoExit".into(),
        "-Command".into(),
        format!("claude --resume {session_id}"),
    ])
}

/// Set in every child: cmd.exe then never runs a program (e.g. a planted
/// `node.exe`) from the current directory, which may be an untrusted folder
/// of a resumed session.
pub const NO_CWD_EXE: &str = "NoDefaultCurrentDirectoryInExePath";

/// Variables a running Claude Code session sets for itself and its children.
/// Inherited, they make `claude` in the terminal act as a child of that
/// session (no transcript, wrong session id), so they are always removed.
const SESSION_MARKERS: &[&str] = &[
    "CLAUDECODE",
    "CLAUDE_SESSION_ID",
    "CLAUDE_PID",
    "CLAUDE_CODE_ENTRYPOINT",
    "CLAUDE_CODE_SESSION_ID",
    "CLAUDE_CODE_CHILD_SESSION",
    "CLAUDE_CODE_SSE_PORT",
    "CLAUDE_CODE_BRIDGE_SESSION_ID",
    "CLAUDE_CODE_SESSION_ATTENDED",
];
const SESSION_MARKER_PREFIXES: &[&str] = &["CLAUDE_CODE_MESSAGING_"];

/// The child's environment, from this process's environment. Names are
/// compared case-insensitively, as on Windows; `persistent` holds the
/// uppercased names of the user's and machine's persistent variables.
/// - Session markers are always removed.
/// - Other `CLAUDE_CODE_*` and `CLAUDE_EFFORT` are the user's own settings
///   when persistent (kept), otherwise they were set by a parent Claude Code
///   session (removed).
/// - Everything else is kept, including NO_COLOR / FORCE_COLOR.
/// - `NoDefaultCurrentDirectoryInExePath=1` is added (see `NO_CWD_EXE`).
pub fn child_env(
    vars: impl IntoIterator<Item = (OsString, OsString)>,
    persistent: &HashSet<String>,
) -> Vec<(OsString, OsString)> {
    let keep = |key: &OsString| {
        let k = key.to_string_lossy().to_ascii_uppercase();
        if SESSION_MARKERS.contains(&k.as_str()) || SESSION_MARKER_PREFIXES.iter().any(|p| k.starts_with(p)) {
            return false;
        }
        if k == NO_CWD_EXE.to_ascii_uppercase() {
            return false;
        }
        if k.starts_with("CLAUDE_CODE_") || k == "CLAUDE_EFFORT" {
            return persistent.contains(&k);
        }
        true
    };
    let mut out: Vec<(OsString, OsString)> = vars.into_iter().filter(|(k, _)| keep(k)).collect();
    out.push((NO_CWD_EXE.into(), "1".into()));
    out
}

impl PtyManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// The plain shell: `powershell.exe -NoLogo` (absolute path) in the home dir.
    pub fn spawn_shell(
        &self,
        cols: u16,
        rows: u16,
        on_output: impl Fn(&[u8]) -> bool + Send + 'static,
        on_exit: impl FnOnce(i32) + Send + 'static,
    ) -> Result<PtyInfo, String> {
        let mut cmd = CommandBuilder::new(powershell_exe());
        cmd.arg("-NoLogo");
        if let Some(home) = std::env::var_os("USERPROFILE")
            .or_else(|| std::env::var_os("HOME"))
            .map(std::path::PathBuf::from)
            .filter(|p| p.is_dir())
        {
            cmd.cwd(home);
        }
        self.spawn_builder(cmd, cols, rows, on_output, on_exit)
    }

    /// `powershell.exe` in `cwd` running `claude --resume <session_id>`, left
    /// open afterwards. The caller passes an already validated id and folder
    /// (see `AppState::resume_target`); nothing else reaches the command line.
    pub fn spawn_resume(
        &self,
        cwd: &Path,
        session_id: &str,
        cols: u16,
        rows: u16,
        on_output: impl Fn(&[u8]) -> bool + Send + 'static,
        on_exit: impl FnOnce(i32) + Send + 'static,
    ) -> Result<PtyInfo, String> {
        let args = resume_args(session_id)?;
        self.spawn_resume_in(powershell_exe().as_os_str(), &args, cwd, session_id, cols, rows, on_output, on_exit)
    }

    /// `spawn_in` guarded by the resumed-session set: refused while another
    /// terminal resuming `session_id` is alive; the id is released on exit.
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_resume_in(
        &self,
        program: &OsStr,
        args: &[String],
        cwd: &Path,
        session_id: &str,
        cols: u16,
        rows: u16,
        on_output: impl Fn(&[u8]) -> bool + Send + 'static,
        on_exit: impl FnOnce(i32) + Send + 'static,
    ) -> Result<PtyInfo, String> {
        if !self.resumed.lock().map_err(poisoned)?.insert(session_id.to_string()) {
            return Err("Bu oturum zaten açık".into());
        }
        let release = {
            let resumed = Arc::clone(&self.resumed);
            let sid = session_id.to_string();
            move || {
                if let Ok(mut r) = resumed.lock() {
                    r.remove(&sid);
                }
            }
        };
        let release_on_exit = release.clone();
        let res = self.spawn_in(program, args, cwd, cols, rows, on_output, move |code| {
            release_on_exit();
            on_exit(code);
        });
        if res.is_err() {
            release();
        }
        res
    }

    /// `program args` in `cwd`; shared by `spawn_resume` and tests (stub program).
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_in(
        &self,
        program: &OsStr,
        args: &[String],
        cwd: &Path,
        cols: u16,
        rows: u16,
        on_output: impl Fn(&[u8]) -> bool + Send + 'static,
        on_exit: impl FnOnce(i32) + Send + 'static,
    ) -> Result<PtyInfo, String> {
        let mut cmd = CommandBuilder::new(program);
        cmd.args(args);
        cmd.cwd(cwd);
        self.spawn_builder(cmd, cols, rows, on_output, on_exit)
    }

    /// Internal / test use only; never exposed as a Tauri command.
    #[allow(dead_code)]
    pub fn spawn_cmd(
        &self,
        program: &str,
        args: &[&str],
        cols: u16,
        rows: u16,
        on_output: impl Fn(&[u8]) -> bool + Send + 'static,
        on_exit: impl FnOnce(i32) + Send + 'static,
    ) -> Result<PtyInfo, String> {
        let mut cmd = CommandBuilder::new(program);
        cmd.args(args);
        self.spawn_builder(cmd, cols, rows, on_output, on_exit)
    }

    fn spawn_builder(
        &self,
        mut cmd: CommandBuilder,
        cols: u16,
        rows: u16,
        on_output: impl Fn(&[u8]) -> bool + Send + 'static,
        on_exit: impl FnOnce(i32) + Send + 'static,
    ) -> Result<PtyInfo, String> {
        let id = {
            let mut g = self.inner.lock().map_err(poisoned)?;
            if g.ptys.len() >= MAX_PTYS {
                return Err(format!("en fazla {MAX_PTYS} terminal açılabilir"));
            }
            g.next_id += 1;
            g.next_id
        };
        cmd.env_clear();
        for (k, v) in child_env(std::env::vars_os(), &crate::platform::persistent_env_names()) {
            cmd.env(k, v);
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");

        let pair = native_pty_system()
            .openpty(size(cols, rows))
            .map_err(|e| e.to_string())?;
        let mut child = pair.slave.spawn_command(cmd).map_err(|e| e.to_string())?;
        drop(pair.slave);
        let pid = child.process_id().unwrap_or(0);
        let killer = child.clone_killer();
        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer = pair.master.take_writer().map_err(|e| e.to_string())?;

        {
            let mut g = self.inner.lock().map_err(poisoned)?;
            if g.ptys.len() >= MAX_PTYS {
                let _ = child.kill();
                return Err(format!("en fazla {MAX_PTYS} terminal açılabilir"));
            }
            g.ptys.insert(id, Entry { pid, master: pair.master, writer: Arc::new(Mutex::new(writer)), killer });
        }

        let inner_r = Arc::clone(&self.inner);
        let reader_thread = std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if !on_output(&buf[..n]) {
                            // Nobody receives the output any more (webview reloaded):
                            // kill the orphan; the wait thread still reports the exit.
                            let entry = inner_r.lock().ok().and_then(|mut g| g.ptys.remove(&id));
                            if let Some(mut e) = entry {
                                let _ = e.killer.kill();
                            }
                            break;
                        }
                    }
                }
            }
        });

        let inner = Arc::clone(&self.inner);
        std::thread::spawn(move || {
            let code = child.wait().map(|s| s.exit_code() as i32).unwrap_or(-1);
            // Dropping the master closes the pseudo console, which ends the reader.
            let entry = inner.lock().ok().and_then(|mut g| g.ptys.remove(&id));
            drop(entry);
            let _ = reader_thread.join();
            on_exit(code);
        });

        Ok(PtyInfo { id, pid })
    }

    pub fn write(&self, id: u32, data: &str) -> Result<(), String> {
        if data.len() > MAX_WRITE {
            return Err(format!("veri çok büyük (en fazla {MAX_WRITE} bayt)"));
        }
        // Release the global lock before the (possibly blocking) write so a
        // stuck child cannot stall kill/resize/list/spawn.
        let writer = {
            let g = self.inner.lock().map_err(poisoned)?;
            Arc::clone(&g.ptys.get(&id).ok_or_else(|| unknown(id))?.writer)
        };
        let mut w = writer.lock().map_err(poisoned)?;
        w.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
        w.flush().map_err(|e| e.to_string())
    }

    pub fn resize(&self, id: u32, cols: u16, rows: u16) -> Result<(), String> {
        let g = self.inner.lock().map_err(poisoned)?;
        let e = g.ptys.get(&id).ok_or_else(|| unknown(id))?;
        e.master.resize(size(cols, rows)).map_err(|e| e.to_string())
    }

    pub fn kill(&self, id: u32) -> Result<(), String> {
        let entry = {
            let mut g = self.inner.lock().map_err(poisoned)?;
            g.ptys.remove(&id).ok_or_else(|| unknown(id))?
        };
        let mut killer = entry.killer;
        let _ = killer.kill(); // already-dead is fine
        Ok(())
    }

    /// Kill every terminal (app shutdown).
    pub fn kill_all(&self) {
        let entries: Vec<Entry> = match self.inner.lock() {
            Ok(mut g) => g.ptys.drain().map(|(_, e)| e).collect(),
            Err(_) => return,
        };
        for mut e in entries {
            let _ = e.killer.kill();
        }
    }

    pub fn list(&self) -> Vec<PtyInfo> {
        let Ok(g) = self.inner.lock() else {
            return Vec::new();
        };
        let mut v: Vec<PtyInfo> = g.ptys.iter().map(|(&id, e)| PtyInfo { id, pid: e.pid }).collect();
        v.sort_by_key(|p| p.id);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::ffi::OsString;
    use std::sync::mpsc;
    use std::time::Duration;

    const T: Duration = Duration::from_secs(20);

    enum Ev {
        Out(Vec<u8>),
        Exit(i32),
    }

    #[test]
    fn spawn_echo_and_exit() {
        let m = PtyManager::new();
        let (tx, rx) = mpsc::channel();
        let tx2 = tx.clone();
        let info = m
            .spawn_cmd(
                "cmd.exe",
                &["/c", "echo pano-ok"],
                80,
                24,
                move |b| tx.send(Ev::Out(b.to_vec())).is_ok(),
                move |c| {
                    let _ = tx2.send(Ev::Exit(c));
                },
            )
            .unwrap();
        assert!(info.pid > 0);
        let mut text = String::new();
        let code = loop {
            match rx.recv_timeout(T).expect("timed out") {
                Ev::Out(b) => {
                    text.push_str(&String::from_utf8_lossy(&b));
                    // ConPTY asks for the cursor position and waits; a real
                    // terminal (xterm.js) answers, so the test plays that role.
                    if text.contains("[6n") {
                        text = text.replace("[6n", "");
                        let _ = m.write(info.id, "[1;1R");
                    }
                }
                Ev::Exit(c) => break c,
            }
        };
        assert_eq!(code, 0);
        assert!(text.contains("pano-ok"), "output: {text:?}");
        assert!(m.list().is_empty());
    }

    #[test]
    fn kill_removes_from_list() {
        let m = PtyManager::new();
        let (tx, rx) = mpsc::channel();
        let info = m
            .spawn_cmd("cmd.exe", &["/k"], 80, 24, |_| true, move |c| {
                let _ = tx.send(c);
            })
            .unwrap();
        assert_eq!(m.list(), vec![info]);
        m.kill(info.id).unwrap();
        assert!(m.list().is_empty());
        rx.recv_timeout(T).unwrap(); // on_exit still fires
        assert!(m.kill(info.id).is_err());
    }

    #[test]
    fn unknown_id_is_error() {
        let m = PtyManager::new();
        assert!(m.write(42, "x").is_err());
        assert!(m.resize(42, 80, 24).is_err());
        assert!(m.kill(42).is_err());
    }

    struct BlockingWriter {
        entered: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
    }
    impl Write for BlockingWriter {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            let _ = self.entered.send(());
            let _ = self.release.recv_timeout(T);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn write_does_not_block_other_commands() {
        let m = Arc::new(PtyManager::new());
        let info = m.spawn_cmd("cmd.exe", &["/k"], 80, 24, |_| true, |_| {}).unwrap();
        let (entered_tx, entered_rx) = mpsc::channel();
        let (rel_tx, rel_rx) = mpsc::channel();
        {
            let g = m.inner.lock().unwrap();
            *g.ptys[&info.id].writer.lock().unwrap() =
                Box::new(BlockingWriter { entered: entered_tx, release: rel_rx });
        }
        let m2 = Arc::clone(&m);
        let h = std::thread::spawn(move || m2.write(info.id, "x"));
        entered_rx.recv_timeout(T).unwrap(); // writer is now stuck
        let (done_tx, done_rx) = mpsc::channel();
        let m3 = Arc::clone(&m);
        std::thread::spawn(move || {
            let listed = m3.list();
            let killed = m3.kill(info.id);
            let _ = done_tx.send((listed, killed));
        });
        let (listed, killed) = done_rx.recv_timeout(Duration::from_secs(5)).expect("list/kill blocked");
        assert_eq!(listed.len(), 1);
        killed.unwrap();
        let _ = rel_tx.send(());
        h.join().unwrap().unwrap();
    }

    #[test]
    fn write_rejects_oversized_data() {
        let m = PtyManager::new();
        let big = "a".repeat(MAX_WRITE + 1);
        assert!(m.write(1, &big).is_err());
        let info = m.spawn_cmd("cmd.exe", &["/k"], 80, 24, |_| true, |_| {}).unwrap();
        assert!(m.write(info.id, &big).unwrap_err().contains("büyük"));
        m.kill(info.id).unwrap();
    }

    #[test]
    fn kill_all_kills_everything() {
        let m = PtyManager::new();
        let (tx, rx) = mpsc::channel();
        for _ in 0..3 {
            let tx = tx.clone();
            m.spawn_cmd("cmd.exe", &["/k"], 80, 24, |_| true, move |c| {
                let _ = tx.send(c);
            })
            .unwrap();
        }
        assert_eq!(m.list().len(), 3);
        m.kill_all();
        assert!(m.list().is_empty());
        for _ in 0..3 {
            rx.recv_timeout(T).unwrap();
        }
    }

    #[test]
    fn failed_output_delivery_kills_the_terminal() {
        let m = PtyManager::new();
        let (tx, rx) = mpsc::channel();
        let info = m
            .spawn_cmd("cmd.exe", &["/k"], 80, 24, |_| false, move |c| {
                let _ = tx.send(c);
            })
            .unwrap();
        // The first output chunk (ConPTY's ESC[6n) cannot be delivered.
        rx.recv_timeout(T).expect("orphaned terminal must exit");
        assert!(m.list().is_empty());
        assert!(m.kill(info.id).is_err());
    }

    #[test]
    fn resume_args_exact() {
        assert_eq!(
            resume_args("0f864996-1f75-464e-b101-a3ceed5d79d9").unwrap(),
            vec!["-NoLogo", "-NoExit", "-Command", "claude --resume 0f864996-1f75-464e-b101-a3ceed5d79d9"]
        );
        for bad in ["", "x; calc", "0F864996-1F75-464E-B101-A3CEED5D79D9", "0f864996-1f75-464e-b101-a3ceed5d79d9 & calc"] {
            assert!(resume_args(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn powershell_is_absolute() {
        let p = powershell_exe();
        assert!(p.is_absolute());
        assert!(p.ends_with(Path::new("WindowsPowerShell").join("v1.0").join("powershell.exe")));
    }

    fn kv(pairs: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
        pairs.iter().map(|(k, v)| (OsString::from(k), OsString::from(v))).collect()
    }

    fn names(v: Vec<(OsString, OsString)>) -> Vec<String> {
        v.into_iter().map(|(k, _)| k.to_string_lossy().into_owned()).collect()
    }

    fn set(n: &[&str]) -> HashSet<String> {
        n.iter().map(|s| s.to_ascii_uppercase()).collect()
    }

    #[test]
    fn child_env_always_drops_session_markers() {
        let input = kv(&[
            ("PATH", r"C:\Windows"),
            ("CLAUDECODE", "1"),
            ("ClaudeCode", "1"),
            ("CLAUDE_SESSION_ID", "x"),
            ("claude_pid", "42"),
            ("CLAUDE_CODE_ENTRYPOINT", "cli"),
            ("claude_code_session_id", "x"),
            ("CLAUDE_CODE_CHILD_SESSION", "1"),
            ("CLAUDE_CODE_SSE_PORT", "1234"),
            ("CLAUDE_CODE_MESSAGING_SOCKET", "x"),
            ("CLAUDE_CODE_BRIDGE_SESSION_ID", "x"),
            ("CLAUDE_CODE_SESSION_ATTENDED", "1"),
            ("CLAUDECODE_X", "keep"),
            ("CLAUDE_CONFIG_DIR", r"D:\cfg"),
            ("NO_COLOR", "1"),
        ]);
        // Even when the user made them persistent, markers never leak.
        let persistent = set(&["CLAUDECODE", "CLAUDE_CODE_SSE_PORT", "CLAUDE_CODE_ENTRYPOINT"]);
        assert_eq!(
            names(child_env(input, &persistent)),
            vec!["PATH", "CLAUDECODE_X", "CLAUDE_CONFIG_DIR", "NO_COLOR", NO_CWD_EXE]
        );
    }

    #[test]
    fn child_env_keeps_users_persistent_claude_code_settings() {
        let input = kv(&[
            ("CLAUDE_CODE_GIT_BASH_PATH", r"C:\Git\bin\bash.exe"),
            ("Claude_Code_Use_Bedrock", "1"),
            ("CLAUDE_CODE_MAX_OUTPUT_TOKENS", "8000"),
            ("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1"),
            ("CLAUDE_EFFORT", "high"),
            ("CLAUDE_CODE_SOMETHING_THE_PARENT_SET", "x"),
        ]);
        let persistent = set(&[
            "CLAUDE_CODE_GIT_BASH_PATH",
            "claude_code_use_bedrock",
            "CLAUDE_CODE_MAX_OUTPUT_TOKENS",
            "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC",
        ]);
        assert_eq!(
            names(child_env(input.clone(), &persistent)),
            vec![
                "CLAUDE_CODE_GIT_BASH_PATH",
                "Claude_Code_Use_Bedrock",
                "CLAUDE_CODE_MAX_OUTPUT_TOKENS",
                "CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC",
                NO_CWD_EXE,
            ],
            "CLAUDE_EFFORT and unknown CLAUDE_CODE_* came from the parent session"
        );
        let with_effort = set(&["CLAUDE_EFFORT"]);
        assert_eq!(names(child_env(input, &with_effort)), vec!["CLAUDE_EFFORT", NO_CWD_EXE]);
    }

    #[test]
    fn child_env_forbids_exe_lookup_in_the_current_directory() {
        let out = child_env(kv(&[("nodefaultcurrentdirectoryinexepath", "0"), ("PATH", "x")]), &HashSet::new());
        let v: Vec<(String, String)> =
            out.into_iter().map(|(k, v)| (k.to_string_lossy().into(), v.to_string_lossy().into())).collect();
        assert_eq!(v, vec![("PATH".to_string(), "x".to_string()), (NO_CWD_EXE.to_string(), "1".to_string())]);
    }

    #[test]
    fn spawned_child_does_not_see_claude_markers() {
        // Set in this process (unique name, removed below); the child must not
        // inherit it: an unknown CLAUDE_CODE_* that is not persistent.
        const VAR: &str = "CLAUDE_CODE_PANO_TEST_MARKER_7F3A9C";
        std::env::set_var(VAR, "sizdi");
        let m = PtyManager::new();
        let (tx, rx) = mpsc::channel();
        let tx2 = tx.clone();
        let info = m
            .spawn_cmd(
                "cmd.exe",
                &["/c", "echo [%CLAUDE_CODE_PANO_TEST_MARKER_7F3A9C%] [%NoDefaultCurrentDirectoryInExePath%]"],
                80,
                24,
                move |b| tx.send(Ev::Out(b.to_vec())).is_ok(),
                move |c| {
                    let _ = tx2.send(Ev::Exit(c));
                },
            )
            .unwrap();
        let text = collect_output(&m, info.id, &rx);
        std::env::remove_var(VAR);
        assert!(!text.contains("sizdi"), "marker leaked: {text:?}");
        assert!(text.contains("[%CLAUDE_CODE_PANO_TEST_MARKER_7F3A9C%]"), "output: {text:?}");
        assert!(text.contains("[1]"), "NoDefaultCurrentDirectoryInExePath not set: {text:?}");
    }

    /// Output until exit, answering ConPTY's cursor-position query.
    fn collect_output(m: &PtyManager, id: u32, rx: &mpsc::Receiver<Ev>) -> String {
        let mut text = String::new();
        loop {
            match rx.recv_timeout(T).expect("timed out") {
                Ev::Out(b) => {
                    text.push_str(&String::from_utf8_lossy(&b));
                    if text.contains("\x1b[6n") {
                        text = text.replace("\x1b[6n", "");
                        let _ = m.write(id, "\x1b[1;1R");
                    }
                }
                Ev::Exit(_) => return text,
            }
        }
    }

    #[test]
    fn a_session_is_resumed_by_one_live_terminal_at_a_time() {
        const SID: &str = "11111111-2222-4333-8444-555555555555";
        let m = PtyManager::new();
        let dir = std::env::temp_dir();
        let args = vec!["/k".to_string()];
        let (tx, rx) = mpsc::channel();
        let start = |tx: mpsc::Sender<i32>| {
            m.spawn_resume_in(OsStr::new("cmd.exe"), &args, &dir, SID, 80, 24, |_| true, move |c| {
                let _ = tx.send(c);
            })
        };
        let first = start(tx.clone()).unwrap();
        assert_eq!(start(tx.clone()).unwrap_err(), "Bu oturum zaten açık");
        // Another session is not blocked.
        let other = m
            .spawn_resume_in(OsStr::new("cmd.exe"), &args, &dir, "aaaaaaaa-2222-4333-8444-555555555555", 80, 24, |_| true, |_| {})
            .unwrap();
        m.kill(first.id).unwrap();
        rx.recv_timeout(T).expect("exit");
        let again = start(tx.clone()).unwrap();
        m.kill(again.id).unwrap();
        m.kill(other.id).unwrap();
        // A failed spawn does not leave the id behind.
        let bad = m.spawn_resume_in(OsStr::new("yok-boyle-bir-program.exe"), &args, &dir, "bbbbbbbb-2222-4333-8444-555555555555", 80, 24, |_| true, |_| {});
        assert!(bad.is_err());
        assert!(!m.resumed.lock().unwrap().contains("bbbbbbbb-2222-4333-8444-555555555555"));
    }

    #[test]
    fn size_is_clamped() {
        let s = size(0, 5000);
        assert_eq!((s.cols, s.rows), (1, 1000));
    }

    #[test]
    fn caps_concurrent_ptys() {
        let m = PtyManager::new();
        let mut ids = Vec::new();
        for _ in 0..MAX_PTYS {
            ids.push(m.spawn_cmd("cmd.exe", &["/k"], 80, 24, |_| true, |_| {}).unwrap().id);
        }
        assert!(m.spawn_cmd("cmd.exe", &["/k"], 80, 24, |_| true, |_| {}).is_err());
        for id in ids {
            m.kill(id).unwrap();
        }
    }
}
