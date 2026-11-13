//! Update manifest (`latest.json`) shared by the app and the `tiksu-release` tool.
//!
//! The signature is ed25519 over `"tiksu-clipper <version>\n"` followed by the exe bytes, so the
//! version can't be swapped onto an older signed exe and any host can serve the files.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub version: String,
    /// Where the exe is downloaded from (GitHub release asset or tiksu.fi).
    pub url: String,
    #[serde(default)]
    pub notes: String,
    /// Hex ed25519 signature of [`signed_message`].
    pub signature: String,
}

pub fn signed_message(version: &str, exe: &[u8]) -> Vec<u8> {
    let mut msg = format!("tiksu-clipper {}\n", version).into_bytes();
    msg.extend_from_slice(exe);
    msg
}

/// "1.2.3" or "v1.2.3" -> (1, 2, 3).
pub fn parse_version(v: &str) -> Option<(u32, u32, u32)> {
    let mut parts = v.trim().trim_start_matches('v').split('.').map(|p| p.parse::<u32>().ok());
    let v = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(v)
}

#[allow(dead_code)] // used by the release tool and tests
pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn from_hex(s: &str) -> Option<Vec<u8>> {
    let s = s.trim();
    if !s.len().is_multiple_of(2) {
        return None;
    }
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

/// Checks that `exe` is the build signed for `manifest.version` by the holder of `public_key_hex`.
pub fn verify(manifest: &Manifest, exe: &[u8], public_key_hex: &str) -> Result<(), String> {
    let key: [u8; 32] = from_hex(public_key_hex)
        .and_then(|k| k.try_into().ok())
        .ok_or("virheellinen julkinen avain")?;
    let key = VerifyingKey::from_bytes(&key).map_err(|e| e.to_string())?;
    let sig = from_hex(&manifest.signature).ok_or("allekirjoitus ei ole hex-muotoa")?;
    let sig = Signature::from_slice(&sig).map_err(|e| e.to_string())?;
    key.verify(&signed_message(&manifest.version, exe), &sig)
        .map_err(|_| "allekirjoitus ei täsmää: tiedosto ei ole aito Tiksu Clipper -julkaisu".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    #[test]
    fn versions_parse_and_compare() {
        assert_eq!(parse_version("v0.10.2"), Some((0, 10, 2)));
        assert!(parse_version("0.10.2") > parse_version("0.9.9"));
        assert_eq!(parse_version("1.2"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
        assert_eq!(parse_version("1.x.3"), None);
    }

    #[test]
    fn signature_binds_exe_and_version() {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let public = to_hex(key.verifying_key().as_bytes());
        let exe = b"MZ fake exe";
        let mut m = Manifest {
            version: "1.2.3".into(),
            url: "https://example/x.exe".into(),
            notes: String::new(),
            signature: to_hex(&key.sign(&signed_message("1.2.3", exe)).to_bytes()),
        };
        assert_eq!(from_hex(&public).map(|k| to_hex(&k)), Some(public.clone()));
        assert!(verify(&m, exe, &public).is_ok());
        assert!(verify(&m, b"MZ tampered", &public).is_err());
        m.version = "9.9.9".into(); // old signed exe offered as a newer version
        assert!(verify(&m, exe, &public).is_err());
        let other = to_hex(SigningKey::from_bytes(&[8u8; 32]).verifying_key().as_bytes());
        m.version = "1.2.3".into();
        assert!(verify(&m, exe, &other).is_err());
    }
}
