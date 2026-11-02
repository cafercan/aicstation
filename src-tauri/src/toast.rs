//! Local Windows toast notifications through WinRT (`ToastNotificationManager`
//! in the `windows` crate). tauri-plugin-notification is not in the offline
//! crate cache, so this is the brief's fallback. Nothing leaves the machine.
//!
//! An unpackaged app needs an AppUserModelID to show toasts: the bundle
//! identifier is registered once under
//! `HKCU\Software\Classes\AppUserModelId\<id>` (display name + icon) and set
//! as the process's explicit AUMID. Clicking a toast runs `on_click`.

/// Longest title / body put in a toast, in characters.
const MAX_TEXT: usize = 200;

/// A char allowed in the toast text: not a control character (tab and
/// newline excepted), not a format character (bidi overrides etc.), and
/// valid XML 1.0 content (U+FFFE/U+FFFF are not; noncharacters such as
/// U+FDD0..U+FDEF and U+xFFFE/U+xFFFF are dropped as well).
fn toast_char(c: char) -> bool {
    let u = c as u32;
    let control = c.is_control() && !matches!(c, '\t' | '\n');
    let nonchar = (0xFDD0..=0xFDEF).contains(&u) || (u & 0xFFFE) == 0xFFFE;
    !control && !nonchar && !crate::native_text::is_format_char(c)
}

/// Text safe inside an XML element or attribute: escaped, control, format
/// and XML-invalid characters dropped, capped at MAX_TEXT characters.
pub fn xml_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars().filter(|&c| toast_char(c)).take(MAX_TEXT) {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Toast XML: a title, an optional body, and either the system default
/// notification sound or silence (no bundled audio).
pub fn toast_xml(title: &str, body: Option<&str>, sound: bool) -> String {
    let body = body.map(|b| format!("<text>{}</text>", xml_text(b))).unwrap_or_default();
    let audio = if sound {
        r#"<audio src="ms-winsoundevent:Notification.Default"/>"#
    } else {
        r#"<audio silent="true"/>"#
    };
    format!(
        r#"<toast><visual><binding template="ToastGeneric"><text>{}</text>{body}</binding></visual>{audio}</toast>"#,
        xml_text(title)
    )
}

#[cfg(windows)]
pub use win::Toaster;

#[cfg(windows)]
mod win {
    use std::collections::VecDeque;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::sync::Mutex;
    use windows::core::{IInspectable, Ref, HSTRING};
    use windows::Data::Xml::Dom::XmlDocument;
    use windows::Foundation::TypedEventHandler;
    use windows::UI::Notifications::{ToastNotification, ToastNotificationManager};
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE,
        REG_SZ,
    };

    /// Toasts kept alive so their click handler still runs from the Action Center.
    const KEEP: usize = 20;

    fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }

    /// Registers the AUMID for the current user (display name and icon of
    /// the toasts) and sets it on this process. Best effort.
    fn register(aumid: &str, display_name: &str, icon: Option<&Path>) {
        let key_path = wide(format!(r"Software\Classes\AppUserModelId\{aumid}").as_ref());
        let mut key: HKEY = std::ptr::null_mut();
        let rc = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                key_path.as_ptr(),
                0,
                std::ptr::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                std::ptr::null(),
                &mut key,
                std::ptr::null_mut(),
            )
        };
        if rc == ERROR_SUCCESS {
            let set = |name: &str, value: &std::ffi::OsStr| {
                let n = wide(name.as_ref());
                let v = wide(value);
                unsafe {
                    RegSetValueExW(key, n.as_ptr(), 0, REG_SZ, v.as_ptr().cast(), (v.len() * 2) as u32);
                }
            };
            set("DisplayName", display_name.as_ref());
            if let Some(icon) = icon {
                set("IconUri", icon.as_os_str());
            }
            unsafe { RegCloseKey(key) };
        }
        let _ = unsafe { windows::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(&HSTRING::from(aumid)) };
    }

    pub struct Toaster {
        aumid: HSTRING,
        shown: Mutex<VecDeque<ToastNotification>>,
    }

    impl Toaster {
        pub fn new(aumid: &str, display_name: &str, icon: Option<&Path>) -> Self {
            register(aumid, display_name, icon);
            Self { aumid: HSTRING::from(aumid), shown: Mutex::new(VecDeque::new()) }
        }

        /// Shows a toast; errors are logged, never raised (a notification
        /// must not disturb the snapshot thread).
        pub fn show(&self, xml: &str, on_click: impl Fn() + Send + 'static) {
            if let Err(e) = self.try_show(xml, on_click) {
                eprintln!("bildirim gösterilemedi: {e}");
            }
        }

        fn try_show(&self, xml: &str, on_click: impl Fn() + Send + 'static) -> windows::core::Result<()> {
            let doc = XmlDocument::new()?;
            doc.LoadXml(&HSTRING::from(xml))?;
            let toast = ToastNotification::CreateToastNotification(&doc)?;
            toast.Activated(&TypedEventHandler::new(
                move |_: Ref<ToastNotification>, _: Ref<IInspectable>| {
                    on_click();
                    Ok(())
                },
            ))?;
            ToastNotificationManager::CreateToastNotifierWithId(&self.aumid)?.Show(&toast)?;
            let mut shown = self.shown.lock().unwrap_or_else(|e| e.into_inner());
            shown.push_back(toast);
            while shown.len() > KEEP {
                shown.pop_front();
            }
            Ok(())
        }
    }
}

/// Non-Windows stub: toasts are not shown.
#[cfg(not(windows))]
pub struct Toaster;

#[cfg(not(windows))]
impl Toaster {
    pub fn new(_aumid: &str, _display_name: &str, _icon: Option<&std::path::Path>) -> Self {
        Toaster
    }
    pub fn show(&self, _xml: &str, _on_click: impl Fn() + Send + 'static) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untrusted_text_is_escaped_and_cleaned() {
        assert_eq!(xml_text(r#"<a href="x">&'</a>"#), "&lt;a href=&quot;x&quot;&gt;&amp;&apos;&lt;/a&gt;");
        assert_eq!(xml_text("a\u{0}b\u{1b}[31mc\u{7f}"), "ab[31mc");
        assert_eq!(xml_text(&"é".repeat(500)).chars().count(), MAX_TEXT);
    }

    #[test]
    fn format_and_xml_invalid_characters_are_dropped() {
        assert_eq!(xml_text("\u{202E}gpj.exe\u{202C}"), "gpj.exe");
        assert_eq!(xml_text("a\u{2067}b\u{200D}c\u{FEFF}d\u{E007F}"), "abcd");
        assert_eq!(xml_text("a\u{FFFE}b\u{FFFF}c\u{FDD0}d\u{1FFFF}e\u{10FFFE}"), "abcde");
        // Tab and newline stay; the rest of the BMP and astral text stays.
        assert_eq!(xml_text("a\tb\nc \u{FFFD} 中 😀"), "a\tb\nc \u{FFFD} 中 😀");
    }

    #[test]
    fn toast_xml_shape_and_sound() {
        let silent = toast_xml("api onay bekliyor", Some("Bash: rm <x>"), false);
        assert_eq!(
            silent,
            r#"<toast><visual><binding template="ToastGeneric"><text>api onay bekliyor</text><text>Bash: rm &lt;x&gt;</text></binding></visual><audio silent="true"/></toast>"#
        );
        let loud = toast_xml("t", None, true);
        assert!(loud.contains(r#"<audio src="ms-winsoundevent:Notification.Default"/>"#));
        assert!(!loud.contains("<text></text>") && loud.matches("<text>").count() == 1);
        // Only the system default sound: no file or URL source is ever used.
        assert!(!loud.contains("file:") && !loud.contains("http"));
    }
}
