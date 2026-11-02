//! ConPTY terminal manager. Independent of the webview: output and exit are
//! delivered through plain callbacks; the Tauri commands wrap them in Channels.

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

pub const MAX_PTYS: usize = 32;
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
    writer: Box<dyn Write + Send>,
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

impl PtyManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// The only shell the UI can start: `powershell.exe -NoLogo` in the home dir.
    pub fn spawn_shell(
        &self,
        cols: u16,
        rows: u16,
        on_output: impl Fn(&[u8]) + Send + 'static,
        on_exit: impl FnOnce(i32) + Send + 'static,
    ) -> Result<PtyInfo, String> {
        let mut cmd = CommandBuilder::new("powershell.exe");
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

    /// Internal / test use only; never exposed as a Tauri command.
    #[allow(dead_code)]
    pub fn spawn_cmd(
        &self,
        program: &str,
        args: &[&str],
        cols: u16,
        rows: u16,
        on_output: impl Fn(&[u8]) + Send + 'static,
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
        on_output: impl Fn(&[u8]) + Send + 'static,
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
            g.ptys.insert(id, Entry { pid, master: pair.master, writer, killer });
        }

        let reader_thread = std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            loop {
                match reader.read(&mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => on_output(&buf[..n]),
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
        let mut g = self.inner.lock().map_err(poisoned)?;
        let e = g.ptys.get_mut(&id).ok_or_else(|| unknown(id))?;
        e.writer.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
        e.writer.flush().map_err(|e| e.to_string())
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
                move |b| {
                    let _ = tx.send(Ev::Out(b.to_vec()));
                },
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
            .spawn_cmd("cmd.exe", &["/k"], 80, 24, |_| {}, move |c| {
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
            ids.push(m.spawn_cmd("cmd.exe", &["/k"], 80, 24, |_| {}, |_| {}).unwrap().id);
        }
        assert!(m.spawn_cmd("cmd.exe", &["/k"], 80, 24, |_| {}, |_| {}).is_err());
        for id in ids {
            m.kill(id).unwrap();
        }
    }
}
