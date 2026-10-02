//! Passwort-Generator auf Basis von `OsRng` (gleichverteilt, kein Modulo-Bias).

use rand::rngs::OsRng;
use rand::seq::SliceRandom;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

const LOWER: &str = "abcdefghijklmnopqrstuvwxyz";
const UPPER: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &str = "0123456789";
const SYMBOLS: &str = "!@#$%^&*()-_=+[]{};:,.<>/?~";
const AMBIGUOUS: &str = "Il1O0o|`'\"";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenOptions {
    pub length: usize,
    pub lowercase: bool,
    pub uppercase: bool,
    pub digits: bool,
    pub symbols: bool,
    pub exclude_ambiguous: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Generated {
    pub password: String,
    pub entropy_bits: f64,
}

pub fn generate(opts: &GenOptions) -> Result<Generated> {
    let sets: Vec<Vec<char>> = [
        (opts.lowercase, LOWER),
        (opts.uppercase, UPPER),
        (opts.digits, DIGITS),
        (opts.symbols, SYMBOLS),
    ]
    .iter()
    .filter(|(on, _)| *on)
    .map(|(_, s)| {
        s.chars()
            .filter(|c| !(opts.exclude_ambiguous && AMBIGUOUS.contains(*c)))
            .collect()
    })
    .collect();

    if sets.is_empty() {
        return Err(Error::Invalid("Mindestens ein Zeichensatz muss gewählt sein".into()));
    }
    if !(4..=128).contains(&opts.length) || opts.length < sets.len() {
        return Err(Error::Invalid("Länge muss zwischen 4 und 128 liegen".into()));
    }

    let pool: Vec<char> = sets.iter().flatten().copied().collect();
    let mut rng = OsRng;
    // Je ein Zeichen aus jedem gewählten Satz garantieren, Rest aus dem Gesamtpool.
    let mut chars: Vec<char> = sets.iter().map(|s| s[rng.gen_range(0..s.len())]).collect();
    while chars.len() < opts.length {
        chars.push(pool[rng.gen_range(0..pool.len())]);
    }
    chars.shuffle(&mut rng);

    Ok(Generated {
        password: chars.into_iter().collect(),
        entropy_bits: opts.length as f64 * (pool.len() as f64).log2(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(length: usize) -> GenOptions {
        GenOptions {
            length,
            lowercase: true,
            uppercase: true,
            digits: true,
            symbols: true,
            exclude_ambiguous: false,
        }
    }

    #[test]
    fn length_and_all_sets_present() {
        for _ in 0..200 {
            let p = generate(&opts(8)).unwrap().password;
            assert_eq!(p.chars().count(), 8);
            assert!(p.chars().any(|c| c.is_ascii_lowercase()));
            assert!(p.chars().any(|c| c.is_ascii_uppercase()));
            assert!(p.chars().any(|c| c.is_ascii_digit()));
            assert!(p.chars().any(|c| SYMBOLS.contains(c)));
        }
    }

    #[test]
    fn excludes_ambiguous() {
        let mut o = opts(128);
        o.exclude_ambiguous = true;
        for _ in 0..50 {
            let p = generate(&o).unwrap().password;
            assert!(!p.chars().any(|c| AMBIGUOUS.contains(c)));
        }
    }

    #[test]
    fn rejects_invalid() {
        let mut o = opts(3);
        assert!(generate(&o).is_err());
        o.length = 20;
        o.lowercase = false;
        o.uppercase = false;
        o.digits = false;
        o.symbols = false;
        assert!(generate(&o).is_err());
    }
}
