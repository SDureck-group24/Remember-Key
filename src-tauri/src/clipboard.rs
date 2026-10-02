//! Zwischenablage über die Win32-API.
//!
//! Geheimnisse werden mit den Formaten markiert, die Windows-Zwischenablage-
//! verlauf (Win+V), Cloud-Sync und Clipboard-Manager ausschließen. Zum Leeren
//! wird die Sequenznummer verglichen, damit nur unser eigener Inhalt gelöscht
//! wird und wir das Geheimnis nicht im Speicher halten müssen.

use crate::error::{Error, Result};

#[cfg(windows)]
mod imp {
    use std::ptr;
    use std::thread::sleep;
    use std::time::Duration;

    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardSequenceNumber, OpenClipboard,
        RegisterClipboardFormatW, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows_sys::Win32::System::Ole::CF_UNICODETEXT;
    use zeroize::Zeroizing;

    use super::{Error, Result};

    struct Open;

    impl Open {
        fn new() -> Result<Self> {
            for _ in 0..20 {
                if unsafe { OpenClipboard(ptr::null_mut()) } != 0 {
                    return Ok(Open);
                }
                sleep(Duration::from_millis(25));
            }
            Err(Error::Clipboard("konnte nicht geöffnet werden".into()))
        }
    }

    impl Drop for Open {
        fn drop(&mut self) {
            unsafe { CloseClipboard() };
        }
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Kopiert `bytes` in einen globalen Speicherblock und übergibt ihn der Zwischenablage.
    unsafe fn set_data(format: u32, bytes: &[u8]) -> Result<()> {
        let h = GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1));
        if h.is_null() {
            return Err(Error::Clipboard("Speicher konnte nicht reserviert werden".into()));
        }
        let p = GlobalLock(h) as *mut u8;
        if p.is_null() {
            GlobalFree(h);
            return Err(Error::Clipboard("Speicher konnte nicht gesperrt werden".into()));
        }
        ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
        GlobalUnlock(h);
        if SetClipboardData(format, h).is_null() {
            GlobalFree(h);
            return Err(Error::Clipboard("Daten konnten nicht gesetzt werden".into()));
        }
        Ok(())
    }

    pub fn set_secret(text: &str) -> Result<u32> {
        let utf16: Zeroizing<Vec<u16>> = Zeroizing::new(wide(text));
        {
            let _open = Open::new()?;
            unsafe {
                EmptyClipboard();
                let bytes = std::slice::from_raw_parts(utf16.as_ptr() as *const u8, utf16.len() * 2);
                set_data(CF_UNICODETEXT as u32, bytes)?;
                for name in [
                    "ExcludeClipboardContentFromMonitorProcessing",
                    "CanIncludeInClipboardHistory",
                    "CanUploadToCloudClipboard",
                ] {
                    let fmt = RegisterClipboardFormatW(wide(name).as_ptr());
                    if fmt != 0 {
                        // DWORD 0 = "nicht zulassen"; für das Exclude-Format ist der Inhalt egal.
                        let _ = set_data(fmt, &0u32.to_le_bytes());
                    }
                }
            }
        }
        Ok(unsafe { GetClipboardSequenceNumber() })
    }

    pub fn clear_if_unchanged(seq: u32) {
        if unsafe { GetClipboardSequenceNumber() } != seq {
            return;
        }
        if let Ok(_open) = Open::new() {
            unsafe { EmptyClipboard() };
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{Error, Result};

    pub fn set_secret(_text: &str) -> Result<u32> {
        Err(Error::Clipboard("nur unter Windows implementiert".into()))
    }

    pub fn clear_if_unchanged(_seq: u32) {}
}

pub use imp::{clear_if_unchanged, set_secret};
