//! The shell the embedded terminals run, and how it is told to resume a
//! session. Windows: Windows PowerShell from the System32 directory. Unix:
//! the user's login shell (`$SHELL`), falling back to bash, then sh.
//!
//! Only a validated lowercase session UUID and the shell's own validated
//! path ever reach a command line.
use station_core::locate::is_session_id;
use std::path::{Path, PathBuf};

/// How a shell takes a command to run before it stays interactive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellKind {
    /// Windows PowerShell or PowerShell 7 (`pwsh`).
    PowerShell,
    Bash,
    Zsh,
    Fish,
    /// sh, dash, ksh ... (`-i -c '<cmd>; exec <shell>'`, as bash/zsh/fish
    /// without integration scripts).
    Posix,
    /// Nushell (`-e <cmd>` keeps it open afterwards).
    Nu,
    Cmd,
}

impl ShellKind {
    pub fn of(program: &Path) -> Self {
        let stem = program.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_ascii_lowercase();
        match stem.as_str() {
            "pwsh" | "powershell" | "pwsh-preview" => ShellKind::PowerShell,
            "nu" => ShellKind::Nu,
            "cmd" => ShellKind::Cmd,
            "bash" => ShellKind::Bash,
            "zsh" => ShellKind::Zsh,
            "fish" => ShellKind::Fish,
            _ => ShellKind::Posix,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shell {
    pub program: PathBuf,
    pub kind: ShellKind,
}

impl Shell {
    pub fn new(program: PathBuf) -> Self {
        let kind = ShellKind::of(&program);
        Shell { program, kind }
    }

    /// Arguments of a plain interactive shell.
    pub fn interactive_args(&self) -> Vec<String> {
        match self.kind {
            ShellKind::PowerShell => vec!["-NoLogo".into()],
            _ => Vec::new(),
        }
    }

    /// Arguments that run `claude --resume <id>` and then leave an interactive
    /// shell open in the same folder.
    pub fn resume_args(&self, session_id: &str) -> Result<Vec<String>, String> {
        if !is_session_id(session_id) {
            return Err(crate::errors::SESSION_INVALID.into());
        }
        let claude = format!("claude --resume {session_id}");
        Ok(match self.kind {
            ShellKind::PowerShell => vec!["-NoLogo".into(), "-NoExit".into(), "-Command".into(), claude],
            ShellKind::Nu => vec!["-e".into(), claude],
            ShellKind::Cmd => vec!["/k".into(), claude],
            ShellKind::Bash | ShellKind::Zsh | ShellKind::Fish | ShellKind::Posix => {
                // `-i` so the rc files run first (PATH to `claude`, prompt);
                // `exec` then replaces the one-shot shell with an interactive
                // one. The quoted path is safe: `posix_quotable` held when the
                // shell was chosen. The same text works in fish.
                let me = self.program.to_str().filter(|p| posix_quotable(p)).ok_or(crate::errors::SHELL_NOT_FOUND)?;
                vec!["-i".into(), "-c".into(), format!("{claude}; exec '{me}'")]
            }
        })
    }
}

/// Whether `p` can be put inside single quotes for both POSIX shells and fish.
fn posix_quotable(p: &str) -> bool {
    !p.is_empty() && !p.contains(['\'', '\\']) && !p.chars().any(char::is_control)
}

/// Absolute path of Windows PowerShell under the System32 directory from
/// `GetSystemDirectoryW` (never PATH, the cwd or `%SystemRoot%`).
#[cfg(windows)]
pub fn powershell_exe() -> PathBuf {
    crate::platform::system_dir().join("WindowsPowerShell").join("v1.0").join("powershell.exe")
}

#[cfg(windows)]
pub fn default_shell() -> Result<Shell, String> {
    Ok(Shell::new(powershell_exe()))
}

/// `$SHELL` when it is an absolute, executable, quotable path; otherwise
/// the first of bash and sh found in the system bins.
#[cfg(unix)]
pub fn default_shell() -> Result<Shell, String> {
    unix_shell(std::env::var_os("SHELL").as_deref())
}

#[cfg(unix)]
pub(crate) fn unix_shell(env_shell: Option<&std::ffi::OsStr>) -> Result<Shell, String> {
    use std::ffi::OsStr;
    let usable = |p: &Path| p.is_absolute() && p.to_str().is_some_and(posix_quotable);
    env_shell
        .map(Path::new)
        .filter(|p| usable(p))
        .and_then(|p| crate::platform::find_program(p.as_os_str(), None))
        .or_else(|| ["bash", "sh"].iter().find_map(|n| crate::platform::find_program(OsStr::new(n), None)))
        .filter(|p| usable(p))
        .map(Shell::new)
        .ok_or_else(|| crate::errors::SHELL_NOT_FOUND.into())
}

/// Program names tried for a shell id on Unix.
#[cfg(unix)]
fn unix_names(id: &str) -> &'static [&'static str] {
    match id {
        "bash" => &["bash"],
        "zsh" => &["zsh"],
        "fish" => &["fish"],
        "pwsh" => &["pwsh", "pwsh-preview"],
        "nu" => &["nu"],
        _ => &[],
    }
}

/// The program for a shell id (see `settings::SHELLS`), or None when it is
/// not installed. `auto` is [`default_shell`].
#[cfg(unix)]
pub fn find(id: &str) -> Option<Shell> {
    if id == "auto" {
        return default_shell().ok();
    }
    let path = std::env::var_os("PATH");
    unix_names(id)
        .iter()
        .find_map(|n| crate::platform::find_program(std::ffi::OsStr::new(n), path.as_deref()))
        .filter(|p| p.to_str().is_some_and(posix_quotable))
        .map(Shell::new)
}

/// Windows: Windows PowerShell from System32, PowerShell 7 and Git Bash
/// from their standard install folders (never PATH).
#[cfg(windows)]
pub fn find(id: &str) -> Option<Shell> {
    let program_files = || {
        std::env::var_os("ProgramFiles")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"))
    };
    let p = match id {
        "auto" | "powershell" => powershell_exe(),
        "pwsh" => program_files().join("PowerShell").join("7").join("pwsh.exe"),
        "bash" => program_files().join("Git").join("bin").join("bash.exe"),
        _ => return None,
    };
    p.is_file().then(|| Shell::new(p))
}

/// The shell for the stored setting; an id that is no longer installed
/// falls back to the platform default.
pub fn for_setting(id: &str) -> Result<Shell, String> {
    find(id).map_or_else(default_shell, Ok)
}

/// Shell ids that resolve on this machine, `auto` first.
pub fn installed() -> Vec<String> {
    crate::settings::SHELLS.iter().filter(|id| **id == "auto" || find(id).is_some()).map(|s| s.to_string()).collect()
}

/// The folder a plain terminal starts in.
pub fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"));
    #[cfg(not(windows))]
    let home = std::env::var_os("HOME");
    home.map(PathBuf::from).filter(|p| p.is_absolute() && p.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SID: &str = "f00dcafe-0000-4000-8000-000000000001";

    #[test]
    fn kind_comes_from_the_file_name() {
        for (p, k) in [
            ("/usr/bin/fish", ShellKind::Fish),
            ("/bin/zsh", ShellKind::Zsh),
            ("/bin/bash", ShellKind::Bash),
            ("/bin/dash", ShellKind::Posix),
            ("/usr/bin/pwsh", ShellKind::PowerShell),
            ("powershell.exe", ShellKind::PowerShell),
            ("cmd.exe", ShellKind::Cmd),
            ("/usr/bin/nu", ShellKind::Nu),
        ] {
            assert_eq!(ShellKind::of(Path::new(p)), k, "{p}");
        }
    }

    #[test]
    fn powershell_resume_args_exact() {
        let sh = Shell::new(PathBuf::from("/usr/bin/pwsh"));
        assert_eq!(sh.resume_args(SID).unwrap(), vec!["-NoLogo", "-NoExit", "-Command", &format!("claude --resume {SID}")]);
        assert_eq!(sh.interactive_args(), vec!["-NoLogo"]);
    }

    #[test]
    fn posix_resume_execs_the_same_shell() {
        let sh = Shell::new(PathBuf::from("/usr/bin/fish"));
        assert_eq!(sh.resume_args(SID).unwrap(), vec!["-i", "-c", &format!("claude --resume {SID}; exec '/usr/bin/fish'")]);
        assert!(sh.interactive_args().is_empty());
        let odd = Shell::new(PathBuf::from("/opt/it's/bash"));
        assert_eq!(odd.resume_args(SID).unwrap_err(), crate::errors::SHELL_NOT_FOUND);
    }

    #[test]
    fn resume_refuses_anything_but_a_lowercase_uuid() {
        let sh = Shell::new(PathBuf::from("/bin/bash"));
        for bad in ["", "x; rm -rf ~", "F00DCAFE-0000-4000-8000-000000000001", &format!("{SID}; id")] {
            assert_eq!(sh.resume_args(bad).unwrap_err(), crate::errors::SESSION_INVALID, "{bad}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn unix_shell_prefers_a_valid_dollar_shell() {
        let sh = unix_shell(Some(std::ffi::OsStr::new("/bin/sh"))).unwrap();
        assert_eq!(sh.program, PathBuf::from("/bin/sh"));
        for bad in ["sh", "", "/yok/boyle/kabuk", "/bin/it's"] {
            let sh = unix_shell(Some(std::ffi::OsStr::new(bad))).unwrap();
            assert!(sh.program.is_absolute() && sh.program != Path::new(bad), "{bad}");
        }
        assert!(unix_shell(None).unwrap().program.is_absolute());
    }

    #[cfg(unix)]
    #[test]
    fn ids_resolve_to_installed_programs_or_fall_back() {
        let sh = find("bash").expect("bash is installed on the test machines");
        assert!(sh.program.is_absolute() && sh.kind == ShellKind::Bash);
        assert_eq!(find("powershell"), None, "Windows PowerShell only exists on Windows");
        assert_eq!(find("nope"), None);
        assert_eq!(for_setting("nope").unwrap(), default_shell().unwrap());
        let ids = installed();
        assert_eq!(ids[0], "auto");
        assert!(ids.contains(&"bash".to_string()) && !ids.contains(&"powershell".to_string()));
    }

    #[cfg(windows)]
    #[test]
    fn powershell_comes_from_the_system_directory() {
        let sys = crate::platform::system_dir();
        assert!(sys.is_absolute());
        assert!(sys.to_string_lossy().to_lowercase().ends_with("system32"), "{sys:?}");
        assert_eq!(powershell_exe(), sys.join("WindowsPowerShell").join("v1.0").join("powershell.exe"));
        assert_eq!(default_shell().unwrap().kind, ShellKind::PowerShell);
    }
}
