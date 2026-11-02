//! "Windows Terminal'de aç": starts a new PowerShell tab in the most recent
//! Windows Terminal window, in the user's home directory. Nothing comes from
//! the UI: the program is resolved from `%LOCALAPPDATA%` (fallback PATH) and
//! the arguments are fixed. No shell is involved, so nothing is interpolated.
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

pub const WT_NOT_FOUND: &str = "Windows Terminal bulunamadı";

/// `wt.exe -w 0 nt -d <home> powershell.exe -NoLogo`.
pub fn wt_args(home: &Path) -> Result<Vec<OsString>, String> {
    // wt splits its own command line on `;` (subcommand separator), even
    // inside quotes; a relative directory would resolve against our cwd.
    if !home.is_absolute() || home.as_os_str().to_string_lossy().contains(';') {
        return Err("Kullanıcı klasörü geçersiz".into());
    }
    let mut a: Vec<OsString> = ["-w", "0", "nt", "-d"].iter().map(OsString::from).collect();
    a.push(home.as_os_str().to_owned());
    a.push("powershell.exe".into());
    a.push("-NoLogo".into());
    Ok(a)
}

/// The App Execution Alias in `%LOCALAPPDATA%\Microsoft\WindowsApps` when it
/// exists, otherwise plain `wt.exe` (PATH lookup).
pub fn wt_program(local_app_data: Option<&OsStr>, exists: impl Fn(&Path) -> bool) -> PathBuf {
    if let Some(base) = local_app_data.map(Path::new).filter(|p| p.is_absolute()) {
        let alias = base.join(r"Microsoft\WindowsApps\wt.exe");
        if exists(&alias) {
            return alias;
        }
    }
    PathBuf::from("wt.exe")
}

/// Spawns Windows Terminal with the fixed arguments; takes no input.
pub fn open() -> Result<(), String> {
    let home = std::env::var_os("USERPROFILE").ok_or("Kullanıcı klasörü bulunamadı")?;
    let args = wt_args(Path::new(&home))?;
    // The alias is a reparse point that `metadata` may fail to follow.
    let program = wt_program(std::env::var_os("LOCALAPPDATA").as_deref(), |p| {
        std::fs::symlink_metadata(p).is_ok()
    });
    match std::process::Command::new(program).args(args).spawn() {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(WT_NOT_FOUND.into()),
        Err(e) => Err(format!("Windows Terminal açılamadı: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strs(v: &[OsString]) -> Vec<&str> {
        v.iter().map(|s| s.to_str().unwrap()).collect()
    }

    #[test]
    fn args_are_fixed_new_tab_in_home_with_powershell() {
        let a = wt_args(Path::new(r"C:\Users\Ayşe Yılmaz")).unwrap();
        assert_eq!(
            strs(&a),
            ["-w", "0", "nt", "-d", r"C:\Users\Ayşe Yılmaz", "powershell.exe", "-NoLogo"]
        );
    }

    #[test]
    fn args_reject_home_wt_would_misparse() {
        // `;` separates wt subcommands; a relative or empty path is not a home.
        assert!(wt_args(Path::new(r"C:\Users\a;b")).is_err());
        assert!(wt_args(Path::new(r"Users\x")).is_err());
        assert!(wt_args(Path::new("")).is_err());
    }

    #[test]
    fn program_prefers_windowsapps_alias() {
        let p = wt_program(Some(OsStr::new(r"C:\Users\x\AppData\Local")), |p| {
            p == Path::new(r"C:\Users\x\AppData\Local\Microsoft\WindowsApps\wt.exe")
        });
        assert_eq!(p, PathBuf::from(r"C:\Users\x\AppData\Local\Microsoft\WindowsApps\wt.exe"));
    }

    #[test]
    fn program_falls_back_to_path_lookup_only_when_alias_absent() {
        let p = wt_program(Some(OsStr::new(r"C:\Users\x\AppData\Local")), |_| false);
        assert_eq!(p, PathBuf::from("wt.exe"));
        assert_eq!(wt_program(None, |_| true), PathBuf::from("wt.exe"));
        // A relative LOCALAPPDATA is ignored (would resolve against the cwd).
        assert_eq!(wt_program(Some(OsStr::new(r"rel\dir")), |_| true), PathBuf::from("wt.exe"));
    }
}
