//! Registriert den Native-Messaging-Host (`remember-key-browser.exe`) für Chrome, Edge und
//! Firefox/Zen beim aktuellen Benutzer (HKCU, ohne Adminrechte).
//!
//! Die Browser finden den Host über einen Registry-Eintrag, der auf eine JSON-Datei zeigt.
//! Diese nennt den Pfad des Hosts und die einzige Erweiterung, die ihn starten darf.
//! Bei jedem Einschalten des KI-Zugriffs neu geschrieben, damit der Pfad zur aktuell
//! laufenden App passt (Dev-Build oder installierte Version).

use std::path::{Path, PathBuf};

use rk_agent::{BROWSER_HOST_EXE, CHROME_EXTENSION_ID, GECKO_EXTENSION_ID, NATIVE_HOST_NAME};
use serde_json::json;

const DESCRIPTION: &str = "Remember Key – Login-Ausfüllen für KI-Assistenten";

/// Registry-Pfade (unter HKCU) und welche Manifest-Variante sie bekommen.
const TARGETS: [(&str, Flavor); 3] = [
    (r"Software\Google\Chrome\NativeMessagingHosts", Flavor::Chromium),
    (r"Software\Microsoft\Edge\NativeMessagingHosts", Flavor::Chromium),
    // Auch Zen und andere Firefox-Ableger lesen diesen Pfad.
    (r"Software\Mozilla\NativeMessagingHosts", Flavor::Gecko),
];

#[derive(Clone, Copy)]
enum Flavor {
    Chromium,
    Gecko,
}

fn manifest(flavor: Flavor, host: &Path) -> serde_json::Value {
    let mut m = json!({
        "name": NATIVE_HOST_NAME,
        "description": DESCRIPTION,
        "path": host.display().to_string(),
        "type": "stdio",
    });
    match flavor {
        Flavor::Chromium => m["allowed_origins"] = json!([format!("chrome-extension://{CHROME_EXTENSION_ID}/")]),
        Flavor::Gecko => m["allowed_extensions"] = json!([GECKO_EXTENSION_ID]),
    }
    m
}

/// Pfad des Hosts neben der App, sofern vorhanden.
pub fn host_path() -> Option<PathBuf> {
    let p = std::env::current_exe().ok()?.with_file_name(BROWSER_HOST_EXE);
    p.exists().then_some(p)
}

/// Schreibt Manifeste nach `<dir>/native/` und trägt sie in der Registry ein.
pub fn register(dir: &Path) -> Result<(), String> {
    let host = host_path().ok_or("remember-key-browser.exe fehlt neben der App")?;
    let native = dir.join("native");
    std::fs::create_dir_all(&native).map_err(|e| e.to_string())?;
    for (flavor, file) in [(Flavor::Chromium, "chromium.json"), (Flavor::Gecko, "gecko.json")] {
        let text = serde_json::to_string_pretty(&manifest(flavor, &host)).map_err(|e| e.to_string())?;
        std::fs::write(native.join(file), text).map_err(|e| e.to_string())?;
    }
    for (key, flavor) in TARGETS {
        let file = native.join(match flavor {
            Flavor::Chromium => "chromium.json",
            Flavor::Gecko => "gecko.json",
        });
        set_default_value(&format!(r"{key}\{NATIVE_HOST_NAME}"), &file.display().to_string())?;
    }
    Ok(())
}

#[cfg(windows)]
fn set_default_value(subkey: &str, value: &str) -> Result<(), String> {
    use windows_sys::Win32::System::Registry::{RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ};

    let key: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
    let data: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    // Legt den Schlüssel bei Bedarf an; `null` als Wertname = Standardwert.
    let rc = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            std::ptr::null(),
            REG_SZ,
            data.as_ptr().cast(),
            (data.len() * 2) as u32,
        )
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(format!("Registry-Eintrag {subkey} konnte nicht geschrieben werden (Fehler {rc})"))
    }
}

#[cfg(not(windows))]
fn set_default_value(_subkey: &str, _value: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifests_allow_only_our_extension() {
        let host = Path::new(r"C:\App\remember-key-browser.exe");
        let c = manifest(Flavor::Chromium, host);
        assert_eq!(c["name"], NATIVE_HOST_NAME);
        assert_eq!(c["path"], r"C:\App\remember-key-browser.exe");
        assert_eq!(c["allowed_origins"][0], format!("chrome-extension://{CHROME_EXTENSION_ID}/"));
        assert!(c.get("allowed_extensions").is_none());
        let g = manifest(Flavor::Gecko, host);
        assert_eq!(g["allowed_extensions"][0], GECKO_EXTENSION_ID);
        assert!(g.get("allowed_origins").is_none());
    }
}
