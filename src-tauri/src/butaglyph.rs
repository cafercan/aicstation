//! butaglyph (`buta`, github.com/Huseynteymurzade28/butaglyph) as the
//! prompt of the embedded terminals. The app never edits the user's
//! butaglyph config: a chosen preset is written by `buta preset <name>` into
//! the app's own folder and handed to the prompt as `BUTA_CONFIG`.
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// Longest output of a `buta` call that is read.
const MAX_OUTPUT: usize = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub installed: bool,
    pub version: Option<String>,
    pub presets: Vec<Preset>,
}

static PRESET_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Folder for the preset configs (`<app data>/prompts`); set at startup.
pub fn set_preset_dir(dir: PathBuf) {
    let _ = PRESET_DIR.set(dir);
}

/// The `buta` binary: a butaglyph the user installed (PATH, system bins
/// first, see `platform::find_program`; then `~/.local/bin`, where its
/// README installs it, which an app started from the desktop menu may not
/// have in PATH), else the one the app's packages ship next to the app.
#[cfg(unix)]
pub fn find() -> Option<PathBuf> {
    use std::ffi::OsStr;
    crate::platform::find_program(OsStr::new("buta"), std::env::var_os("PATH").as_deref())
        .or_else(|| {
            let home = crate::shell::home_dir()?;
            crate::platform::find_program(home.join(".local/bin/buta").as_os_str(), None)
        })
        .or_else(|| bundled_in(&app_dir()?))
}

/// Windows: `%USERPROFILE%\.local\bin\buta.exe`, as its README installs it,
/// else the one the installer puts next to the app.
#[cfg(windows)]
pub fn find() -> Option<PathBuf> {
    let p = crate::shell::home_dir()?.join(".local").join("bin").join("buta.exe");
    p.is_file().then_some(p).or_else(|| bundled_in(&app_dir()?))
}

/// Folder of the running app's executable.
fn app_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(Path::to_path_buf)
}

/// The `buta` shipped with the app in `dir` (Linux packages: Tauri's
/// sidecar; Windows: the installer's `buta.exe`).
#[cfg(unix)]
fn bundled_in(dir: &Path) -> Option<PathBuf> {
    crate::platform::find_program(dir.join("buta").as_os_str(), None)
}

#[cfg(windows)]
fn bundled_in(dir: &Path) -> Option<PathBuf> {
    let p = dir.join("buta.exe");
    p.is_file().then_some(p)
}

/// stdout of `buta <args>` when it succeeds, with the cleaned environment,
/// colors off.
fn run(buta: &Path, args: &[&str]) -> Option<String> {
    run_env(buta, args, &[("NO_COLOR", "1")])
}

fn run_env(buta: &Path, args: &[&str], env: &[(&str, &str)]) -> Option<String> {
    let mut cmd = Command::new(buta);
    // buta is a console program and the app is not: without this every call
    // flashes an empty console window (several when the settings page opens).
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    let out = cmd
        .args(args)
        .env_clear()
        .envs(crate::pty::child_env(std::env::vars_os(), &Default::default()))
        .env_remove("NO_COLOR")
        .envs(env.iter().copied())
        .output()
        .ok()?;
    if !out.status.success() || out.stdout.len() > MAX_OUTPUT {
        return None;
    }
    String::from_utf8(out.stdout).ok()
}

/// Presets from `buta preset`: one `name  description` per line until the
/// first blank line; a name that is not a valid preset id is skipped.
pub fn parse_presets(text: &str) -> Vec<Preset> {
    text.lines()
        .take_while(|l| !l.trim().is_empty())
        .filter_map(|l| {
            let (name, desc) = l.split_once(char::is_whitespace).unwrap_or((l, ""));
            (!name.is_empty() && crate::settings::valid_preset(name))
                .then(|| Preset { name: name.into(), description: station_core::model::cap_display(desc.trim()) })
        })
        .collect()
}

/// Whether `buta` is installed, its version and presets.
pub fn info() -> Info {
    let Some(buta) = find() else { return Info::default() };
    let version = run(&buta, &["--version"]).map(|v| station_core::model::cap_display(v.trim()));
    let presets = run(&buta, &["preset"]).map(|t| parse_presets(&t)).unwrap_or_default();
    Info { installed: true, version, presets }
}

/// The config file of preset `name`, written from `buta preset <name>` into
/// the preset folder (refreshed each time, so a newer buta's version of the
/// preset is used). None for an invalid name or when buta refuses it.
pub fn preset_config(buta: &Path, name: &str) -> Option<PathBuf> {
    if name.is_empty() || !crate::settings::valid_preset(name) {
        return None;
    }
    let dir = PRESET_DIR.get()?;
    let text = run(buta, &["preset", name])?;
    std::fs::create_dir_all(dir).ok()?;
    let path = dir.join(format!("{name}.zon"));
    if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
        let tmp = dir.join(format!("{name}.zon.tmp"));
        std::fs::write(&tmp, &text).and_then(|()| std::fs::rename(&tmp, &path)).ok()?;
    }
    Some(path)
}

/// The prompt part of a terminal's startup for the stored settings: buta
/// and, for a preset, `BUTA_CONFIG`. Empty when the prompt is the shell's
/// own or buta is not installed (the terminal still opens).
pub fn startup(prompt: &str, preset: &str) -> crate::integration::Startup {
    let mut s = crate::integration::Startup::default();
    if prompt != "butaglyph" {
        return s;
    }
    let Some(buta) = find() else { return s };
    if let Some(cfg) = preset_config(&buta, preset) {
        s.env.push(("BUTA_CONFIG".into(), cfg.to_string_lossy().into_owned()));
    }
    s.buta = Some(buta);
    s
}

/// Sample prompts drawn by the real `buta` with `preset` (empty: the
/// user's own config) in the home folder: after a successful command, and
/// after a failed slow one. ANSI text for the UI's preview terminal.
pub fn preview(preset: &str) -> Result<Vec<String>, String> {
    if !crate::settings::valid_preset(preset) {
        return Err(crate::errors::SETTINGS_INVALID.into());
    }
    let buta = find().ok_or(crate::errors::BUTAGLYPH_NOT_FOUND)?;
    let config = if preset.is_empty() {
        None
    } else {
        Some(preset_config(&buta, preset).ok_or(crate::errors::BUTAGLYPH_FAILED)?)
    };
    let config = config.as_ref().map(|c| c.to_string_lossy().into_owned());
    let mut env: Vec<(&str, &str)> = vec![("COLORTERM", "truecolor"), ("TERM", "xterm-256color")];
    if let Some(c) = &config {
        env.push(("BUTA_CONFIG", c));
    }
    let home = crate::shell::home_dir().map(|h| h.to_string_lossy().into_owned()).unwrap_or_else(|| "/".into());
    let mut out = Vec::new();
    for (status, ms) in [("0", "40"), ("1", "3200")] {
        let args = ["prompt", "--status", status, "--duration-ms", ms, "--cwd", &home];
        let text = run_env(&buta, &args, &env).ok_or(crate::errors::BUTAGLYPH_FAILED)?;
        out.push(text.chars().take(4096).collect());
    }
    Ok(out)
}

/// `buta config`: butaglyph's own editor in the browser (on 127.0.0.1),
/// for the user's own config. Takes nothing from the UI.
pub fn open_editor() -> Result<(), String> {
    let buta = find().ok_or(crate::errors::BUTAGLYPH_NOT_FOUND)?;
    let mut child = Command::new(buta)
        .arg("config")
        .env_clear()
        .envs(crate::pty::child_env(std::env::vars_os(), &Default::default()))
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| crate::errors::with(crate::errors::BUTAGLYPH_FAILED, e))?;
    std::thread::spawn(move || child.wait());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_list_is_parsed_and_filtered() {
        let text = "carpet       butaglyph's own look.\n\
                    tokyo-night  the Tokyo Night palette.\n\
                    ../evil      nope\n\
                    \n\
                    Try them with `buta preview`.\n";
        let p = parse_presets(text);
        assert_eq!(p.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["carpet", "tokyo-night"]);
        assert_eq!(p[1].description, "the Tokyo Night palette.");
    }

    #[test]
    fn own_prompt_or_missing_buta_injects_nothing() {
        assert_eq!(startup("own", "neon"), crate::integration::Startup::default());
        assert_eq!(preset_config(Path::new("/yok/buta"), "../x"), None);
        assert_eq!(preset_config(Path::new("/yok/buta"), ""), None);
    }

    /// With the real buta, when installed: two colored prompts, the second
    /// one different (failed status).
    #[test]
    fn real_buta_preview_draws_two_colored_prompts() {
        if find().is_none() {
            eprintln!("buta not installed; skipping");
            return;
        }
        let tmp = tempfile::tempdir().unwrap();
        set_preset_dir(tmp.path().join("prompts"));
        if PRESET_DIR.get() != Some(&tmp.path().join("prompts")) {
            return; // another test set the folder first
        }
        let p = preview("minimal").unwrap();
        assert_eq!(p.len(), 2);
        assert!(p.iter().all(|s| s.contains("\x1b[")), "{p:?}");
        assert_ne!(p[0], p[1]);
    }

    #[test]
    fn bundled_buta_is_found_next_to_the_app() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(bundled_in(tmp.path()), None);
        let name = if cfg!(windows) { "buta.exe" } else { "buta" };
        let buta = tmp.path().join(name);
        std::fs::write(&buta, "").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(bundled_in(tmp.path()), None, "not executable");
            std::fs::set_permissions(&buta, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        assert_eq!(bundled_in(tmp.path()), Some(buta));
    }

    #[test]
    fn preview_refuses_an_invalid_preset_before_running_anything() {
        assert_eq!(preview("../x").unwrap_err(), crate::errors::SETTINGS_INVALID);
    }

    #[cfg(unix)]
    #[test]
    fn preset_config_comes_from_buta() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let buta = tmp.path().join("buta");
        std::fs::write(&buta, "#!/bin/sh\n[ \"$1\" = preset ] && [ \"$2\" = neon ] && echo '.{ .glyphs = .ascii }' && exit 0\nexit 1\n").unwrap();
        std::fs::set_permissions(&buta, std::fs::Permissions::from_mode(0o755)).unwrap();
        set_preset_dir(tmp.path().join("prompts"));
        let dir = PRESET_DIR.get().unwrap().clone();
        let p = preset_config(&buta, "neon").expect("written");
        assert_eq!(p, dir.join("neon.zon"));
        assert_eq!(std::fs::read_to_string(&p).unwrap(), ".{ .glyphs = .ascii }\n");
        assert_eq!(preset_config(&buta, "yok"), None, "buta refuses an unknown preset");
    }
}
