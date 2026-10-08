//! Signed releases. A release of Glass or glass-evo carries `SHA256SUMS`,
//! every asset's digest, and `SHA256SUMS.sig`, that file's Ed25519
//! signature made in the release workflow with the project's key. The
//! public key is here, read from the repository's `keys/release-signing.pub`
//! at build time. A remote holds a release that carries a signature to it,
//! and takes a release without one as it did before signing.

use ed25519_dalek::{Signature, Verifier, VerifyingKey};

const PUBLIC_KEY_PEM: &str = include_str!("../../../../keys/release-signing.pub");

pub const SUMS_NAME: &str = "SHA256SUMS";
pub const SIG_NAME: &str = "SHA256SUMS.sig";
/// The sums are small; a signature is 64 bytes.
pub const MAX_SUMS: u64 = 64 * 1024;
pub const MAX_SIG: u64 = 4096;

/// The project's public key.
pub fn public_key() -> Result<VerifyingKey, String> {
    key_from_pem(PUBLIC_KEY_PEM)
}

/// An Ed25519 public key out of its PEM (SubjectPublicKeyInfo): the
/// base64 body decoded, the twelve-byte DER prefix of the key's kind passed
/// over, thirty-two bytes of key.
pub fn key_from_pem(pem: &str) -> Result<VerifyingKey, String> {
    const PREFIX: [u8; 12] = [
        0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00,
    ];
    let body: String = pem
        .lines()
        .filter(|l| !l.trim().starts_with("-----"))
        .map(str::trim)
        .collect();
    let der = base64_decode(&body)?;
    if der.len() != 44 || der[..12] != PREFIX {
        return Err("the public key is not an Ed25519 key".to_string());
    }
    let mut raw = [0u8; 32];
    raw.copy_from_slice(&der[12..]);
    VerifyingKey::from_bytes(&raw).map_err(|e| format!("the public key is not usable: {e}"))
}

/// Whether `sig` is the signature of `sums` by `key`.
pub fn verify_with(key: &VerifyingKey, sums: &[u8], sig: &[u8]) -> bool {
    let Ok(signature) = Signature::from_slice(sig) else {
        return false;
    };
    key.verify(sums, &signature).is_ok()
}

/// The digest the sums state for `name`, as `sha256sum` writes the lines
/// (`<hex>  <name>`, a `*` before the name for binary mode); none where
/// they do not name it.
pub fn digest_in_sums(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hex, rest) = line.trim().split_once(' ')?;
        let file = rest.trim_start().trim_start_matches('*').trim_end();
        (hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()) && file == name)
            .then(|| hex.to_ascii_lowercase())
    })
}

/// A file held to a signed release: the signature checked by `key`, and
/// `digest` held to the sums' line for `asset`. `unsigned` where the
/// release carries no sums and no signature, as every release before
/// signing; an error where it carries them and they do not hold.
pub fn held_to_signature(
    key: &VerifyingKey,
    asset: &str,
    digest: &str,
    sums: Option<&[u8]>,
    sig: Option<&[u8]>,
) -> Result<&'static str, String> {
    let (Some(sums), Some(sig)) = (sums, sig) else {
        return Ok("unsigned");
    };
    if !verify_with(key, sums, sig) {
        return Err(
            "the release's signature does not verify; the release is not as published".into(),
        );
    }
    let text = String::from_utf8_lossy(sums);
    match digest_in_sums(&text, asset) {
        None => Err(format!("the release's signed sums do not name {asset}")),
        Some(stated) if stated != digest.to_ascii_lowercase() => Err(format!(
            "the archive does not match the signed digest of {asset}; it is not the release as published"
        )),
        Some(_) => Ok("signed"),
    }
}

/// Standard base64, padding passed over.
fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf: u32 = 0;
    let mut bits: u32 = 0;
    for c in s.bytes() {
        if c == b'=' {
            break;
        }
        let v = TABLE
            .iter()
            .position(|&t| t == c)
            .ok_or_else(|| "the key is not base64".to_string())? as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    #[test]
    fn the_project_key_reads_out_of_its_pem() {
        let key = public_key().expect("the repository's key");
        // The PEM's body begins "MCowBQYDK2VwAyEAoUKU": the key's first bytes are a1 42 94.
        assert_eq!(&key.to_bytes()[..3], &[0xa1, 0x42, 0x94]);
        assert!(key_from_pem(
            "-----BEGIN PUBLIC KEY-----\nMCowBQYDK2VwAyEA\n-----END PUBLIC KEY-----\n"
        )
        .is_err());
        assert!(key_from_pem("not a key").is_err());
    }

    #[test]
    fn a_signature_over_the_sums_holds_with_its_key_alone_and_the_digest_with_the_sums() {
        let mine = SigningKey::from_bytes(&[7u8; 32]);
        let sums = format!(
            "{}  glass-0.9.21-x64.tar.gz\n{} *glass-0.9.21.zip\n",
            "ab".repeat(32),
            "cd".repeat(32)
        );
        let sig = mine.sign(sums.as_bytes()).to_bytes();
        let key = mine.verifying_key();
        assert!(verify_with(&key, sums.as_bytes(), &sig));
        assert!(!verify_with(&key, format!("{sums}x").as_bytes(), &sig));
        assert!(!verify_with(&key, sums.as_bytes(), &sig[..63]));
        assert!(!verify_with(
            &SigningKey::from_bytes(&[8u8; 32]).verifying_key(),
            sums.as_bytes(),
            &sig
        ));
        assert!(
            !verify_with(&public_key().unwrap(), sums.as_bytes(), &sig),
            "not the project's key"
        );
        assert_eq!(
            digest_in_sums(&sums, "glass-0.9.21-x64.tar.gz").as_deref(),
            Some("ab".repeat(32).as_str())
        );
        assert_eq!(
            digest_in_sums(&sums, "glass-0.9.21.zip").as_deref(),
            Some("cd".repeat(32).as_str())
        );
        assert_eq!(digest_in_sums(&sums, "glass-0.9.21-arm.tar.gz"), None);
        // Held: as published; another digest; not named; unsigned as before.
        let ab = "ab".repeat(32);
        assert_eq!(
            held_to_signature(
                &key,
                "glass-0.9.21-x64.tar.gz",
                &ab,
                Some(sums.as_bytes()),
                Some(&sig)
            ),
            Ok("signed")
        );
        assert!(held_to_signature(
            &key,
            "glass-0.9.21-x64.tar.gz",
            &"ee".repeat(32),
            Some(sums.as_bytes()),
            Some(&sig)
        )
        .unwrap_err()
        .contains("signed digest"));
        assert!(held_to_signature(
            &key,
            "glass-0.9.21-arm.tar.gz",
            &ab,
            Some(sums.as_bytes()),
            Some(&sig)
        )
        .unwrap_err()
        .contains("do not name"));
        assert!(held_to_signature(
            &key,
            "glass-0.9.21-x64.tar.gz",
            &ab,
            Some(sums.as_bytes()),
            Some(&[0u8; 64])
        )
        .unwrap_err()
        .contains("does not verify"));
        assert_eq!(
            held_to_signature(&key, "glass-0.9.21-x64.tar.gz", &ab, None, None),
            Ok("unsigned")
        );
    }
}
