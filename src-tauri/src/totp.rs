//! TOTP nach RFC 6238 (HMAC-SHA1/256/512).

use data_encoding::BASE32_NOPAD;
use hmac::{Hmac, Mac};
use zeroize::Zeroizing;

use crate::error::{Error, Result};
use crate::vault::TotpConfig;

fn invalid(msg: &str) -> Error {
    Error::Invalid(format!("TOTP: {msg}"))
}

fn normalize_secret(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_whitespace() && *c != '-' && *c != '=')
        .collect::<String>()
        .to_ascii_uppercase()
}

fn decode_secret(secret: &str) -> Result<Zeroizing<Vec<u8>>> {
    let bytes = BASE32_NOPAD
        .decode(secret.as_bytes())
        .map_err(|_| invalid("Secret ist kein gültiges Base32"))?;
    if bytes.len() < 10 {
        return Err(invalid("Secret ist zu kurz"));
    }
    Ok(Zeroizing::new(bytes))
}

/// Akzeptiert ein Base32-Secret oder eine `otpauth://totp/...`-URI.
pub fn parse_input(input: &str) -> Result<TotpConfig> {
    let input = input.trim();
    let mut cfg = TotpConfig {
        secret: String::new(),
        algorithm: "SHA1".into(),
        digits: 6,
        period: 30,
    };

    if input.to_ascii_lowercase().starts_with("otpauth://") {
        let url = url::Url::parse(input).map_err(|_| invalid("URI ist ungültig"))?;
        if url.host_str().map(|h| h.to_ascii_lowercase()) != Some("totp".into()) {
            return Err(invalid("nur otpauth://totp wird unterstützt"));
        }
        for (k, v) in url.query_pairs() {
            match k.to_ascii_lowercase().as_str() {
                "secret" => cfg.secret = normalize_secret(&v),
                "algorithm" => cfg.algorithm = v.to_ascii_uppercase(),
                "digits" => cfg.digits = v.parse().map_err(|_| invalid("digits ungültig"))?,
                "period" => cfg.period = v.parse().map_err(|_| invalid("period ungültig"))?,
                _ => {}
            }
        }
    } else {
        cfg.secret = normalize_secret(input);
    }

    if cfg.secret.is_empty() {
        return Err(invalid("Secret fehlt"));
    }
    if !matches!(cfg.algorithm.as_str(), "SHA1" | "SHA256" | "SHA512") {
        return Err(invalid("Algorithmus nicht unterstützt"));
    }
    if !(6..=8).contains(&cfg.digits) {
        return Err(invalid("digits muss 6–8 sein"));
    }
    if !(15..=300).contains(&cfg.period) {
        return Err(invalid("period muss 15–300 Sekunden sein"));
    }
    decode_secret(&cfg.secret)?;
    Ok(cfg)
}

/// Darstellung zum Bearbeiten: reines Secret bei Standardwerten, sonst URI.
pub fn to_input(cfg: &TotpConfig) -> String {
    if cfg.algorithm == "SHA1" && cfg.digits == 6 && cfg.period == 30 {
        cfg.secret.clone()
    } else {
        format!(
            "otpauth://totp/?secret={}&algorithm={}&digits={}&period={}",
            cfg.secret, cfg.algorithm, cfg.digits, cfg.period
        )
    }
}

fn hmac_digest(algorithm: &str, key: &[u8], msg: &[u8]) -> Vec<u8> {
    macro_rules! run {
        ($h:ty) => {{
            let mut mac = <Hmac<$h>>::new_from_slice(key).expect("HMAC akzeptiert jede Schlüssellänge");
            mac.update(msg);
            mac.finalize().into_bytes().to_vec()
        }};
    }
    match algorithm {
        "SHA256" => run!(sha2::Sha256),
        "SHA512" => run!(sha2::Sha512),
        _ => run!(sha1::Sha1),
    }
}

fn hotp(algorithm: &str, key: &[u8], counter: u64, digits: u32) -> String {
    let digest = hmac_digest(algorithm, key, &counter.to_be_bytes());
    let offset = (digest[digest.len() - 1] & 0x0f) as usize;
    let bin = u32::from_be_bytes(digest[offset..offset + 4].try_into().unwrap()) & 0x7fff_ffff;
    let code = bin % 10u32.pow(digits);
    format!("{:0width$}", code, width = digits as usize)
}

/// Liefert Code und verbleibende Sekunden bis zum Wechsel.
pub fn generate(cfg: &TotpConfig, unix_time: u64) -> Result<(String, u64)> {
    let key = decode_secret(&cfg.secret)?;
    let code = hotp(&cfg.algorithm, &key, unix_time / cfg.period, cfg.digits);
    Ok((code, cfg.period - unix_time % cfg.period))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(raw_key: &[u8], algorithm: &str) -> TotpConfig {
        TotpConfig {
            secret: BASE32_NOPAD.encode(raw_key),
            algorithm: algorithm.into(),
            digits: 8,
            period: 30,
        }
    }

    // Testvektoren aus RFC 6238, Anhang B.
    #[test]
    fn rfc6238_vectors() {
        let sha1 = cfg(b"12345678901234567890", "SHA1");
        let sha256 = cfg(b"12345678901234567890123456789012", "SHA256");
        let sha512 = cfg(
            b"1234567890123456789012345678901234567890123456789012345678901234",
            "SHA512",
        );
        let cases: [(u64, &str, &str, &str); 6] = [
            (59, "94287082", "46119246", "90693936"),
            (1111111109, "07081804", "68084774", "25091201"),
            (1111111111, "14050471", "67062674", "99943326"),
            (1234567890, "89005924", "91819424", "93441116"),
            (2000000000, "69279037", "90698825", "38618901"),
            (20000000000, "65353130", "77737706", "47863826"),
        ];
        for (t, e1, e256, e512) in cases {
            assert_eq!(generate(&sha1, t).unwrap().0, e1, "SHA1 t={t}");
            assert_eq!(generate(&sha256, t).unwrap().0, e256, "SHA256 t={t}");
            assert_eq!(generate(&sha512, t).unwrap().0, e512, "SHA512 t={t}");
        }
    }

    #[test]
    fn remaining_seconds() {
        let c = cfg(b"12345678901234567890", "SHA1");
        assert_eq!(generate(&c, 59).unwrap().1, 1);
        assert_eq!(generate(&c, 60).unwrap().1, 30);
    }

    #[test]
    fn parses_plain_secret() {
        let c = parse_input("jbsw y3dp ehpk 3pxp").unwrap();
        assert_eq!(c.secret, "JBSWY3DPEHPK3PXP");
        assert_eq!((c.digits, c.period, c.algorithm.as_str()), (6, 30, "SHA1"));
        assert_eq!(to_input(&c), "JBSWY3DPEHPK3PXP");
    }

    #[test]
    fn parses_otpauth_uri() {
        let c = parse_input(
            "otpauth://totp/Example:alice@example.com?secret=JBSWY3DPEHPK3PXP&issuer=Example&algorithm=SHA256&digits=8&period=60",
        )
        .unwrap();
        assert_eq!(c.algorithm, "SHA256");
        assert_eq!((c.digits, c.period), (8, 60));
        assert_eq!(parse_input(&to_input(&c)).unwrap().period, 60);
    }

    #[test]
    fn rejects_bad_input() {
        assert!(parse_input("").is_err());
        assert!(parse_input("not base32 !!!").is_err());
        assert!(parse_input("otpauth://hotp/x?secret=JBSWY3DPEHPK3PXP").is_err());
        assert!(parse_input("otpauth://totp/x?secret=JBSWY3DPEHPK3PXP&digits=12").is_err());
    }
}
