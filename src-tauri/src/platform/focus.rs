//! "Sekmeye git" for a session in Windows Terminal: find the WindowsTerminal
//! process above the Claude process, bring its window forward and select the
//! session's tab with UI Automation. Best effort; not unit-tested (needs a
//! desktop): the decisions are in `pano_core::host` and tested there.
use super::windows::{host_of, snapshot_procs};
use pano_core::host::{choose_tab, Host, TabChoice};
use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::System::Variant::{VARIANT, VT_I4};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationSelectionItemPattern,
    TreeScope_Descendants, UIA_ControlTypePropertyId, UIA_SelectionItemPatternId, UIA_TabItemControlTypeId,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId, IsIconic,
    IsWindowVisible, SetForegroundWindow, ShowWindow, SW_RESTORE,
};

use crate::errors::{
    TAB_AMBIGUOUS as DUPLICATE_NOTE, TAB_NOT_FOUND as NOT_FOUND, TAB_NO_MATCH as NO_MATCH_NOTE,
    TAB_SELECT_FAILED as SELECT_FAILED_NOTE,
};

const WT_WINDOW_CLASS: &str = "CASCADIA_HOSTING_WINDOW_CLASS";

pub fn focus_session_tab(pid: u32, name: &str) -> Result<Option<String>, String> {
    let wt = match host_of(pid, &snapshot_procs()) {
        Host::WindowsTerminal(wt) => wt,
        _ => return Err(NOT_FOUND.into()),
    };
    let windows = wt_windows(wt);
    if windows.is_empty() {
        return Err(NOT_FOUND.into());
    }
    let com = Com::init();
    let uia: Option<IUIAutomation> = unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok() };
    let tabs: Vec<Vec<IUIAutomationElement>> =
        windows.iter().map(|&w| uia.as_ref().map(|u| tab_items(u, w)).unwrap_or_default()).collect();
    let titles: Vec<Vec<String>> = tabs
        .iter()
        .map(|ts| ts.iter().map(|t| unsafe { t.CurrentName() }.map(|b| b.to_string()).unwrap_or_default()).collect())
        .collect();
    let result = match choose_tab(&titles, name) {
        TabChoice::Select(w, t) => {
            bring_forward(windows[w]);
            let selected = unsafe {
                tabs[w][t]
                    .GetCurrentPatternAs::<IUIAutomationSelectionItemPattern>(UIA_SelectionItemPatternId)
                    .and_then(|p| p.Select())
            };
            Ok(select_note(selected.is_ok()))
        }
        TabChoice::Duplicate(w) => {
            bring_forward(windows[w]);
            Ok(Some(DUPLICATE_NOTE.into()))
        }
        TabChoice::NoMatch => {
            bring_forward(windows[0]);
            Ok(Some(NO_MATCH_NOTE.into()))
        }
    };
    drop(tabs);
    drop(uia);
    drop(com);
    result
}

/// Note after selecting the single matching tab: none on success, else
/// `tab_select_failed` (the tab was found; unlike `tab_no_match`).
fn select_note(selected: bool) -> Option<String> {
    (!selected).then(|| SELECT_FAILED_NOTE.into())
}

/// COM for this (blocking-pool) thread; uninitialized on drop if we did it.
struct Com(bool);
impl Com {
    fn init() -> Self {
        Com(unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok())
    }
}
impl Drop for Com {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}

struct Search {
    pid: u32,
    wt: Vec<HWND>,
    other: Vec<HWND>,
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let s = &mut *(lparam.0 as *mut Search);
    let mut owner = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut owner));
    if owner == s.pid && IsWindowVisible(hwnd).as_bool() {
        let mut buf = [0u16; 64];
        let n = GetClassNameW(hwnd, &mut buf).max(0) as usize;
        if String::from_utf16_lossy(&buf[..n]) == WT_WINDOW_CLASS {
            s.wt.push(hwnd);
        } else {
            s.other.push(hwnd);
        }
    }
    BOOL(1)
}

/// Visible top-level windows of the WindowsTerminal process, frontmost
/// first (EnumWindows walks the z-order); its terminal windows if it has any.
fn wt_windows(pid: u32) -> Vec<HWND> {
    let mut s = Search { pid, wt: Vec::new(), other: Vec::new() };
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut s as *mut Search as isize));
    }
    if s.wt.is_empty() {
        s.other
    } else {
        s.wt
    }
}

/// TabItem elements of a window, in UI order. Empty on any UIA failure.
/// UIA filters by control type (one cross-process query for the tabs
/// instead of fetching every descendant and checking each).
fn tab_items(uia: &IUIAutomation, hwnd: HWND) -> Vec<IUIAutomationElement> {
    unsafe {
        let Ok(root) = uia.ElementFromHandle(hwnd) else { return Vec::new() };
        let Ok(is_tab) = uia.CreatePropertyCondition(UIA_ControlTypePropertyId, &i4_variant(UIA_TabItemControlTypeId.0))
        else {
            return Vec::new();
        };
        let Ok(found) = root.FindAll(TreeScope_Descendants, &is_tab) else { return Vec::new() };
        let n = found.Length().unwrap_or(0);
        (0..n).filter_map(|i| found.GetElement(i).ok()).collect()
    }
}

/// A VT_I4 VARIANT (plain value, nothing to free).
fn i4_variant(v: i32) -> VARIANT {
    let mut var = VARIANT::default();
    unsafe {
        let inner = &mut *var.Anonymous.Anonymous;
        inner.vt = VT_I4;
        inner.Anonymous.lVal = v;
    }
    var
}

/// Restores a minimized window and makes it the foreground window. The
/// caller's click makes this app foreground, which normally allows
/// SetForegroundWindow; otherwise input is attached to the foreground thread.
fn bring_forward(hwnd: HWND) {
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        if SetForegroundWindow(hwnd).as_bool() {
            return;
        }
        let fg = GetForegroundWindow();
        let fg_thread = GetWindowThreadProcessId(fg, None);
        let me = GetCurrentThreadId();
        let attached = fg_thread != 0 && fg_thread != me && AttachThreadInput(me, fg_thread, true).as_bool();
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);
        if attached {
            let _ = AttachThreadInput(me, fg_thread, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_select_of_the_single_match_has_its_own_note() {
        assert_eq!(select_note(true), None);
        assert_eq!(select_note(false).as_deref(), Some(crate::errors::TAB_SELECT_FAILED));
        assert_ne!(crate::errors::TAB_SELECT_FAILED, NO_MATCH_NOTE);
    }

    #[test]
    fn tab_condition_variant_is_the_tab_item_control_type() {
        let v = i4_variant(UIA_TabItemControlTypeId.0);
        unsafe {
            assert_eq!(v.Anonymous.Anonymous.vt, VT_I4);
            assert_eq!(v.Anonymous.Anonymous.Anonymous.lVal, 50019);
        }
    }
}
