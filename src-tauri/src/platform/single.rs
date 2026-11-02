//! Single instance without tauri-plugin-single-instance (not in the offline
//! crate cache): a named mutex marks the first instance; a second launch
//! sets a named auto-reset event, and the first instance's waiter thread
//! brings its window forward and answers on a second event. When no answer
//! comes within [`ACK_TIMEOUT`] (the first instance hangs or is still
//! starting), the second launch tells the user instead of exiting silently.
//! Names live in the per-session `Local\` namespace, so other logged-on
//! users are unaffected.
use std::os::windows::ffi::OsStrExt;
use std::time::Duration;
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::{
    CreateEventW, CreateMutexW, ResetEvent, SetEvent, WaitForSingleObject, INFINITE,
};

/// How long a second launch waits for the running instance to answer.
pub const ACK_TIMEOUT: Duration = Duration::from_secs(3);

fn wide(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

/// Handle shared with the waiter thread (kernel handles are process-wide).
struct Event(HANDLE);
unsafe impl Send for Event {}

pub enum Instance {
    /// This is the first instance; `listen` runs the callback on every later launch.
    First(Activation),
    /// Another instance runs and was asked to come forward. `responded` is
    /// false when it did not confirm within [`ACK_TIMEOUT`].
    Second { responded: bool },
}

pub struct Activation {
    ev: Event,
    ack: Event,
}

/// Confirms an activation to the waiting second launch. Send it to the
/// thread that actually brought the window forward (the event loop), so a
/// hung UI does not confirm.
pub struct Ack(HANDLE);
unsafe impl Send for Ack {}

impl Ack {
    pub fn done(self) {
        if !self.0.is_null() {
            unsafe { SetEvent(self.0) };
        }
    }
}

impl Activation {
    /// Spawns the thread that calls `on_activate` each time another launch
    /// signals; `on_activate` must call [`Ack::done`] once the window is shown.
    pub fn listen(self, on_activate: impl Fn(Ack) + Send + 'static) {
        let (ev, ack) = (self.ev, self.ack);
        std::thread::spawn(move || {
            let (ev, ack) = (ev, ack);
            loop {
                if unsafe { WaitForSingleObject(ev.0, INFINITE) } != WAIT_OBJECT_0 {
                    return;
                }
                on_activate(Ack(ack.0));
            }
        });
    }
}

/// `key` must be stable per app (the bundle identifier).
pub fn acquire(key: &str) -> Instance {
    // The events are created first so a second instance always finds them
    // while the first one lives.
    let event_name = wide(&format!(r"Local\{key}.activate"));
    let ack_name = wide(&format!(r"Local\{key}.activated"));
    let mutex_name = wide(&format!(r"Local\{key}.instance"));
    let ev = unsafe { CreateEventW(std::ptr::null(), 0, 0, event_name.as_ptr()) };
    let ack = unsafe { CreateEventW(std::ptr::null(), 0, 0, ack_name.as_ptr()) };
    let mutex = unsafe { CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr()) };
    let exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    if mutex.is_null() || ev.is_null() {
        // Cannot tell: run normally rather than refuse to start.
        return Instance::First(Activation { ev: Event(ev), ack: Event(ack) });
    }
    if exists {
        // Let the first instance take the foreground for its window.
        let _ = unsafe {
            windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(
                windows::Win32::UI::WindowsAndMessaging::ASFW_ANY,
            )
        };
        let responded = unsafe {
            if ack.is_null() {
                // No way to hear back: assume it worked, as before.
                SetEvent(ev);
                true
            } else {
                // A late answer to an earlier launch must not count for this one.
                ResetEvent(ack);
                SetEvent(ev);
                WaitForSingleObject(ack, ACK_TIMEOUT.as_millis() as u32) == WAIT_OBJECT_0
            }
        };
        unsafe {
            CloseHandle(ev);
            if !ack.is_null() {
                CloseHandle(ack);
            }
            CloseHandle(mutex);
        }
        return Instance::Second { responded };
    }
    // The mutex handle is kept for the life of the process.
    Instance::First(Activation { ev: Event(ev), ack: Event(ack) })
}

/// Native message box (no window of our own exists yet) with `text`.
pub fn message_box(title: &str, text: &str) {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND};
    let (title, text) = (wide(title), wide(text));
    unsafe {
        MessageBoxW(None, PCWSTR(text.as_ptr()), PCWSTR(title.as_ptr()), MB_OK | MB_ICONINFORMATION | MB_SETFOREGROUND);
    }
}
