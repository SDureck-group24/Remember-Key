//! Sperrt den Tresor, sobald Windows gesperrt wird (Win+L, Bildschirmschoner mit
//! Kennwort, Benutzerwechsel) oder in den Energiesparmodus/Ruhezustand geht.
//!
//! Dafür läuft ein unsichtbares Fenster in einem eigenen Thread, das sich für
//! Sitzungsereignisse registriert und Energie-Broadcasts empfängt.

#[cfg(windows)]
mod imp {
    use std::ptr;
    use std::sync::OnceLock;

    use tauri::AppHandle;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::System::RemoteDesktop::{WTSRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW, TranslateMessage, MSG,
        WNDCLASSW,
    };

    const WM_WTSSESSION_CHANGE: u32 = 0x02B1;
    const WTS_SESSION_LOCK: WPARAM = 0x7;
    const WM_POWERBROADCAST: u32 = 0x0218;
    const PBT_APMSUSPEND: WPARAM = 0x4;

    static APP: OnceLock<AppHandle> = OnceLock::new();

    unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let lock = (msg == WM_WTSSESSION_CHANGE && wparam == WTS_SESSION_LOCK)
            || (msg == WM_POWERBROADCAST && wparam == PBT_APMSUSPEND);
        if lock {
            if let Some(app) = APP.get() {
                crate::lock_from_system(app);
            }
        }
        DefWindowProcW(hwnd, msg, wparam, lparam)
    }

    pub fn spawn(app: AppHandle) {
        if APP.set(app).is_err() {
            return;
        }
        std::thread::spawn(|| unsafe {
            let class: Vec<u16> = "RememberKeySessionWatcher\0".encode_utf16().collect();
            let instance = GetModuleHandleW(ptr::null());
            let mut wc: WNDCLASSW = std::mem::zeroed();
            wc.lpfnWndProc = Some(wndproc);
            wc.hInstance = instance;
            wc.lpszClassName = class.as_ptr();
            if RegisterClassW(&wc) == 0 {
                return;
            }
            // Unsichtbares Top-Level-Fenster (kein Message-only-Fenster, damit
            // WM_POWERBROADCAST ankommt).
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                class.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                ptr::null_mut(),
                ptr::null_mut(),
                instance,
                ptr::null(),
            );
            if hwnd.is_null() {
                return;
            }
            WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        });
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn spawn(_app: tauri::AppHandle) {}
}

pub use imp::spawn;
