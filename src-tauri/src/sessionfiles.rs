//! Files of a session folder for the message box's `@` suggestions, listed
//! at most once per [`TTL`] per folder: git's list of tracked and untracked,
//! not ignored, files in a work tree (what Claude Code suggests), else a
//! bounded walk.
use station_core::files::{self, FileHint, MAX_ENTRIES};
use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const TTL: Duration = Duration::from_secs(10);
const TIMEOUT: Duration = Duration::from_secs(3);
const MAX_FOLDERS: usize = 16;
/// Most bytes of git output read (paths of `MAX_ENTRIES` files fit).
const MAX_GIT_OUT: u64 = 4 << 20;

type Cache = HashMap<String, (Instant, Arc<Vec<FileHint>>)>;
static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// `git ls-files` of the work tree at `cwd`; None outside a work tree, or
/// when git is missing, fails or is too slow.
fn git_files(cwd: &Path) -> Option<Vec<FileHint>> {
    let mut c = Command::new("git");
    c.args(["--no-optional-locks", "-c", "core.fsmonitor=false", "ls-files", "-z", "--cached", "--others", "--exclude-standard"])
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = c.spawn().ok()?;
    let out = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        use std::io::Read;
        let mut buf = Vec::new();
        let mut out = out;
        let _ = (&mut out).take(MAX_GIT_OUT).read_to_end(&mut buf);
        // Drain the rest so git exits.
        let _ = std::io::copy(&mut out, &mut std::io::sink());
        buf
    });
    let start = Instant::now();
    let ok = loop {
        match child.try_wait() {
            Ok(Some(st)) => break st.success(),
            Ok(None) if start.elapsed() < TIMEOUT => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    let buf = reader.join().ok()?;
    // An empty list (the folder is ignored, or has no files yet) falls back to the walk.
    Some(files::from_git(&buf, MAX_ENTRIES)).filter(|v| ok && !v.is_empty())
}

/// The files of `dir` (an absolute local folder already checked by the caller).
pub fn list(dir: &Path) -> Arc<Vec<FileHint>> {
    let key = dir.to_string_lossy().into_owned();
    let now = Instant::now();
    if let Some((at, v)) = CACHE.lock().ok().and_then(|g| g.as_ref().and_then(|m| m.get(&key).cloned())) {
        if now.duration_since(at) < TTL {
            return v;
        }
    }
    let v = Arc::new(git_files(dir).unwrap_or_else(|| files::walk(dir, MAX_ENTRIES)));
    if let Ok(mut g) = CACHE.lock() {
        let m = g.get_or_insert_with(HashMap::new);
        if m.len() >= MAX_FOLDERS {
            m.retain(|_, (at, _)| now.duration_since(*at) < TTL);
            if m.len() >= MAX_FOLDERS {
                m.clear();
            }
        }
        m.insert(key, (now, v.clone()));
    }
    v
}
