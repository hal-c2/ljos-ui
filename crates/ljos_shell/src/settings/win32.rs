use std::cell::Cell;
use std::sync::{Arc, Mutex, PoisonError};

use ljos_a11y::{Contrast, HostPrefs};
use ljos_platform::Post;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_NOTIFY, KEY_READ, REG_NOTIFY_CHANGE_LAST_SET, RRF_RT_REG_DWORD,
    RegCloseKey, RegGetValueW, RegNotifyChangeKeyValue, RegOpenKeyExW,
};
use windows_sys::Win32::System::Threading::{CreateEventW, INFINITE};
use windows_sys::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, MSG,
    MsgWaitForMultipleObjects, PM_REMOVE, PeekMessageW, PostMessageW, QS_ALLINPUT, RegisterClassW,
    SPI_GETCLIENTAREAANIMATION, SPI_GETHIGHCONTRAST, SystemParametersInfoW, WM_APP,
    WM_SETTINGCHANGE, WNDCLASSW,
};

/// What ends the thread, posted to its window.
const STOP: u32 = WM_APP + 1;

/// The thread with the hidden window. Dropped, it tells the window to
/// stop, which ends the thread.
pub(crate) struct Watch {
    held: Arc<Mutex<Held>>,
}

#[derive(Default)]
struct Held {
    window: Option<HWND>,
    closed: bool,
}

thread_local! {
    /// Set by the window when the system says a setting changed.
    static CHANGED: Cell<bool> = const { Cell::new(false) };
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

pub(crate) fn watch(post: Post) -> Watch {
    let held = Arc::new(Mutex::new(Held::default()));
    let shared = Arc::clone(&held);
    let _ = std::thread::Builder::new()
        .name("ljos-settings".to_owned())
        .spawn(move || hear(&post, &shared));
    Watch { held }
}

impl Drop for Watch {
    fn drop(&mut self) {
        let mut held = self.held.lock().unwrap_or_else(PoisonError::into_inner);
        held.closed = true;
        if let Some(window) = held.window {
            // SAFETY: a message to a window of this process; a window
            // already gone takes none.
            unsafe { PostMessageW(window, STOP, 0, 0) };
        }
    }
}

unsafe extern "system" fn on_message(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_SETTINGCHANGE {
        CHANGED.with(|c| c.set(true));
    }
    // SAFETY: what the window does not handle, the system does.
    unsafe { DefWindowProcW(window, message, wparam, lparam) }
}

fn read(accessibility: HKEY) -> HostPrefs {
    let mut animate: i32 = 1;
    let mut contrast = HIGHCONTRASTW {
        cbSize: std::mem::size_of::<HIGHCONTRASTW>() as u32,
        dwFlags: 0,
        lpszDefaultScheme: std::ptr::null_mut(),
    };
    let mut percent: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    let value = wide("TextScaleFactor");
    // SAFETY: each call writes only the value it is given, of the size
    // it says.
    let (animated, contrasted, scaled) = unsafe {
        (
            SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, (&raw mut animate).cast(), 0) != 0,
            SystemParametersInfoW(
                SPI_GETHIGHCONTRAST,
                contrast.cbSize,
                (&raw mut contrast).cast(),
                0,
            ) != 0,
            accessibility != 0
                && RegGetValueW(
                    accessibility,
                    std::ptr::null(),
                    value.as_ptr(),
                    RRF_RT_REG_DWORD,
                    std::ptr::null_mut(),
                    (&raw mut percent).cast(),
                    &mut size,
                ) == 0,
        )
    };
    HostPrefs {
        reduced_motion: animated.then_some(animate == 0),
        contrast: contrasted.then_some(if contrast.dwFlags & HCF_HIGHCONTRASTON != 0 {
            Contrast::High
        } else {
            Contrast::NoPreference
        }),
        // Without the value the text is at 100 %.
        font_scale: Some(if scaled && percent > 0 {
            f64::from(percent) / 100.0
        } else {
            1.0
        }),
        ..HostPrefs::default()
    }
}

fn hear(post: &Post, held: &Mutex<Held>) {
    let class = wide("LjosSettings");
    // SAFETY: a window of a class of this thread's, which is never
    // shown; its procedure only notes a message. Handles are closed
    // before the thread ends.
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        let wc = WNDCLASSW {
            lpfnWndProc: Some(on_message),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassW(&wc);
        // A top-level window: a message-only one hears no broadcasts.
        let window = CreateWindowExW(
            0,
            class.as_ptr(),
            class.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            instance,
            std::ptr::null(),
        );
        if window == 0 {
            return;
        }
        {
            let mut held = held.lock().unwrap_or_else(PoisonError::into_inner);
            if held.closed {
                DestroyWindow(window);
                return;
            }
            held.window = Some(window);
        }
        let mut accessibility: HKEY = 0;
        let key = wide("Software\\Microsoft\\Accessibility");
        if RegOpenKeyExW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            0,
            KEY_READ | KEY_NOTIFY,
            &mut accessibility,
        ) != 0
        {
            accessibility = 0;
        }
        let event: HANDLE = CreateEventW(std::ptr::null(), 0, 0, std::ptr::null());
        let arm = || {
            if accessibility != 0 && event != 0 {
                RegNotifyChangeKeyValue(accessibility, 0, REG_NOTIFY_CHANGE_LAST_SET, event, 1);
            }
        };
        arm();
        let mut told = read(accessibility);
        super::report(post, told);
        let waits: u32 = u32::from(event != 0);
        'heard: loop {
            // Sleeps until a message or the key's change.
            let woke = MsgWaitForMultipleObjects(waits, &event, 0, INFINITE, QS_ALLINPUT);
            if waits == 1 && woke == 0 {
                CHANGED.with(|c| c.set(true));
                arm();
            }
            let mut msg: MSG = std::mem::zeroed();
            while PeekMessageW(&mut msg, 0, 0, 0, PM_REMOVE) != 0 {
                if msg.message == STOP {
                    break 'heard;
                }
                DispatchMessageW(&msg);
            }
            if CHANGED.with(|c| c.replace(false)) {
                let now = read(accessibility);
                if now != told {
                    told = now;
                    super::report(post, now);
                }
            }
        }
        if event != 0 {
            CloseHandle(event);
        }
        if accessibility != 0 {
            RegCloseKey(accessibility);
        }
        DestroyWindow(window);
    }
}
