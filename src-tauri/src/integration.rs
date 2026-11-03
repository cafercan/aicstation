//! Shell integration of the embedded terminals: what runs after the user's
//! own rc files, without ever editing them. It can load a butaglyph prompt
//! and run `claude --resume <id>`, after which the same interactive shell
//! stays open.
//!
//! - bash: `--rcfile <dir>/bashrc` (sources `~/.bashrc` first)
//! - zsh: `ZDOTDIR=<dir>/zsh`, whose `.zshenv`/`.zshrc` source the user's
//!   own (from their original `ZDOTDIR` or `$HOME`) and then put it back
//! - fish: `-C <init>` (runs after config.fish)
//! - PowerShell: `-NoExit -Command <init>` (runs after `$PROFILE`)
//!
//! The scripts are constants; the values reach them only through the
//! environment (`SESSION_STATION_*`), so nothing is interpolated into a
//! command line. Shells without integration (sh, dash, nushell, cmd) still
//! resume, but get no injected prompt.
use crate::shell::{Shell, ShellKind};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Path of the `buta` binary for the prompt.
pub const ENV_BUTA: &str = "SESSION_STATION_BUTA";
/// Session id to resume (a validated lowercase UUID).
pub const ENV_RESUME: &str = "SESSION_STATION_RESUME";
const ENV_USER_ZDOTDIR: &str = "SESSION_STATION_USER_ZDOTDIR";

const BASHRC: &str = r#"# SessionStation: runs instead of ~/.bashrc, then sources it.
[ -f ~/.bashrc ] && . ~/.bashrc
if [ -n "${SESSION_STATION_BUTA-}" ]; then
    eval "$("$SESSION_STATION_BUTA" init bash)"
    # buta runs first (it needs $?), but a prompt the user's rc set up
    # (starship, oh-my-bash...) would draw over it later in PROMPT_COMMAND:
    # keep buta's PS1 and put it back last.
    if declare -F __buta_prompt_command >/dev/null; then
        eval "__ss_buta_pc() $(declare -f __buta_prompt_command | tail -n +2)"
        __buta_prompt_command() { __ss_buta_pc; local s=$?; __ss_buta_ps1=$PS1; return $s; }
        __ss_buta_last() { local s=$?; PS1=${__ss_buta_ps1-$PS1}; return $s; }
        if [[ $(declare -p PROMPT_COMMAND 2>/dev/null) == "declare -a"* ]]; then
            PROMPT_COMMAND+=(__ss_buta_last)
        else
            PROMPT_COMMAND="${PROMPT_COMMAND:+$PROMPT_COMMAND;}__ss_buta_last"
        fi
    fi
fi
unset SESSION_STATION_BUTA
if [ -n "${SESSION_STATION_RESUME-}" ]; then
    __ss_resume=$SESSION_STATION_RESUME
    unset SESSION_STATION_RESUME
    claude --resume "$__ss_resume"
    unset __ss_resume
fi
"#;

/// zsh reads `.zshenv` then `.zshrc` from `$ZDOTDIR`. The user's real
/// ZDOTDIR is active while their files run, ours in between.
const ZSHENV: &str = r#"# SessionStation: sources the user's .zshenv, then hands .zshrc back to us.
__ss_zdotdir=$ZDOTDIR
if [ -n "${SESSION_STATION_USER_ZDOTDIR+x}" ]; then
    ZDOTDIR=$SESSION_STATION_USER_ZDOTDIR
else
    unset ZDOTDIR
fi
[ -f "${ZDOTDIR:-$HOME}/.zshenv" ] && . "${ZDOTDIR:-$HOME}/.zshenv"
if [ -n "${ZDOTDIR+x}" ]; then
    export SESSION_STATION_USER_ZDOTDIR=$ZDOTDIR
else
    unset SESSION_STATION_USER_ZDOTDIR
fi
ZDOTDIR=$__ss_zdotdir
unset __ss_zdotdir
"#;

const ZSHRC: &str = r#"# SessionStation: sources the user's .zshrc, then the prompt and resume.
if [ -n "${SESSION_STATION_USER_ZDOTDIR+x}" ]; then
    ZDOTDIR=$SESSION_STATION_USER_ZDOTDIR
else
    unset ZDOTDIR
fi
unset SESSION_STATION_USER_ZDOTDIR
[ -f "${ZDOTDIR:-$HOME}/.zshrc" ] && . "${ZDOTDIR:-$HOME}/.zshrc"
if [ -n "${SESSION_STATION_BUTA-}" ]; then
    eval "$("$SESSION_STATION_BUTA" init zsh)"
    # Put buta's prompt back after any precmd hook of the user's rc.
    __ss_buta_last() { PROMPT='${__buta_prompt}'; }
    precmd_functions+=(__ss_buta_last)
fi
unset SESSION_STATION_BUTA
if [ -n "${SESSION_STATION_RESUME-}" ]; then
    __ss_resume=$SESSION_STATION_RESUME
    unset SESSION_STATION_RESUME
    claude --resume "$__ss_resume"
    unset __ss_resume
fi
"#;

const FISH_INIT: &str = "if set -q SESSION_STATION_BUTA; $SESSION_STATION_BUTA init fish | source; end; \
set -e SESSION_STATION_BUTA; \
if set -q SESSION_STATION_RESUME; set -l r $SESSION_STATION_RESUME; set -e SESSION_STATION_RESUME; claude --resume $r; end";

const PWSH_INIT: &str = "if ($env:SESSION_STATION_BUTA) { Invoke-Expression (& $env:SESSION_STATION_BUTA init powershell | Out-String) }; \
Remove-Item Env:SESSION_STATION_BUTA -ErrorAction SilentlyContinue; \
if ($env:SESSION_STATION_RESUME) { $r = $env:SESSION_STATION_RESUME; Remove-Item Env:SESSION_STATION_RESUME; claude --resume $r }";

static DIR: OnceLock<PathBuf> = OnceLock::new();

/// Writes the bash and zsh scripts under `dir` (when missing or changed)
/// and remembers it. Called once at startup; on failure bash and zsh run
/// without integration.
pub fn install(dir: &Path) -> std::io::Result<()> {
    let write = |p: &Path, text: &str| -> std::io::Result<()> {
        if std::fs::read_to_string(p).ok().as_deref() != Some(text) {
            std::fs::write(p, text)?;
        }
        Ok(())
    };
    std::fs::create_dir_all(dir.join("zsh"))?;
    write(&dir.join("bashrc"), BASHRC)?;
    write(&dir.join("zsh").join(".zshenv"), ZSHENV)?;
    write(&dir.join("zsh").join(".zshrc"), ZSHRC)?;
    let _ = DIR.set(dir.to_path_buf());
    Ok(())
}

/// What runs after the user's rc files.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Startup {
    /// `buta` binary: load its prompt.
    pub buta: Option<PathBuf>,
    /// Extra environment for the prompt (e.g. `BUTA_CONFIG`).
    pub env: Vec<(String, String)>,
    /// Session to resume (validated by the caller and again here).
    pub resume: Option<String>,
}

/// Program, arguments and extra environment of one terminal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
}

/// The launch of `shell` with `startup`, using the installed scripts.
pub fn launch(shell: &Shell, startup: &Startup) -> Result<Launch, String> {
    launch_with(shell, startup, DIR.get().map(PathBuf::as_path), std::env::var_os("ZDOTDIR"))
}

/// [`launch`] with the script folder and the user's `ZDOTDIR` passed in.
pub fn launch_with(
    shell: &Shell,
    startup: &Startup,
    dir: Option<&Path>,
    user_zdotdir: Option<std::ffi::OsString>,
) -> Result<Launch, String> {
    if let Some(id) = &startup.resume {
        if !station_core::locate::is_session_id(id) {
            return Err(crate::errors::SESSION_INVALID.into());
        }
    }
    let mut env: Vec<(String, String)> = Vec::new();
    let integrated = match shell.kind {
        ShellKind::Bash | ShellKind::Zsh => dir.is_some(),
        ShellKind::Fish | ShellKind::PowerShell => true,
        _ => false,
    };
    let wanted = startup.buta.is_some() || startup.resume.is_some();
    let mut args = shell.interactive_args();
    if !(integrated && wanted) {
        // Nothing to inject, or a shell without integration: the plain
        // shell, or the shell's own way of resuming.
        if let Some(id) = &startup.resume {
            args = shell.resume_args(id)?;
        }
        return Ok(Launch { program: shell.program.clone(), args, env });
    }
    if let Some(b) = &startup.buta {
        env.push((ENV_BUTA.into(), b.to_string_lossy().into_owned()));
        env.extend(startup.env.iter().cloned());
    }
    if let Some(id) = &startup.resume {
        env.push((ENV_RESUME.into(), id.clone()));
    }
    match shell.kind {
        ShellKind::Bash => {
            let rc = dir.expect("integrated").join("bashrc");
            args = vec!["--rcfile".into(), rc.to_string_lossy().into_owned()];
        }
        ShellKind::Zsh => {
            env.push(("ZDOTDIR".into(), dir.expect("integrated").join("zsh").to_string_lossy().into_owned()));
            if let Some(z) = user_zdotdir {
                env.push((ENV_USER_ZDOTDIR.into(), z.to_string_lossy().into_owned()));
            }
        }
        ShellKind::Fish => args = vec!["-C".into(), FISH_INIT.into()],
        ShellKind::PowerShell => {
            args = vec!["-NoLogo".into(), "-NoExit".into(), "-Command".into(), PWSH_INIT.into()];
        }
        _ => unreachable!("not integrated"),
    }
    Ok(Launch { program: shell.program.clone(), args, env })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SID: &str = "f00dcafe-0000-4000-8000-000000000001";

    fn sh(p: &str) -> Shell {
        Shell::new(PathBuf::from(p))
    }

    fn resume() -> Startup {
        Startup { resume: Some(SID.into()), ..Startup::default() }
    }

    fn with_buta() -> Startup {
        Startup {
            buta: Some(PathBuf::from("/opt/buta")),
            env: vec![("BUTA_CONFIG".into(), "/data/prompts/neon.zon".into())],
            resume: None,
        }
    }

    #[test]
    fn nothing_to_inject_is_the_plain_shell() {
        let l = launch_with(&sh("/bin/bash"), &Startup::default(), Some(Path::new("/d")), None).unwrap();
        assert_eq!((l.args.len(), l.env.len()), (0, 0));
        let l = launch_with(&sh("/usr/bin/pwsh"), &Startup::default(), None, None).unwrap();
        assert_eq!(l.args, vec!["-NoLogo"]);
    }

    #[test]
    fn bash_gets_the_rcfile_and_values_through_the_environment() {
        let l = launch_with(&sh("/bin/bash"), &resume(), Some(Path::new("/d")), None).unwrap();
        let rc = Path::new("/d").join("bashrc").to_string_lossy().into_owned();
        assert_eq!(l.args, vec!["--rcfile".to_string(), rc]);
        assert_eq!(l.env, vec![(ENV_RESUME.to_string(), SID.to_string())]);
        let l = launch_with(&sh("/bin/bash"), &with_buta(), Some(Path::new("/d")), None).unwrap();
        assert_eq!(
            l.env,
            vec![
                (ENV_BUTA.to_string(), "/opt/buta".to_string()),
                ("BUTA_CONFIG".to_string(), "/data/prompts/neon.zon".to_string())
            ]
        );
    }

    #[test]
    fn zsh_swaps_zdotdir_and_remembers_the_users() {
        let l = launch_with(&sh("/bin/zsh"), &resume(), Some(Path::new("/d")), Some("/home/u/.zsh".into())).unwrap();
        assert!(l.args.is_empty());
        let zdot = Path::new("/d").join("zsh").to_string_lossy().into_owned();
        assert!(l.env.contains(&("ZDOTDIR".into(), zdot)), "{:?}", l.env);
        assert!(l.env.contains(&(ENV_USER_ZDOTDIR.into(), "/home/u/.zsh".into())));
        let l = launch_with(&sh("/bin/zsh"), &resume(), Some(Path::new("/d")), None).unwrap();
        assert!(!l.env.iter().any(|(k, _)| k == ENV_USER_ZDOTDIR));
    }

    #[test]
    fn fish_and_powershell_take_a_fixed_init() {
        let l = launch_with(&sh("/usr/bin/fish"), &resume(), None, None).unwrap();
        assert_eq!(l.args, vec!["-C", FISH_INIT]);
        let l = launch_with(&sh("/usr/bin/pwsh"), &with_buta(), None, None).unwrap();
        assert_eq!(l.args, vec!["-NoLogo", "-NoExit", "-Command", PWSH_INIT]);
        assert!(!FISH_INIT.contains(SID) && !PWSH_INIT.contains("/opt/buta"), "values never in the command line");
    }

    #[test]
    fn without_scripts_or_integration_resume_still_works() {
        // bash whose scripts could not be written: the exec form.
        let l = launch_with(&sh("/bin/bash"), &with_buta(), None, None).unwrap();
        assert!(l.args.is_empty() && l.env.is_empty(), "no prompt injected");
        let l = launch_with(&sh("/bin/bash"), &resume(), None, None).unwrap();
        assert_eq!(l.args, sh("/bin/bash").resume_args(SID).unwrap());
        let l = launch_with(&sh("/usr/bin/nu"), &resume(), None, None).unwrap();
        assert_eq!(l.args, vec!["-e".to_string(), format!("claude --resume {SID}")]);
    }

    #[test]
    fn resume_id_is_validated_again() {
        let bad = Startup { resume: Some("x; id".into()), ..Startup::default() };
        assert_eq!(launch_with(&sh("/bin/bash"), &bad, Some(Path::new("/d")), None).unwrap_err(), crate::errors::SESSION_INVALID);
    }

    /// Runs the real shell with the launch, a fake `claude` that prints its
    /// arguments, a fake buta and a user rc that prints a marker; `-c` ends
    /// the shell right after the startup ran.
    #[cfg(unix)]
    fn run_real(kind: &str, startup: &Startup) -> Option<String> {
        use std::os::unix::fs::PermissionsExt;
        let shell = crate::shell::find(kind)?;
        let tmp = tempfile::tempdir().unwrap();
        let (home, bin, dir) = (tmp.path().join("home"), tmp.path().join("bin"), tmp.path().join("integ"));
        std::fs::create_dir_all(home.join(".config/fish")).unwrap();
        std::fs::create_dir_all(&bin).unwrap();
        install(&dir).unwrap();
        let exe = |name: &str, body: &str| {
            let p = bin.join(name);
            std::fs::write(&p, format!("#!/bin/sh\n{body}\n")).unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            p
        };
        exe("claude", r#"echo "claude-ran:$*""#);
        // `buta init <shell>` prints a line in that shell's syntax.
        let buta = exe("buta", r#"echo "echo buta-init:$2:\$BUTA_CONFIG""#);
        std::fs::write(home.join(".bashrc"), "echo user-rc\n").unwrap();
        std::fs::write(home.join(".zshrc"), "echo user-rc\n").unwrap();
        std::fs::write(home.join(".zshenv"), "echo user-env\n").unwrap();
        std::fs::write(home.join(".config/fish/config.fish"), "echo user-rc\n").unwrap();
        let startup = Startup { buta: startup.buta.as_ref().map(|_| buta.clone()), ..startup.clone() };
        let l = launch_with(&shell, &startup, Some(&dir), None).unwrap();
        let mut cmd = std::process::Command::new(&l.program);
        let check = if kind == "fish" {
            "if set -q SESSION_STATION_RESUME; echo after--$SESSION_STATION_RESUME; else; echo after--gone; end"
        } else {
            "echo after-$ZDOTDIR-${SESSION_STATION_RESUME:-gone}"
        };
        cmd.args(&l.args).args(["-i", "-c", check]);
        cmd.env_clear().env("HOME", &home).env("PATH", format!("{}:/usr/bin:/bin", bin.display()));
        cmd.env("TERM", "dumb").envs(l.env.iter().map(|(k, v)| (k, v)));
        let out = cmd.output().unwrap();
        Some(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    #[cfg(unix)]
    #[test]
    fn real_shells_source_the_user_rc_then_prompt_then_resume() {
        let both = Startup {
            buta: Some(PathBuf::from("x")),
            env: vec![("BUTA_CONFIG".into(), "/p/neon.zon".into())],
            resume: Some(SID.into()),
        };
        let mut ran = 0;
        for kind in ["bash", "zsh", "fish"] {
            let Some(out) = run_real(kind, &both) else { continue };
            ran += 1;
            let at = |s: &str| out.find(s).unwrap_or_else(|| panic!("{kind}: no {s:?} in {out:?}"));
            let (rc, prompt, claude) = (at("user-rc"), at(&format!("buta-init:{kind}:/p/neon.zon")), at(&format!("claude-ran:--resume {SID}")));
            assert!(rc < prompt && prompt < claude, "{kind} order: {out:?}");
            // The id is consumed; zsh's ZDOTDIR is the user's again (unset).
            assert!(out.contains("after--gone"), "{kind}: {out:?}");
            if kind == "zsh" {
                assert!(out.find("user-env").unwrap() < rc, "{out:?}");
            }
        }
        assert!(ran > 0, "no shell to test with");
    }

    #[test]
    fn install_writes_the_scripts_once() {
        let tmp = tempfile::tempdir().unwrap();
        install(tmp.path()).unwrap();
        assert_eq!(std::fs::read_to_string(tmp.path().join("bashrc")).unwrap(), BASHRC);
        assert_eq!(std::fs::read_to_string(tmp.path().join("zsh/.zshrc")).unwrap(), ZSHRC);
        install(tmp.path()).unwrap();
    }
}
