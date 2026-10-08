//! Windows Hello (PIN, Fingerabdruck, Gesicht) als zusätzliche Bestätigung.
//!
//! Wird für Freigaben von KI-Anfragen genutzt, wenn `Settings::agent_hello` gesetzt ist:
//! Ein Klick im Fenster allein reicht dann nicht – auch ein anderes Programm, das die
//! Oberfläche fernsteuert, kommt an Windows Hello nicht vorbei.

#[cfg(windows)]
mod imp {
    use windows::core::{factory, HSTRING};
    use windows::Security::Credentials::UI::{
        UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability,
    };
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::WinRT::IUserConsentVerifierInterop;
    use windows_future::IAsyncOperation;

    /// Ist Windows Hello eingerichtet und nutzbar?
    pub fn available() -> bool {
        UserConsentVerifier::CheckAvailabilityAsync()
            .and_then(|op| op.join())
            .is_ok_and(|a| a == UserConsentVerifierAvailability::Available)
    }

    /// Zeigt die Windows-Hello-Abfrage über dem Fenster `hwnd` an und wartet blockierend.
    pub fn verify(hwnd: isize, message: &str) -> Result<(), String> {
        let result = (|| -> windows::core::Result<UserConsentVerificationResult> {
            let interop = factory::<UserConsentVerifier, IUserConsentVerifierInterop>()?;
            let op: IAsyncOperation<UserConsentVerificationResult> =
                unsafe { interop.RequestVerificationForWindowAsync(HWND(hwnd as *mut _), &HSTRING::from(message))? };
            op.join()
        })()
        .map_err(|e| format!("Windows Hello: {}", e.message()))?;
        match result {
            UserConsentVerificationResult::Verified => Ok(()),
            UserConsentVerificationResult::Canceled => Err("Windows Hello wurde abgebrochen".into()),
            UserConsentVerificationResult::DeviceNotPresent | UserConsentVerificationResult::NotConfiguredForUser => {
                Err("Windows Hello ist auf diesem Gerät nicht eingerichtet".into())
            }
            UserConsentVerificationResult::DisabledByPolicy => Err("Windows Hello ist per Richtlinie deaktiviert".into()),
            UserConsentVerificationResult::RetriesExhausted => Err("Zu viele Fehlversuche bei Windows Hello".into()),
            _ => Err("Windows Hello hat nicht bestätigt".into()),
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn available() -> bool {
        false
    }

    pub fn verify(_hwnd: isize, _message: &str) -> Result<(), String> {
        Err("Windows Hello ist nur unter Windows verfügbar".into())
    }
}

pub use imp::{available, verify};

#[cfg(test)]
mod tests {
    /// Nur Abfrage der Verfügbarkeit, ohne Dialog: `cargo test -- --ignored hello_availability --nocapture`
    #[test]
    #[ignore]
    fn hello_availability() {
        println!("Windows Hello verfügbar: {}", super::available());
    }
}
