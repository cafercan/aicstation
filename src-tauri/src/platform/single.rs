//! Single instance without tauri-plugin-single-instance (not in the offline
//! crate cache): a named mutex marks the first instance; a second launch
//! sets a named auto-reset event and exits, and the first instance's waiter
//! thread brings its window forward. Names live in the per-session `Local\`
//! namespace, so other logged-on users are unaffected.
use std::os::windows::ffi::OsStrExt;
use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, WAIT_OBJECT_0};
use windows_sys::Win32::System::Threading::{CreateEventW, CreateMutexW, SetEvent, WaitForSingleObject, INFINITE};

fn wide(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

/// Handle shared with the waiter thread (kernel handles are process-wide).
struct Event(HANDLE);
unsafe impl Send for Event {}

pub enum Instance {
    /// This is the first instance; `wait` runs the callback on every later launch.
    First(Activation),
    /// Another instance runs and was asked to come forward.
    Second,
}

pub struct Activation(Event);

impl Activation {
    /// Spawns the thread that calls `on_activate` each time another launch
    /// signals.
    pub fn listen(self, on_activate: impl Fn() + Send + 'static) {
        let ev = self.0;
        std::thread::spawn(move || {
            let ev = ev;
            loop {
                if unsafe { WaitForSingleObject(ev.0, INFINITE) } != WAIT_OBJECT_0 {
                    return;
                }
                on_activate();
            }
        });
    }
}

/// `key` must be stable per app (the bundle identifier).
pub fn acquire(key: &str) -> Instance {
    // The event is created first so a second instance always finds it while
    // the first one lives.
    let event_name = wide(&format!(r"Local\{key}.activate"));
    let mutex_name = wide(&format!(r"Local\{key}.instance"));
    let ev = unsafe { CreateEventW(std::ptr::null(), 0, 0, event_name.as_ptr()) };
    let mutex = unsafe { CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr()) };
    let exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    if mutex.is_null() || ev.is_null() {
        // Cannot tell: run normally rather than refuse to start.
        return Instance::First(Activation(Event(ev)));
    }
    if exists {
        // Let the first instance take the foreground for its window.
        let _ = unsafe {
            windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(
                windows::Win32::UI::WindowsAndMessaging::ASFW_ANY,
            )
        };
        unsafe {
            SetEvent(ev);
            CloseHandle(ev);
            CloseHandle(mutex);
        }
        return Instance::Second;
    }
    // The mutex handle is kept for the life of the process.
    Instance::First(Activation(Event(ev)))
}
