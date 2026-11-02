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
use std::time::{Duration, Instant};

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
    /// Session ids held by the terminal that resumed them (see `Resumed`).
    resumed: Arc<Mutex<Resumed>>,
}

/// How long a resumed `claude` may take to appear in the session registry
/// before its hold is dropped (it failed to start).
pub const RESUME_START_GRACE: Duration = Duration::from_secs(30);

struct ResumeHold {
    /// Identifies this hold, so an exiting terminal never drops a newer one.
    token: u64,
    /// The terminal, once the spawn succeeded.
    pty: Option<u32>,
    since: Instant,
    seen_live: bool,
}

/// Sessions resumed by an app terminal. A second resume before the new
/// `claude` registers would run the session twice, so the id is held from
/// the spawn until `claude` has been seen live and then left the live set
/// (the -NoExit shell stays open after it), or until it failed to appear
/// within `RESUME_START_GRACE`, or until the terminal exits.
#[derive(Default)]
struct Resumed {
    next: u64,
    held: HashMap<String, ResumeHold>,
}

impl Resumed {
    /// Atomic insert-or-refuse. Err carries the holding terminal (None while
    /// its spawn is still in flight).
    fn try_hold(&mut self, sid: &str, now: Instant) -> Result<u64, Option<u32>> {
        if let Some(h) = self.held.get(sid) {
            return Err(h.pty);
        }
        self.next += 1;
        self.held.insert(sid.to_string(), ResumeHold { token: self.next, pty: None, since: now, seen_live: false });
        Ok(self.next)
    }

    fn attach(&mut self, sid: &str, token: u64, pty: u32) {
        if let Some(h) = self.held.get_mut(sid).filter(|h| h.token == token) {
            h.pty = Some(pty);
        }
    }

    fn release(&mut self, sid: &str, token: u64) {
        if self.held.get(sid).is_some_and(|h| h.token == token) {
            self.held.remove(sid);
        }
    }

    /// One snapshot's live session ids.
    fn observe(&mut self, live: &HashSet<String>, now: Instant) {
        self.held.retain(|sid, h| {
            if live.contains(sid) {
                h.seen_live = true;
                true
            } else {
                !h.seen_live && now.saturating_duration_since(h.since) < RESUME_START_GRACE
            }
        });
    }

    #[cfg(test)]
    fn holder(&self, sid: &str) -> Option<u32> {
        self.held.get(sid).and_then(|h| h.pty)
    }
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
    crate::errors::with(crate::errors::PTY_UNKNOWN, id)
}

fn spawn_failed(e: impl std::fmt::Display) -> String {
    crate::errors::with(crate::errors::PTY_SPAWN_FAILED, e)
}

fn io_failed(e: impl std::fmt::Display) -> String {
    crate::errors::with(crate::errors::PTY_IO, e)
}

fn resize_failed(e: impl std::fmt::Display) -> String {
    crate::errors::with(crate::errors::PTY_RESIZE_FAILED, e)
}

fn resume_terminal_open(pty: Option<u32>) -> String {
    match pty {
        Some(id) => crate::errors::with(crate::errors::SESSION_RESUME_TERMINAL_OPEN, id),
        None => crate::errors::SESSION_RESUME_TERMINAL_OPEN.into(),
    }
}

fn poisoned<T>(_: T) -> String {
    crate::errors::INTERNAL_LOCK.to_string()
}

/// Absolute path of Windows PowerShell (never resolved through PATH).
pub fn powershell_exe() -> PathBuf {
    let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    PathBuf::from(root).join("System32").join("WindowsPowerShell").join("v1.0").join("powershell.exe")
}

/// PowerShell arguments for resuming a session. Only a lowercase UUID is
/// accepted, so the `-Command` text can never carry anything else.
pub fn resume_args(session_id: &str) -> Result<Vec<String>, String> {
    if !is_session_id(session_id) {
        return Err(crate::errors::SESSION_INVALID.into());
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

    /// `spawn_in` guarded by the resumed-session holds (see `Resumed`):
    /// refused with `session_resume_terminal_open[:<terminal>]` while held.
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
        let token = self
            .resumed
            .lock()
            .map_err(poisoned)?
            .try_hold(session_id, Instant::now())
            .map_err(resume_terminal_open)?;
        let release = {
            let resumed = Arc::clone(&self.resumed);
            let sid = session_id.to_string();
            move || {
                if let Ok(mut r) = resumed.lock() {
                    r.release(&sid, token);
                }
            }
        };
        let release_on_exit = release.clone();
        let res = self.spawn_in(program, args, cwd, cols, rows, on_output, move |code| {
            release_on_exit();
            on_exit(code);
        });
        match &res {
            Ok(info) => {
                if let Ok(mut r) = self.resumed.lock() {
                    r.attach(session_id, token, info.id);
                }
            }
            Err(_) => release(),
        }
        res
    }

    /// The app terminal holding `session_id` (claude runs or starts there).
    #[cfg(test)]
    pub fn resume_terminal(&self, session_id: &str) -> Option<u32> {
        self.resumed.lock().ok()?.holder(session_id)
    }

    /// `Err` for a resume of a session held by one of our terminals.
    pub fn check_resume_terminal(&self, session_id: &str) -> Result<(), String> {
        let held = self.resumed.lock().map_err(poisoned)?.held.get(session_id).map(|h| h.pty);
        match held {
            Some(pty) => Err(resume_terminal_open(pty)),
            None => Ok(()),
        }
    }

    /// Feeds one snapshot's live session ids to the resume holds (called by
    /// the snapshot thread after every refresh).
    pub fn observe_live(&self, live: &HashSet<String>) {
        self.observe_live_at(live, Instant::now());
    }

    pub fn observe_live_at(&self, live: &HashSet<String>, now: Instant) {
        if let Ok(mut r) = self.resumed.lock() {
            r.observe(live, now);
        }
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
                return Err(crate::errors::with(crate::errors::PTY_LIMIT, MAX_PTYS));
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
            .map_err(spawn_failed)?;
        let mut child = pair.slave.spawn_command(cmd).map_err(spawn_failed)?;
        drop(pair.slave);
        let pid = child.process_id().unwrap_or(0);
        let killer = child.clone_killer();
        let mut reader = pair.master.try_clone_reader().map_err(spawn_failed)?;
        let writer = pair.master.take_writer().map_err(spawn_failed)?;

        {
            let mut g = self.inner.lock().map_err(poisoned)?;
            if g.ptys.len() >= MAX_PTYS {
                let _ = child.kill();
                return Err(crate::errors::with(crate::errors::PTY_LIMIT, MAX_PTYS));
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
            return Err(crate::errors::with(crate::errors::PTY_TOO_LARGE, MAX_WRITE));
        }
        // Release the global lock before the (possibly blocking) write so a
        // stuck child cannot stall kill/resize/list/spawn.
        let writer = {
            let g = self.inner.lock().map_err(poisoned)?;
            Arc::clone(&g.ptys.get(&id).ok_or_else(|| unknown(id))?.writer)
        };
        let mut w = writer.lock().map_err(poisoned)?;
        w.write_all(data.as_bytes()).map_err(io_failed)?;
        w.flush().map_err(io_failed)
    }

    pub fn resize(&self, id: u32, cols: u16, rows: u16) -> Result<(), String> {
        let g = self.inner.lock().map_err(poisoned)?;
        let e = g.ptys.get(&id).ok_or_else(|| unknown(id))?;
        e.master.resize(size(cols, rows)).map_err(resize_failed)
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
    use std::time::{Duration, Instant};

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
        assert_eq!(m.write(42, "x").unwrap_err(), "pty_unknown:42");
        assert_eq!(m.resize(42, 80, 24).unwrap_err(), "pty_unknown:42");
        assert_eq!(m.kill(42).unwrap_err(), "pty_unknown:42");
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
        assert_eq!(m.write(info.id, &big).unwrap_err(), format!("pty_too_large:{MAX_WRITE}"));
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
            resume_args("f00dcafe-0000-4000-8000-000000000001").unwrap(),
            vec!["-NoLogo", "-NoExit", "-Command", "claude --resume f00dcafe-0000-4000-8000-000000000001"]
        );
        for bad in ["", "x; calc", "F00DCAFE-0000-4000-8000-000000000001", "f00dcafe-0000-4000-8000-000000000001 & calc"] {
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

    const A: &str = "11111111-2222-4333-8444-555555555555";
    const B: &str = "aaaaaaaa-2222-4333-8444-555555555555";

    fn live(ids: &[&str]) -> HashSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn resume_guard_is_insert_or_refuse() {
        let t0 = Instant::now();
        let mut r = Resumed::default();
        let tok = r.try_hold(A, t0).unwrap();
        assert_eq!(r.try_hold(A, t0), Err(None), "spawn still in flight: no terminal yet");
        r.attach(A, tok, 7);
        assert_eq!(r.try_hold(A, t0), Err(Some(7)));
        assert_eq!(r.holder(A), Some(7));
        assert!(r.try_hold(B, t0).is_ok(), "another session is not blocked");
    }

    #[test]
    fn resume_guard_released_after_the_session_was_live_and_left() {
        let t0 = Instant::now();
        let mut r = Resumed::default();
        let tok = r.try_hold(A, t0).unwrap();
        r.attach(A, tok, 1);
        r.observe(&live(&[]), t0 + Duration::from_secs(2));
        assert_eq!(r.holder(A), Some(1), "not seen live yet: still starting");
        r.observe(&live(&[A]), t0 + Duration::from_secs(3));
        r.observe(&live(&[A]), t0 + Duration::from_secs(600));
        assert_eq!(r.holder(A), Some(1), "live: held however long it runs");
        // The user quit claude; PowerShell (-NoExit) keeps the terminal open.
        r.observe(&live(&[]), t0 + Duration::from_secs(601));
        assert_eq!(r.holder(A), None);
        assert!(r.try_hold(A, t0 + Duration::from_secs(602)).is_ok());
    }

    #[test]
    fn resume_guard_released_when_claude_never_shows_up() {
        let t0 = Instant::now();
        let mut r = Resumed::default();
        let tok = r.try_hold(A, t0).unwrap();
        r.attach(A, tok, 1);
        r.observe(&live(&[]), t0 + RESUME_START_GRACE - Duration::from_millis(1));
        assert_eq!(r.holder(A), Some(1));
        r.observe(&live(&[]), t0 + RESUME_START_GRACE);
        assert_eq!(r.holder(A), None, "claude failed to start");
    }

    #[test]
    fn resume_guard_exit_releases_only_its_own_hold() {
        let t0 = Instant::now();
        let mut r = Resumed::default();
        let old = r.try_hold(A, t0).unwrap();
        r.attach(A, old, 1);
        r.observe(&live(&[A]), t0);
        r.observe(&live(&[]), t0); // released while terminal 1 stays open
        let new = r.try_hold(A, t0).unwrap();
        r.attach(A, new, 2);
        r.release(A, old); // terminal 1 is closed later
        assert_eq!(r.holder(A), Some(2), "the newer terminal keeps its hold");
        r.release(A, new);
        assert_eq!(r.holder(A), None);
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
        assert_eq!(m.resume_terminal(SID), Some(first.id));
        assert_eq!(
            start(tx.clone()).unwrap_err(),
            format!("{}:{}", crate::errors::SESSION_RESUME_TERMINAL_OPEN, first.id)
        );
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
        assert_eq!(m.resume_terminal("bbbbbbbb-2222-4333-8444-555555555555"), None);
    }

    #[test]
    fn resume_guard_follows_the_live_set_while_the_terminal_stays_open() {
        const SID: &str = "cccccccc-2222-4333-8444-555555555555";
        let m = PtyManager::new();
        let dir = std::env::temp_dir();
        let args = vec!["/k".to_string()];
        let start = || m.spawn_resume_in(OsStr::new("cmd.exe"), &args, &dir, SID, 80, 24, |_| true, |_| {});
        let first = start().unwrap();
        m.observe_live_at(&live(&[SID]), Instant::now());
        assert!(start().is_err(), "claude runs in the resume terminal");
        m.observe_live_at(&live(&[]), Instant::now());
        // claude quit; the -NoExit shell (terminal `first`) is still open.
        assert!(m.list().iter().any(|p| p.id == first.id));
        let second = start().expect("released once the session left the live set");
        m.kill(first.id).unwrap();
        m.kill(second.id).unwrap();
    }

    #[test]
    fn resize_failure_has_its_own_code() {
        assert_eq!(resize_failed("os"), "pty_resize_failed:os");
        assert_eq!(io_failed("os"), "pty_io:os", "write failures keep pty_io");
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
