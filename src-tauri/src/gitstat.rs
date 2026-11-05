//! Git "dirty" marker of a session's folder for the terminal Claude bar:
//! `git status --porcelain --untracked-files=no`, cached per folder and run
//! at most once per [`TTL`].
use std::collections::HashMap;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const TTL: Duration = Duration::from_secs(10);
const TIMEOUT: Duration = Duration::from_secs(3);
const MAX_FOLDERS: usize = 64;

/// Folder -> (when it was checked, result).
type Cache = HashMap<String, (Instant, Option<bool>)>;

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// Modified tracked files in `cwd`: `Some(true)`; clean: `Some(false)`;
/// not a git work tree, git missing or too slow: `None`.
pub fn dirty_uncached(cwd: &Path) -> Option<bool> {
    let mut c = Command::new("git");
    // No fsmonitor hook and no index lock: this only looks.
    c.args(["--no-optional-locks", "-c", "core.fsmonitor=false", "status", "--porcelain", "--untracked-files=no"])
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
    let mut out = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        use std::io::Read;
        let mut buf = [0u8; 1];
        // One byte is enough: any output means dirty. Drain the rest so git exits.
        let n = out.read(&mut buf).unwrap_or(0);
        let _ = std::io::copy(&mut out, &mut std::io::sink());
        n > 0
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
    let any = reader.join().unwrap_or(false);
    ok.then_some(any)
}

pub fn dirty(cwd: &str) -> Option<bool> {
    let p = Path::new(cwd);
    if cwd.is_empty() || !p.is_absolute() || !p.is_dir() {
        return None;
    }
    let now = Instant::now();
    if let Some((at, v)) = CACHE.lock().ok()?.as_ref().and_then(|m| m.get(cwd).copied()) {
        if now.duration_since(at) < TTL {
            return v;
        }
    }
    let v = dirty_uncached(p);
    let mut g = CACHE.lock().ok()?;
    let m = g.get_or_insert_with(HashMap::new);
    if m.len() >= MAX_FOLDERS {
        m.retain(|_, (at, _)| now.duration_since(*at) < TTL);
    }
    m.insert(cwd.to_string(), (now, v));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(dir: &Path, args: &[&str]) {
        let ok = Command::new("git").args(args).current_dir(dir).stdout(Stdio::null()).stderr(Stdio::null()).status().unwrap().success();
        assert!(ok, "git {args:?}");
    }

    #[test]
    fn tracked_changes_make_a_folder_dirty() {
        if Command::new("git").arg("--version").output().is_err() {
            return; // no git on this machine
        }
        let d = tempfile::tempdir().unwrap();
        assert_eq!(dirty_uncached(d.path()), None, "not a repository");
        git(d.path(), &["init", "-q"]);
        git(d.path(), &["config", "user.email", "t@t"]);
        git(d.path(), &["config", "user.name", "t"]);
        std::fs::write(d.path().join("a.txt"), "1").unwrap();
        assert_eq!(dirty_uncached(d.path()), Some(false), "untracked files do not count");
        git(d.path(), &["add", "a.txt"]);
        git(d.path(), &["commit", "-qm", "a"]);
        assert_eq!(dirty_uncached(d.path()), Some(false));
        std::fs::write(d.path().join("a.txt"), "2").unwrap();
        assert_eq!(dirty_uncached(d.path()), Some(true));
        assert_eq!(dirty("relative/path"), None);
    }
}
