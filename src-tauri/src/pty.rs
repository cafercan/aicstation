//! ConPTY terminal manager. Independent of the webview: output and exit are
//! delivered through plain callbacks; the Tauri commands wrap them in Channels.

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use std::collections::HashMap;
use std::io::{Read, Write};
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

    /// The only shell the UI can start: `powershell.exe -NoLogo` (absolute path) in the home dir.
    pub fn spawn_shell(
        &self,
        cols: u16,
        rows: u16,
        on_output: impl Fn(&[u8]) + Send + 'static,
        on_exit: impl FnOnce(i32) + Send + 'static,
    ) -> Result<PtyInfo, String> {
        let root = std::env::var_os("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
        let exe = std::path::PathBuf::from(root)
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe");
        let mut cmd = CommandBuilder::new(exe);
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
            g.ptys.insert(id, Entry { pid, master: pair.master, writer: Arc::new(Mutex::new(writer)), killer });
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
        let info = m.spawn_cmd("cmd.exe", &["/k"], 80, 24, |_| {}, |_| {}).unwrap();
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
        let info = m.spawn_cmd("cmd.exe", &["/k"], 80, 24, |_| {}, |_| {}).unwrap();
        assert!(m.write(info.id, &big).unwrap_err().contains("büyük"));
        m.kill(info.id).unwrap();
    }

    #[test]
    fn kill_all_kills_everything() {
        let m = PtyManager::new();
        let (tx, rx) = mpsc::channel();
        for _ in 0..3 {
            let tx = tx.clone();
            m.spawn_cmd("cmd.exe", &["/k"], 80, 24, |_| {}, move |c| {
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
