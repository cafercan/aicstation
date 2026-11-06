//! Local desktop notifications. Windows: toasts through WinRT (`ToastNotificationManager`
//! in the `windows` crate). tauri-plugin-notification is not in the offline
//! crate cache, so this is the brief's fallback. Nothing leaves the machine.
//!
//! An unpackaged app needs an AppUserModelID to show toasts: the bundle
//! identifier is registered once under
//! `HKCU\Software\Classes\AppUserModelId\<id>` (display name + icon) and set
//! as the process's explicit AUMID. Linux: the freedesktop notification
//! service over D-Bus (zbus). Clicking a notification runs `on_click`.

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

/// One notification, independent of the platform that shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Toast {
    pub title: String,
    pub body: Option<String>,
    pub sound: bool,
}

impl Toast {
    pub fn new(title: &str, body: Option<&str>, sound: bool) -> Self {
        Toast { title: title.to_string(), body: body.map(str::to_string), sound }
    }

    /// Windows toast XML.
    #[cfg_attr(not(windows), allow(dead_code))]
    pub fn xml(&self) -> String {
        toast_xml(&self.title, self.body.as_deref(), self.sound)
    }
}

/// Text for a plain-text field (a notification summary): the same
/// characters as [`xml_text`] keeps, unescaped.
#[cfg_attr(windows, allow(dead_code))]
pub fn plain_text(s: &str) -> String {
    s.chars().filter(|&c| toast_char(c)).take(MAX_TEXT).collect()
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
        /// `tag` is the session the toast is about (its group, for
        /// [`Self::withdraw`]); None for app-wide notices.
        pub fn show(&self, toast: &super::Toast, tag: Option<&str>, on_click: impl Fn() + Send + 'static) {
            if let Err(e) = self.try_show(&toast.xml(), tag, on_click) {
                eprintln!("bildirim gösterilemedi: {e}");
            }
        }

        /// Removes the toasts about `sessions` from Action Center, or all of
        /// the app's toasts when `None`. Best effort.
        pub fn withdraw(&self, sessions: Option<&[String]>) {
            let r = ToastNotificationManager::History().and_then(|h| match sessions {
                None => h.ClearWithId(&self.aumid),
                Some(ids) => ids.iter().try_for_each(|id| h.RemoveGroupWithId(&HSTRING::from(id.as_str()), &self.aumid)),
            });
            if let Err(e) = r {
                eprintln!("bildirim kapatılamadı: {e}");
            }
        }

        fn try_show(&self, xml: &str, tag: Option<&str>, on_click: impl Fn() + Send + 'static) -> windows::core::Result<()> {
            let doc = XmlDocument::new()?;
            doc.LoadXml(&HSTRING::from(xml))?;
            let toast = ToastNotification::CreateToastNotification(&doc)?;
            if let Some(tag) = tag {
                toast.SetGroup(&HSTRING::from(tag))?;
            }
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

#[cfg(target_os = "linux")]
pub use linux::Toaster;

/// Linux: the freedesktop notification service, spoken to directly over one
/// D-Bus connection kept for the app's lifetime.
///
/// GNOME ties all of an app's notifications to the connection that sent the
/// first one and drops them when it goes away, so one connection serves every
/// notification. Notifications stay in the message tray (and in Ubuntu
/// Dock's counter) until closed, so the app closes them itself once the user
/// has looked: see [`Toaster::withdraw`].
#[cfg(target_os = "linux")]
mod linux {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, MutexGuard};
    use zbus::blocking::{Connection, MessageIterator};
    use zbus::zvariant::Value;
    use zbus::MatchRule;

    const SERVICE: &str = "org.freedesktop.Notifications";
    const PATH: &str = "/org/freedesktop/Notifications";

    /// The installed .desktop file (named after productName by the bundler):
    /// lets the server match notifications to the app when the window is hidden.
    const DESKTOP_ENTRY: &str = "AICStation";

    /// Notifications kept open at most; showing one more closes the oldest.
    const MAX_OPEN: usize = 20;

    /// A notification still on the server (banner or message tray).
    pub(super) struct Open {
        pub id: u32,
        /// Session it is about; `None` for app-wide notices.
        pub tag: Option<String>,
        pub on_click: Box<dyn Fn() + Send>,
    }

    type OpenList = Arc<Mutex<Vec<Open>>>;

    fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
        m.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Removes the entry for `id`.
    fn take(open: &OpenList, id: u32) -> Option<Open> {
        let mut open = lock(open);
        let i = open.iter().position(|o| o.id == id)?;
        Some(open.remove(i))
    }

    /// Removes the entries about `sessions` (all of them when `None`) and
    /// returns their ids.
    pub(super) fn split_off(open: &mut Vec<Open>, sessions: Option<&[String]>) -> Vec<u32> {
        let hit = |o: &Open| sessions.is_none_or(|s| o.tag.as_ref().is_some_and(|t| s.contains(t)));
        let ids = open.iter().filter(|o| hit(o)).map(|o| o.id).collect();
        open.retain(|o| !hit(o));
        ids
    }

    /// Removes the oldest entries beyond [`MAX_OPEN`] and returns their ids.
    pub(super) fn overflow(open: &mut Vec<Open>) -> Vec<u32> {
        let n = open.len().saturating_sub(MAX_OPEN);
        open.drain(..n).map(|o| o.id).collect()
    }

    fn close(conn: &Connection, ids: &[u32]) {
        for &id in ids {
            if let Err(e) = conn.call_method(Some(SERVICE), PATH, Some(SERVICE), "CloseNotification", &(id,)) {
                eprintln!("bildirim kapatılamadı: {e}");
            }
        }
    }

    /// Clicks run their notification's `on_click`; a notification closed
    /// by the user or the server is forgotten.
    fn listen(signals: MessageIterator, open: OpenList) {
        for msg in signals.flatten() {
            let header = msg.header();
            match header.member().map(|m| m.as_str()) {
                Some("ActionInvoked") => {
                    if let Ok((id, action)) = msg.body().deserialize::<(u32, String)>() {
                        if action == "default" {
                            if let Some(o) = take(&open, id) {
                                (o.on_click)();
                            }
                        }
                    }
                }
                Some("NotificationClosed") => {
                    if let Ok((id, _reason)) = msg.body().deserialize::<(u32, u32)>() {
                        take(&open, id);
                    }
                }
                _ => {}
            }
        }
    }

    pub struct Toaster {
        app_name: String,
        icon: String,
        /// Connected on first use; dropped after a failed call, so the next
        /// notification reconnects.
        conn: Mutex<Option<Connection>>,
        open: OpenList,
    }

    impl Toaster {
        pub fn new(_app_id: &str, display_name: &str, icon: Option<&std::path::Path>) -> Self {
            Self {
                app_name: display_name.to_string(),
                icon: icon.and_then(|p| p.to_str()).unwrap_or_default().to_string(),
                conn: Mutex::new(None),
                open: Arc::default(),
            }
        }

        /// The connection, with the signal listener subscribed before it is
        /// handed out (no click can arrive unheard).
        fn connection(&self) -> zbus::Result<Connection> {
            let mut slot = lock(&self.conn);
            if let Some(c) = slot.as_ref() {
                return Ok(c.clone());
            }
            let conn = Connection::session()?;
            let rule = MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .sender(SERVICE)?
                .path(PATH)?
                .interface(SERVICE)?
                .build();
            let signals = MessageIterator::for_match_rule(rule, &conn, None)?;
            let open = Arc::clone(&self.open);
            std::thread::spawn(move || listen(signals, open));
            *slot = Some(conn.clone());
            Ok(conn)
        }

        /// Shows a notification about session `tag` (None: app-wide);
        /// errors are logged, never raised. The body is escaped (servers may
        /// read it as markup), the summary is not.
        pub fn show(&self, toast: &super::Toast, tag: Option<&str>, on_click: impl Fn() + Send + 'static) {
            if let Err(e) = self.try_show(toast, tag, Box::new(on_click)) {
                *lock(&self.conn) = None;
                eprintln!("bildirim gösterilemedi: {e}");
            }
        }

        fn try_show(&self, toast: &super::Toast, tag: Option<&str>, on_click: Box<dyn Fn() + Send>) -> zbus::Result<()> {
            let conn = self.connection()?;
            let body = toast.body.as_deref().map(super::xml_text).unwrap_or_default();
            let mut hints: HashMap<&str, Value> = HashMap::new();
            hints.insert("desktop-entry", Value::from(DESKTOP_ENTRY));
            if toast.sound {
                hints.insert("sound-name", Value::from("message-new-instant"));
            } else {
                hints.insert("suppress-sound", Value::from(true));
            }
            let args = (
                self.app_name.as_str(),
                0u32, // replaces_id: a new notification
                self.icon.as_str(),
                super::plain_text(&toast.title),
                body,
                vec!["default", ""],
                hints,
                -1i32, // expire_timeout: the server's default
            );
            let id: u32 = conn.call_method(Some(SERVICE), PATH, Some(SERVICE), "Notify", &args)?.body().deserialize()?;
            let evicted = {
                let mut open = lock(&self.open);
                open.push(Open { id, tag: tag.map(str::to_string), on_click });
                overflow(&mut open)
            };
            close(&conn, &evicted);
            Ok(())
        }

        /// Closes the notifications about `sessions`, or all of them when
        /// `None`, on a thread of its own (callers may be on the UI thread).
        pub fn withdraw(&self, sessions: Option<&[String]>) {
            let ids = split_off(&mut lock(&self.open), sessions);
            if ids.is_empty() {
                return;
            }
            let Some(conn) = lock(&self.conn).clone() else { return };
            std::thread::spawn(move || close(&conn, &ids));
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        fn open(id: u32, tag: Option<&str>) -> Open {
            Open { id, tag: tag.map(str::to_string), on_click: Box::new(|| {}) }
        }

        fn ids(v: &[Open]) -> Vec<u32> {
            v.iter().map(|o| o.id).collect()
        }

        #[test]
        fn withdraw_picks_the_sessions_or_everything() {
            let mut v = vec![open(1, Some("a")), open(2, None), open(3, Some("b")), open(4, Some("a"))];
            assert_eq!(split_off(&mut v, Some(&["a".to_string()])), vec![1, 4]);
            assert_eq!(ids(&v), vec![2, 3]);
            assert_eq!(split_off(&mut v, Some(&[])), Vec::<u32>::new());
            assert_eq!(split_off(&mut v, None), vec![2, 3]);
            assert!(v.is_empty());
        }

        #[test]
        fn the_oldest_beyond_the_cap_are_closed() {
            let mut v: Vec<Open> = (1..=MAX_OPEN as u32).map(|i| open(i, None)).collect();
            assert!(overflow(&mut v).is_empty());
            v.push(open(100, None));
            assert_eq!(overflow(&mut v), vec![1]);
            assert_eq!(v.len(), MAX_OPEN);
            assert_eq!(v.last().map(|o| o.id), Some(100));
        }
    }
}

/// Stub for other platforms: toasts are not shown.
#[cfg(not(any(windows, target_os = "linux")))]
pub struct Toaster;

#[cfg(not(any(windows, target_os = "linux")))]
impl Toaster {
    pub fn new(_aumid: &str, _display_name: &str, _icon: Option<&std::path::Path>) -> Self {
        Toaster
    }
    pub fn show(&self, _toast: &Toast, _tag: Option<&str>, _on_click: impl Fn() + Send + 'static) {}
    pub fn withdraw(&self, _sessions: Option<&[String]>) {}
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
