//! Passwortstärke mit zxcvbn (Wörterbücher, Muster, Tastaturfolgen, Jahreszahlen …)
//! statt einer reinen Zeichenvorrat-mal-Länge-Schätzung.

use serde::Serialize;

use crate::error::{Error, Result};

/// zxcvbn-Score 3 ≙ < 10^10 Rateversuche: Schutz gegen Offline-Angriffe auf langsame Hashes.
pub const MIN_MASTER_SCORE: u8 = 3;

/// Begriffe, die in einem Master-Passwort keine Stärke bringen: App-Bezug und häufige
/// deutsche Passwortwörter (zxcvbn bringt nur englische Wörterbücher mit).
const EXTRA_WORDS: &[&str] = &[
    "remember", "key", "rememberkey", "tresor", "master", "passwort", "kennwort", "geheim", "zugang",
    "hallo", "willkommen", "schatz", "schatzi", "liebe", "ichliebedich", "mausi", "hase", "baby", "sonne",
    "sommer", "winter", "fruehling", "frühling", "herbst", "januar", "februar", "maerz", "märz", "april",
    "mai", "juni", "juli", "august", "september", "oktober", "november", "dezember", "montag", "freitag",
    "fussball", "fußball", "bayern", "borussia", "schalke", "deutschland", "berlin", "hamburg", "muenchen",
    "münchen", "koeln", "köln", "admin", "test", "start", "anfang", "computer", "internet", "arbeit",
    "firma", "schule", "familie", "mutter", "vater", "katze", "hund", "blume", "engel", "teufel",
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Strength {
    /// 0 (sehr schwach) bis 4 (sehr stark).
    pub score: u8,
    pub guesses_log10: f64,
    /// Äquivalente Entropie in Bit (log2 der geschätzten Rateversuche).
    pub bits: f64,
}

pub fn evaluate(password: &str) -> Strength {
    if password.is_empty() {
        return Strength { score: 0, guesses_log10: 0.0, bits: 0.0 };
    }
    let e = zxcvbn::zxcvbn(password, EXTRA_WORDS);
    Strength {
        score: u8::from(e.score()),
        guesses_log10: e.guesses_log10(),
        bits: e.guesses_log10() * std::f64::consts::LOG2_10,
    }
}

pub fn check_master(password: &str) -> Result<()> {
    if evaluate(password).score < MIN_MASTER_SCORE {
        return Err(Error::Invalid(
            "Das Master-Passwort ist zu leicht zu erraten. Verwenden Sie eine längere Passphrase aus mehreren zufälligen Wörtern."
                .into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_common_patterns() {
        for pw in ["Passwort12", "password123", "Sommer2024!", "qwertzuiop", "RememberKey1!", "Schatzi1990", "FCBayern2025"] {
            assert!(check_master(pw).is_err(), "{pw} sollte abgelehnt werden");
        }
    }

    #[test]
    fn accepts_random_passphrase() {
        assert!(check_master("Tafel Krokus Winkel Ozean 27").is_ok());
        assert!(check_master("x7#Lq9!vR2@mZp").is_ok());
    }

    #[test]
    fn empty_is_zero() {
        assert_eq!(evaluate("").score, 0);
    }
}
