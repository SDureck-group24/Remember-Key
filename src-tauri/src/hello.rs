//! Windows Hello (PIN, Fingerabdruck, Gesicht).
//!
//! Zwei Verwendungen:
//! - **Bestätigung** (`verify`): Freigaben von KI-Anfragen, wenn `Settings::agent_hello` gesetzt
//!   ist. Ein Klick im Fenster allein reicht dann nicht – auch ein anderes Programm, das die
//!   Oberfläche fernsteuert, kommt an Windows Hello nicht vorbei.
//! - **Entsperren** (`create_key`, `encrypt`, `decrypt`): Ein RSA-Schlüssel im Windows-Hello-
//!   Schlüsselspeicher (Microsoft Passport KSP). Verschlüsseln geht mit dem öffentlichen Teil
//!   ohne Abfrage; Entschlüsseln verlangt bei jedem Aufruf PIN, Fingerabdruck oder Gesicht.
//!   Der private Schlüssel verlässt den Speicher (TPM, falls vorhanden) nie.

#[cfg(windows)]
mod imp {
    use windows::core::{factory, Owned, HSTRING, PCWSTR, PWSTR};
    use windows::Security::Credentials::UI::{
        UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability,
    };
    use windows::Win32::Foundation::{LocalFree, HANDLE, HLOCAL, HWND};
    use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows::Win32::Security::Cryptography::*;
    use windows::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    use windows::Win32::System::WinRT::IUserConsentVerifierInterop;
    use windows_future::IAsyncOperation;
    use zeroize::Zeroizing;

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

    // ---------- Schlüssel zum Entsperren ----------

    const KEY_PATH: &str = "//RememberKey/Entsperren/Tresorschluessel";
    /// Jede Entschlüsselung verlangt eine neue Bestätigung (kein PIN-Cache).
    const NGC_CACHE_TYPE: PCWSTR = windows::core::w!("NgcCacheType");
    const NGC_CACHE_TYPE_DEPRECATED: PCWSTR = windows::core::w!("NgcCacheTypeProperty");
    const NGC_CACHE_AUTH_MANDATORY: u32 = 1;
    const NTE_NO_KEY: i32 = 0x8009000Du32 as i32;
    const NTE_BAD_KEYSET: i32 = 0x80090016u32 as i32;
    const NTE_USER_CANCELLED: i32 = 0x80090036u32 as i32;
    const ERROR_CANCELLED: i32 = 0x800704C7u32 as i32;

    struct Provider(NCRYPT_PROV_HANDLE);
    impl Drop for Provider {
        fn drop(&mut self) {
            let _ = unsafe { NCryptFreeObject(NCRYPT_HANDLE(self.0 .0)) };
        }
    }

    struct Key(NCRYPT_KEY_HANDLE);
    impl Key {
        fn handle(&self) -> NCRYPT_HANDLE {
            NCRYPT_HANDLE(self.0 .0)
        }
    }
    impl Drop for Key {
        fn drop(&mut self) {
            if self.0 .0 != 0 {
                let _ = unsafe { NCryptFreeObject(self.handle()) };
            }
        }
    }

    fn err(e: windows::core::Error) -> String {
        match e.code().0 {
            NTE_USER_CANCELLED | ERROR_CANCELLED => "Windows Hello wurde abgebrochen".into(),
            NTE_NO_KEY | NTE_BAD_KEYSET => "Windows-Hello-Entsperren ist auf diesem Gerät nicht eingerichtet".into(),
            _ => format!("Windows Hello: {}", e.message()),
        }
    }

    /// SID des angemeldeten Benutzers – Teil des Schlüsselnamens im Passport-KSP.
    fn user_sid() -> windows::core::Result<String> {
        unsafe {
            let mut token = Owned::new(HANDLE::default());
            OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut *token)?;
            let mut len = 0u32;
            let _ = GetTokenInformation(*token, TokenUser, None, 0, &mut len);
            // u64-Puffer für die Ausrichtung von TOKEN_USER.
            let mut buf = vec![0u64; (len as usize).div_ceil(8)];
            GetTokenInformation(*token, TokenUser, Some(buf.as_mut_ptr().cast()), len, &mut len)?;
            let user = &*(buf.as_ptr() as *const TOKEN_USER);
            let mut s = PWSTR::null();
            ConvertSidToStringSidW(user.User.Sid, &mut s)?;
            let sid = s.to_string();
            LocalFree(Some(HLOCAL(s.0.cast())));
            Ok(sid?)
        }
    }

    fn key_name() -> windows::core::Result<HSTRING> {
        Ok(HSTRING::from(format!("{}{KEY_PATH}", user_sid()?)))
    }

    fn provider() -> windows::core::Result<Provider> {
        let mut p = NCRYPT_PROV_HANDLE::default();
        unsafe { NCryptOpenStorageProvider(&mut p, MS_NGC_KEY_STORAGE_PROVIDER, 0)? };
        Ok(Provider(p))
    }

    fn open_key(p: &Provider) -> windows::core::Result<Key> {
        let mut k = Key(NCRYPT_KEY_HANDLE::default());
        unsafe { NCryptOpenKey(p.0, &mut k.0, &key_name()?, CERT_KEY_SPEC(0), NCRYPT_FLAGS(0))? };
        Ok(k)
    }

    /// Fenster und Text für die Windows-Hello-Abfrage.
    fn set_ui(k: &Key, hwnd: isize, message: &str) -> windows::core::Result<()> {
        unsafe {
            NCryptSetProperty(k.handle(), NCRYPT_WINDOW_HANDLE_PROPERTY, &hwnd.to_ne_bytes(), NCRYPT_FLAGS(0))?;
            let text: Vec<u8> = message.encode_utf16().chain([0]).flat_map(u16::to_ne_bytes).collect();
            NCryptSetProperty(k.handle(), NCRYPT_USE_CONTEXT_PROPERTY, &text, NCRYPT_FLAGS(0))
        }
    }

    pub fn key_exists() -> bool {
        provider().and_then(|p| open_key(&p)).is_ok()
    }

    /// Legt den Schlüssel an (Windows Hello fragt dabei nach). Ein vorhandener wird ersetzt.
    pub fn create_key(hwnd: isize, message: &str) -> Result<(), String> {
        (|| -> windows::core::Result<()> {
            let p = provider()?;
            let mut k = Key(NCRYPT_KEY_HANDLE::default());
            unsafe {
                NCryptCreatePersistedKey(
                    p.0,
                    &mut k.0,
                    BCRYPT_RSA_ALGORITHM,
                    &key_name()?,
                    CERT_KEY_SPEC(0),
                    NCRYPT_OVERWRITE_KEY_FLAG,
                )?;
                let cache = NGC_CACHE_AUTH_MANDATORY.to_ne_bytes();
                if NCryptSetProperty(k.handle(), NGC_CACHE_TYPE, &cache, NCRYPT_FLAGS(0)).is_err() {
                    NCryptSetProperty(k.handle(), NGC_CACHE_TYPE_DEPRECATED, &cache, NCRYPT_FLAGS(0))?;
                }
                NCryptSetProperty(
                    k.handle(),
                    NCRYPT_KEY_USAGE_PROPERTY,
                    &NCRYPT_ALLOW_ALL_USAGES.to_ne_bytes(),
                    NCRYPT_FLAGS(0),
                )?;
                set_ui(&k, hwnd, message)?;
                NCryptFinalizeKey(k.0, NCRYPT_FLAGS(0))
            }
        })()
        .map_err(err)
    }

    pub fn delete_key() -> Result<(), String> {
        let p = provider().map_err(err)?;
        match open_key(&p) {
            Ok(mut k) => {
                // NCryptDeleteKey gibt das Handle selbst frei.
                let h = std::mem::take(&mut k.0);
                unsafe { NCryptDeleteKey(h, 0) }.map_err(err)
            }
            Err(e) if matches!(e.code().0, NTE_NO_KEY | NTE_BAD_KEYSET) => Ok(()),
            Err(e) => Err(err(e)),
        }
    }

    /// Öffentlicher Teil als `BCRYPT_RSAPUBLIC_BLOB` (ohne Abfrage).
    pub fn public_key() -> Result<Vec<u8>, String> {
        (|| -> windows::core::Result<Vec<u8>> {
            let p = provider()?;
            let k = open_key(&p)?;
            let mut len = 0u32;
            unsafe {
                NCryptExportKey(k.0, None, BCRYPT_RSAPUBLIC_BLOB, None, None, &mut len, NCRYPT_FLAGS(0))?;
                let mut out = vec![0u8; len as usize];
                NCryptExportKey(k.0, None, BCRYPT_RSAPUBLIC_BLOB, None, Some(&mut out), &mut len, NCRYPT_FLAGS(0))?;
                out.truncate(len as usize);
                Ok(out)
            }
        })()
        .map_err(err)
    }

    /// Verschlüsselt mit dem öffentlichen Schlüssel (reine Rechnung, keine Abfrage).
    pub fn encrypt(public_key: &[u8], data: &[u8]) -> Result<Vec<u8>, String> {
        unsafe {
            let mut alg = BCRYPT_ALG_HANDLE::default();
            BCryptOpenAlgorithmProvider(&mut alg, BCRYPT_RSA_ALGORITHM, PCWSTR::null(), Default::default())
                .ok()
                .map_err(err)?;
            let mut key = BCRYPT_KEY_HANDLE::default();
            let result = BCryptImportKeyPair(alg, None, BCRYPT_RSAPUBLIC_BLOB, &mut key, public_key, 0)
                .ok()
                .and_then(|()| {
                    let mut len = 0u32;
                    BCryptEncrypt(key, Some(data), None, None, None, &mut len, BCRYPT_PAD_PKCS1).ok()?;
                    let mut out = vec![0u8; len as usize];
                    BCryptEncrypt(key, Some(data), None, None, Some(&mut out), &mut len, BCRYPT_PAD_PKCS1).ok()?;
                    out.truncate(len as usize);
                    Ok(out)
                })
                .map_err(err);
            if !key.is_invalid() {
                let _ = BCryptDestroyKey(key);
            }
            let _ = BCryptCloseAlgorithmProvider(alg, 0);
            result
        }
    }

    /// Entschlüsselt mit dem privaten Schlüssel – Windows Hello fragt über `hwnd` nach.
    pub fn decrypt(hwnd: isize, message: &str, data: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
        (|| -> windows::core::Result<Zeroizing<Vec<u8>>> {
            let p = provider()?;
            let k = open_key(&p)?;
            set_ui(&k, hwnd, message)?;
            // Ausgabe ist höchstens so lang wie der Modulus (= Länge der Eingabe).
            let mut out = Zeroizing::new(vec![0u8; data.len()]);
            let mut len = 0u32;
            unsafe { NCryptDecrypt(k.0, Some(data), None, Some(&mut out), &mut len, NCRYPT_PAD_PKCS1_FLAG)? };
            out.truncate(len as usize);
            Ok(out)
        })()
        .map_err(err)
    }
}

#[cfg(not(windows))]
mod imp {
    use zeroize::Zeroizing;

    const NA: &str = "Windows Hello ist nur unter Windows verfügbar";

    pub fn available() -> bool {
        false
    }

    pub fn verify(_hwnd: isize, _message: &str) -> Result<(), String> {
        Err(NA.into())
    }

    pub fn key_exists() -> bool {
        false
    }

    pub fn create_key(_hwnd: isize, _message: &str) -> Result<(), String> {
        Err(NA.into())
    }

    pub fn delete_key() -> Result<(), String> {
        Ok(())
    }

    pub fn public_key() -> Result<Vec<u8>, String> {
        Err(NA.into())
    }

    pub fn encrypt(_public_key: &[u8], _data: &[u8]) -> Result<Vec<u8>, String> {
        Err(NA.into())
    }

    pub fn decrypt(_hwnd: isize, _message: &str, _data: &[u8]) -> Result<Zeroizing<Vec<u8>>, String> {
        Err(NA.into())
    }
}

pub use imp::{available, create_key, decrypt, delete_key, encrypt, key_exists, public_key, verify};

#[cfg(test)]
mod tests {
    /// Nur Abfrage der Verfügbarkeit, ohne Dialog: `cargo test -- --ignored hello_availability --nocapture`
    #[test]
    #[ignore]
    fn hello_availability() {
        println!("Windows Hello verfügbar: {}", super::available());
        println!("Schlüssel zum Entsperren vorhanden: {}", super::key_exists());
    }

    /// Kompletter Durchlauf mit Dialogen: `cargo test -- --ignored hello_roundtrip --nocapture`
    #[test]
    #[ignore]
    fn hello_roundtrip() {
        if !super::key_exists() {
            super::create_key(0, "Remember Key: Test-Schlüssel anlegen").unwrap();
        }
        let pk = super::public_key().unwrap();
        let secret = [7u8; 32];
        let wrapped = super::encrypt(&pk, &secret).unwrap();
        let plain = super::decrypt(0, "Remember Key: Test entschlüsseln", &wrapped).unwrap();
        assert_eq!(&plain[..], &secret);
    }
}
