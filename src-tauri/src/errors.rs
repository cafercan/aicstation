//! Stable error codes for the UI. Commands return a code, never prose; the UI
//! translates it (ui/src/i18n, keys `err.<code>`) and shows an unknown code
//! as sent. Shape: `code` or `code:detail`, where the detail is a number, an
//! OS message or another code (`resume_folder_mismatch:folder_network`).
//!
//! ui/src/i18n/i18n.test.ts reads this file: every code below must have a
//! translation in every UI language.

pub const SESSION_INVALID: &str = "session_invalid";
pub const SESSION_LIVE: &str = "session_live";
/// `session_resume_terminal_open[:<pty id>]`: claude runs (or is starting) in
/// the app terminal that resumed the session; the UI offers to switch to it.
pub const SESSION_RESUME_TERMINAL_OPEN: &str = "session_resume_terminal_open";
pub const SESSION_UNKNOWN: &str = "session_unknown";
pub const SESSION_NOT_LIVE: &str = "session_not_live";
/// `resume_folder_mismatch[:<folder code>]`: the session's first recorded cwd
/// is not usable (detail) or is not the project folder its transcript is in.
pub const RESUME_FOLDER_MISMATCH: &str = "resume_folder_mismatch";

pub const FOLDER_UNKNOWN: &str = "folder_unknown";
pub const FOLDER_NETWORK: &str = "folder_network";
pub const FOLDER_INVALID: &str = "folder_invalid";
pub const FOLDER_RELATIVE: &str = "folder_relative";
pub const FOLDER_MISSING: &str = "folder_missing";
pub const FOLDER_OPEN_FAILED: &str = "folder_open_failed";

pub const PTY_UNKNOWN: &str = "pty_unknown";
pub const PTY_LIMIT: &str = "pty_limit";
pub const PTY_TOO_LARGE: &str = "pty_too_large";
pub const PTY_SPAWN_FAILED: &str = "pty_spawn_failed";
pub const PTY_IO: &str = "pty_io";
pub const PTY_RESIZE_FAILED: &str = "pty_resize_failed";
pub const INTERNAL_LOCK: &str = "internal_lock";
pub const TASK_FAILED: &str = "task_failed";

pub const WT_NOT_FOUND: &str = "wt_not_found";
pub const WT_OPEN_FAILED: &str = "wt_open_failed";
pub const WT_HOME_INVALID: &str = "wt_home_invalid";
pub const WT_HOME_MISSING: &str = "wt_home_missing";
pub const WT_POWERSHELL_INVALID: &str = "wt_powershell_invalid";

/// Error: no Windows Terminal tab for the session.
pub const TAB_NOT_FOUND: &str = "tab_not_found";
/// Note (Ok): window focused, several tabs carry the session's name.
pub const TAB_AMBIGUOUS: &str = "tab_ambiguous";
/// Note (Ok): window focused, the session's tab could not be selected.
pub const TAB_NO_MATCH: &str = "tab_no_match";
/// Note (Ok): window focused, the one matching tab refused to be selected.
pub const TAB_SELECT_FAILED: &str = "tab_select_failed";

/// Settings sent by the UI carry an invalid value (e.g. an unknown language).
pub const SETTINGS_INVALID: &str = "settings_invalid";
/// `settings_save_failed:<OS message>`: settings.json could not be written.
pub const SETTINGS_SAVE_FAILED: &str = "settings_save_failed";

/// `code:detail`.
pub fn with(code: &str, detail: impl std::fmt::Display) -> String {
    format!("{code}:{detail}")
}

#[cfg(test)]
pub const ALL: &[&str] = &[
    SESSION_INVALID, SESSION_LIVE, SESSION_RESUME_TERMINAL_OPEN, SESSION_UNKNOWN, SESSION_NOT_LIVE, RESUME_FOLDER_MISMATCH,
    FOLDER_UNKNOWN, FOLDER_NETWORK, FOLDER_INVALID, FOLDER_RELATIVE, FOLDER_MISSING, FOLDER_OPEN_FAILED,
    PTY_UNKNOWN, PTY_LIMIT, PTY_TOO_LARGE, PTY_SPAWN_FAILED, PTY_IO, PTY_RESIZE_FAILED, INTERNAL_LOCK, TASK_FAILED,
    WT_NOT_FOUND, WT_OPEN_FAILED, WT_HOME_INVALID, WT_HOME_MISSING, WT_POWERSHELL_INVALID,
    TAB_NOT_FOUND, TAB_AMBIGUOUS, TAB_NO_MATCH, TAB_SELECT_FAILED,
    SETTINGS_INVALID, SETTINGS_SAVE_FAILED,
];

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn codes_are_unique_ascii_snake_case() {
        let mut seen = HashSet::new();
        for c in ALL {
            assert!(!c.is_empty() && c.bytes().all(|b| b.is_ascii_lowercase() || b == b'_'), "{c}");
            assert!(seen.insert(*c), "duplicate {c}");
        }
    }

    #[test]
    fn with_joins_code_and_detail() {
        assert_eq!(with(PTY_LIMIT, 32), "pty_limit:32");
        assert_eq!(with(RESUME_FOLDER_MISMATCH, FOLDER_NETWORK), "resume_folder_mismatch:folder_network");
    }
}
