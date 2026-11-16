//! Opening things outside the app on Linux (and other Unix desktops): the
//! file manager through `xdg-open`, and the user's terminal emulator.
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;
use station_core::registry::ProcessProbe;

/// Well-known system bin directories, tried before PATH so a program is not
/// picked up from a user-writable PATH entry first.
const SYSTEM_BINS: &[&str] = &["/usr/bin", "/usr/local/bin", "/bin", "/run/current-system/sw/bin"];

/// Terminal emulators tried in order when `$TERMINAL` is not usable.
/// `x-terminal-emulator` is Debian's alternatives link to the user's choice.
const TERMINALS: &[&str] = &[
    "x-terminal-emulator",
    "ghostty",
    "kitty",
    "alacritty",
    "wezterm",
    "foot",
    "konsole",
    "gnome-terminal",
    "kgx",
    "ptyxis",
    "xfce4-terminal",
    "tilix",
    "terminator",
    "xterm",
];

/// `name` resolved to an executable file: a plain name is looked up in the
/// system bins, then in the absolute entries of `path` (relative PATH entries
/// would resolve against the cwd). A name with a `/` must be absolute.
pub(crate) fn find_program(name: &OsStr, path: Option<&OsStr>) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let exec = |p: &Path| std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
    let n = Path::new(name);
    if name.is_empty() {
        return None;
    }
    if n.components().count() > 1 || name.to_string_lossy().contains('/') {
        return (n.is_absolute() && exec(n)).then(|| n.to_path_buf());
    }
    let user: Vec<PathBuf> = path.map(|p| std::env::split_paths(p).filter(|d| d.is_absolute()).collect()).unwrap_or_default();
    SYSTEM_BINS.iter().map(PathBuf::from).chain(user).map(|d| d.join(n)).find(|c| exec(c))
}

/// Runs `program` detached in `cwd` with the cleaned child environment (no
/// Claude Code session markers), reaping it on a thread.
fn spawn_detached(program: &Path, args: &[&str], cwd: Option<&Path>) -> std::io::Result<()> {
    let mut cmd = Command::new(program);
    cmd.args(args).env_clear().envs(crate::pty::child_env(std::env::vars_os(), &Default::default()));
    if let Some(d) = cwd {
        cmd.current_dir(d);
    }
    let mut child = cmd.spawn()?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

/// Opens an already validated directory in the file manager.
pub fn open_folder(dir: &Path) -> std::io::Result<()> {
    let opener = find_program(OsStr::new("xdg-open"), std::env::var_os("PATH").as_deref())
        .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "xdg-open"))?;
    // `dir` is absolute (starts with `/`), so it can never be read as an option.
    let mut cmd = Command::new(opener);
    cmd.arg(dir);
    let mut child = cmd.spawn()?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

/// The terminal emulator to open: `$TERMINAL` when it names a program,
/// otherwise the first of [`TERMINALS`] that is installed.
pub(crate) fn terminal_program(terminal_env: Option<&OsStr>, path: Option<&OsStr>) -> Option<PathBuf> {
    terminal_env
        .and_then(|t| find_program(t, path))
        .or_else(|| TERMINALS.iter().find_map(|t| find_program(OsStr::new(t), path)))
}

/// Opens the user's terminal emulator in the home folder. Takes nothing from
/// the UI. Every listed emulator starts in its own working directory.
pub fn open_external_terminal() -> Result<(), String> {
    let path: Option<OsString> = std::env::var_os("PATH");
    let program = terminal_program(std::env::var_os("TERMINAL").as_deref(), path.as_deref())
        .ok_or(crate::errors::TERMINAL_NOT_FOUND)?;
    let home = std::env::var_os("HOME").map(PathBuf::from).filter(|h| h.is_absolute() && h.is_dir());
    spawn_detached(&program, &[], home.as_deref())
        .map_err(|e| crate::errors::with(crate::errors::TERMINAL_OPEN_FAILED, e))
}

/// Terminates `pid` when its start time still equals `proc_start`: SIGTERM,
/// then SIGKILL if it is still there after 2 s.
pub fn terminate_verified(pid: u32, proc_start: u64) -> Result<(), String> {
    use crate::errors::{ADOPT_KILL_FAILED, SESSION_NOT_LIVE};
    use std::time::{Duration, Instant};
    let probe = super::probe();
    if probe.start_filetime(pid) != Some(proc_start) {
        return Err(SESSION_NOT_LIVE.into());
    }
    let gone = || probe.start_filetime(pid) != Some(proc_start) || is_zombie(pid);
    let send = |sig: libc::c_int| unsafe { libc::kill(pid as libc::pid_t, sig) } == 0;
    if !send(libc::SIGTERM) {
        let esrch = std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH);
        return Err(if esrch { SESSION_NOT_LIVE } else { ADOPT_KILL_FAILED }.into());
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if gone() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    send(libc::SIGKILL);
    std::thread::sleep(Duration::from_millis(100));
    if gone() { Ok(()) } else { Err(ADOPT_KILL_FAILED.into()) }
}

/// A killed child of this process stays a zombie (it still has a start time)
/// until reaped; count it as gone. Reads /proc, so Linux only (false elsewhere).
fn is_zombie(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .ok()
        .and_then(|s| s.rsplit_once(')').map(|(_, rest)| rest.trim_start().starts_with('Z')))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn long_child() -> std::process::Child {
        Command::new("sleep").arg("30").spawn().unwrap()
    }

    #[test]
    fn terminate_kills_matching_process() {
        let mut child = long_child();
        let start = super::super::probe().start_filetime(child.id()).expect("start time");
        assert_eq!(terminate_verified(child.id(), start), Ok(()));
        let t = std::time::Instant::now();
        while child.try_wait().unwrap().is_none() {
            assert!(t.elapsed() < std::time::Duration::from_secs(2), "child still running");
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    #[test]
    fn terminate_refuses_wrong_start_time() {
        let mut child = long_child();
        let start = super::super::probe().start_filetime(child.id()).expect("start time");
        let r = terminate_verified(child.id(), start + 1);
        let alive = child.try_wait().unwrap().is_none();
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(r, Err(crate::errors::SESSION_NOT_LIVE.to_string()));
        assert!(alive);
    }

    #[test]
    fn programs_resolve_to_absolute_executables_only() {
        let sh = find_program(OsStr::new("sh"), None).expect("sh is in a system bin");
        assert!(sh.is_absolute());
        assert_eq!(find_program(OsStr::new("/bin/sh"), None), Some(PathBuf::from("/bin/sh")));
        // A relative path, a missing program and a relative PATH entry never resolve.
        assert_eq!(find_program(OsStr::new("./sh"), None), None);
        assert_eq!(find_program(OsStr::new("yok-boyle-bir-program"), None), None);
        assert_eq!(find_program(OsStr::new(""), None), None);
        let dir = tempfile::tempdir().unwrap();
        let fake = dir.path().join("sahte-terminal");
        std::fs::write(&fake, "#!/bin/sh\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(find_program(OsStr::new("sahte-terminal"), Some(dir.path().as_os_str())), Some(fake.clone()));
        assert_eq!(find_program(OsStr::new("sahte-terminal"), Some(OsStr::new("rel/dir"))), None);
        // $TERMINAL wins when it resolves; otherwise the list is used.
        assert_eq!(terminal_program(Some(fake.as_os_str()), None), Some(fake));
        assert_ne!(terminal_program(Some(OsStr::new("yok-boyle")), None), Some(PathBuf::from("yok-boyle")));
    }
}
