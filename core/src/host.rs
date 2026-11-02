//! Where a live Claude process runs: in one of Claude Pano's own terminals,
//! in a Windows Terminal tab, or somewhere else (VS Code, an IDE, ...).
//! Pure logic over a process table so it can be tested without Win32; the
//! platform layer fills the table from a Toolhelp snapshot.
use serde::Serialize;
use std::collections::HashMap;

/// What the UI needs to decide whether "Sekmeye git" can work.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum HostKind {
    /// A descendant of this app (one of its ConPTY terminals).
    Embedded,
    /// A descendant of a `WindowsTerminal.exe` process.
    WindowsTerminal,
    #[default]
    Other,
}

/// Result of the ancestor walk; `WindowsTerminal` carries the host's pid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Host {
    Embedded,
    WindowsTerminal(u32),
    Other,
}

impl Host {
    pub fn kind(self) -> HostKind {
        match self {
            Host::Embedded => HostKind::Embedded,
            Host::WindowsTerminal(_) => HostKind::WindowsTerminal,
            Host::Other => HostKind::Other,
        }
    }
}

/// One row of a process snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcInfo {
    pub parent: u32,
    /// Executable file name as Toolhelp reports it (`szExeFile`).
    pub exe: String,
}

/// Longest ancestor chain followed (guards against cycles from pid reuse).
const MAX_DEPTH: usize = 32;

const WINDOWS_TERMINAL: &str = "WindowsTerminal.exe";

/// Hosts that own their own terminal: a session below one of them is not in
/// a Windows Terminal tab even when the host itself was started from one
/// (e.g. `code .` typed in Windows Terminal), so the walk stops there.
const OTHER_HOSTS: &[&str] = &[
    "explorer.exe",
    "Code.exe",
    "Code - Insiders.exe",
    "Cursor.exe",
    "Windsurf.exe",
    "idea64.exe",
    "pycharm64.exe",
    "webstorm64.exe",
    "rider64.exe",
    "clion64.exe",
    "goland64.exe",
    "devenv.exe",
    "Zed.exe",
    "alacritty.exe",
    "WezTerm.exe",
    "wezterm-gui.exe",
    "mintty.exe",
    "ConEmu64.exe",
    "ConEmu.exe",
    "Tabby.exe",
    "Hyper.exe",
];

fn is(exe: &str, name: &str) -> bool {
    exe.eq_ignore_ascii_case(name)
}

/// Walks `pid`'s ancestors (nearest first). The first of these decides:
/// `self_pid` -> Embedded; `WindowsTerminal.exe` -> WindowsTerminal; a known
/// other host, a missing row, a cycle or a parent created after its child
/// (pid reuse: the real parent exited and its pid went to a newer process)
/// -> Other. `created` gives a process's creation time when it can be read.
pub fn find_host(
    pid: u32,
    self_pid: u32,
    table: &HashMap<u32, ProcInfo>,
    created: impl Fn(u32) -> Option<u64>,
) -> Host {
    let mut child = pid;
    let mut seen = vec![pid];
    for _ in 0..MAX_DEPTH {
        let Some(parent) = table.get(&child).map(|p| p.parent) else {
            return Host::Other;
        };
        if parent == 0 || seen.contains(&parent) {
            return Host::Other;
        }
        if let (Some(pc), Some(cc)) = (created(parent), created(child)) {
            if pc > cc {
                return Host::Other;
            }
        }
        if parent == self_pid {
            return Host::Embedded;
        }
        let Some(info) = table.get(&parent) else {
            return Host::Other;
        };
        if is(&info.exe, WINDOWS_TERMINAL) {
            return Host::WindowsTerminal(parent);
        }
        if OTHER_HOSTS.iter().any(|h| is(&info.exe, h)) {
            return Host::Other;
        }
        seen.push(parent);
        child = parent;
    }
    Host::Other
}

/// Whether a tab title names the session: equal (case-sensitive), or equal
/// after one leading status glyph and a space ("✳ ad", "⠂ ad") that Claude
/// Code puts in front of the title while it works.
pub fn tab_matches(title: &str, session_name: &str) -> bool {
    if session_name.is_empty() {
        return false;
    }
    if title == session_name {
        return true;
    }
    let mut chars = title.chars();
    match (chars.next(), chars.next()) {
        (Some(g), Some(' ')) if !g.is_alphanumeric() && !g.is_whitespace() => chars.as_str() == session_name,
        _ => false,
    }
}

/// Which tab to select, given the tab titles of each window of the host
/// (windows in z-order, frontmost first).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TabChoice {
    /// Exactly one tab matches: (window index, tab index).
    Select(usize, usize),
    /// Several tabs match; only the window of the first one is focused.
    Duplicate(usize),
    /// No tab matches; the frontmost window is focused.
    NoMatch,
}

pub fn choose_tab(windows: &[Vec<String>], session_name: &str) -> TabChoice {
    let hits: Vec<(usize, usize)> = windows
        .iter()
        .enumerate()
        .flat_map(|(w, tabs)| {
            tabs.iter().enumerate().filter(|(_, t)| tab_matches(t, session_name)).map(move |(i, _)| (w, i))
        })
        .collect();
    match hits.as_slice() {
        [] => TabChoice::NoMatch,
        [(w, t)] => TabChoice::Select(*w, *t),
        [(w, _), ..] => TabChoice::Duplicate(*w),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SELF: u32 = 100;

    /// pid -> (parent, exe)
    fn table(rows: &[(u32, u32, &str)]) -> HashMap<u32, ProcInfo> {
        rows.iter().map(|&(p, pp, exe)| (p, ProcInfo { parent: pp, exe: exe.into() })).collect()
    }

    fn no_times(_: u32) -> Option<u64> {
        None
    }

    #[test]
    fn claude_in_windows_terminal_tab() {
        let t = table(&[(10, 9, "claude.exe"), (9, 8, "pwsh.exe"), (8, 4, "WindowsTerminal.exe"), (4, 1, "explorer.exe")]);
        assert_eq!(find_host(10, SELF, &t, no_times), Host::WindowsTerminal(8));
    }

    #[test]
    fn exe_name_compared_case_insensitively_through_nested_shells() {
        let t = table(&[
            (10, 11, "node.exe"),
            (11, 12, "cmd.exe"),
            (12, 13, "powershell.exe"),
            (13, 8, "pwsh.exe"),
            (8, 4, "windowsterminal.EXE"),
        ]);
        assert_eq!(find_host(10, SELF, &t, no_times), Host::WindowsTerminal(8));
    }

    #[test]
    fn claude_in_embedded_terminal() {
        let t = table(&[(10, 9, "claude.exe"), (9, SELF, "powershell.exe"), (SELF, 8, "claude-pano.exe"), (8, 4, "WindowsTerminal.exe")]);
        // Our own process wins even when the app itself was started from WT.
        assert_eq!(find_host(10, SELF, &t, no_times), Host::Embedded);
    }

    #[test]
    fn vs_code_terminal_is_other_even_when_code_was_started_from_wt() {
        let t = table(&[(10, 9, "claude.exe"), (9, 7, "pwsh.exe"), (7, 6, "Code.exe"), (6, 8, "Code.exe"), (8, 4, "WindowsTerminal.exe")]);
        assert_eq!(find_host(10, SELF, &t, no_times), Host::Other);
    }

    #[test]
    fn missing_rows_cycles_and_unknown_pids_are_other() {
        assert_eq!(find_host(10, SELF, &table(&[]), no_times), Host::Other);
        let gap = table(&[(10, 9, "claude.exe")]); // parent 9 already exited
        assert_eq!(find_host(10, SELF, &gap, no_times), Host::Other);
        let cycle = table(&[(10, 9, "claude.exe"), (9, 10, "pwsh.exe")]);
        assert_eq!(find_host(10, SELF, &cycle, no_times), Host::Other);
        let to_idle = table(&[(10, 0, "claude.exe")]);
        assert_eq!(find_host(10, SELF, &to_idle, no_times), Host::Other);
    }

    #[test]
    fn chain_longer_than_max_depth_is_other() {
        let mut rows: Vec<(u32, u32, &str)> = (1000..1100).map(|p| (p, p + 1, "cmd.exe")).collect();
        rows.push((1100, 8, "cmd.exe"));
        rows.push((8, 4, "WindowsTerminal.exe"));
        assert_eq!(find_host(1000, SELF, &table(&rows), no_times), Host::Other);
    }

    #[test]
    fn reused_parent_pid_is_not_followed() {
        // pwsh (9) exited; its pid now belongs to a WindowsTerminal started later.
        let t = table(&[(10, 9, "claude.exe"), (9, 4, "WindowsTerminal.exe")]);
        let created = |p: u32| match p {
            10 => Some(500),
            9 => Some(900),
            _ => None,
        };
        assert_eq!(find_host(10, SELF, &t, created), Host::Other);
        let older = |p: u32| match p {
            10 => Some(500),
            9 => Some(100),
            _ => None,
        };
        assert_eq!(find_host(10, SELF, &t, older), Host::WindowsTerminal(9));
    }

    #[test]
    fn host_kind_serializes_camel_case() {
        assert_eq!(serde_json::to_string(&HostKind::WindowsTerminal).unwrap(), "\"windowsTerminal\"");
        assert_eq!(serde_json::to_string(&HostKind::Embedded).unwrap(), "\"embedded\"");
        assert_eq!(serde_json::to_string(&HostKind::Other).unwrap(), "\"other\"");
        assert_eq!(Host::WindowsTerminal(3).kind(), HostKind::WindowsTerminal);
    }

    #[test]
    fn tab_title_match_is_exact_and_case_sensitive() {
        assert!(tab_matches("proje-x", "proje-x"));
        assert!(!tab_matches("Proje-x", "proje-x"));
        assert!(!tab_matches("proje-x ", "proje-x"));
        assert!(!tab_matches("proje-xy", "proje-x"));
        assert!(!tab_matches("", ""));
        assert!(!tab_matches("x", ""));
    }

    #[test]
    fn tab_title_may_carry_one_status_glyph() {
        assert!(tab_matches("✳ proje-x", "proje-x"));
        assert!(tab_matches("⠂ proje-x", "proje-x"));
        assert!(!tab_matches("a proje-x", "proje-x"), "a letter is not a glyph");
        assert!(!tab_matches("✳proje-x", "proje-x"));
        assert!(!tab_matches("✳ ✳ proje-x", "proje-x"));
    }

    fn wins(w: &[&[&str]]) -> Vec<Vec<String>> {
        w.iter().map(|tabs| tabs.iter().map(|s| s.to_string()).collect()).collect()
    }

    #[test]
    fn choose_single_match_across_windows() {
        let w = wins(&[&["PowerShell", "diger"], &["x", "✳ ornek"]]);
        assert_eq!(choose_tab(&w, "ornek"), TabChoice::Select(1, 1));
    }

    #[test]
    fn duplicates_select_nothing_and_focus_first_window() {
        let same_window = wins(&[&["ornek", "ornek"]]);
        assert_eq!(choose_tab(&same_window, "ornek"), TabChoice::Duplicate(0));
        let two_windows = wins(&[&["a"], &["ornek"], &["✳ ornek"]]);
        assert_eq!(choose_tab(&two_windows, "ornek"), TabChoice::Duplicate(1));
    }

    #[test]
    fn no_match_and_no_windows() {
        assert_eq!(choose_tab(&wins(&[&["Ornek"]]), "ornek"), TabChoice::NoMatch);
        assert_eq!(choose_tab(&[], "ornek"), TabChoice::NoMatch);
    }
}
