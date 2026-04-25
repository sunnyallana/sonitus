//! Per-field encryption using XChaCha20-Poly1305.
//!
//! We use this for secrets that need to live in the SQLite database next
//! to other (non-secret) columns: OAuth tokens, SMB passwords, S3 secret
//! keys. Each value gets its own random 192-bit nonce, prepended to the
//! ciphertext. The 192-bit nonce space is large enough that random
//! generation is collision-free for any realistic number of fields.
//!
//! ## Wire format
//!
//! ```text
//! [ 24-byte nonce ][ ciphertext + 16-byte Poly1305 tag ]
//! ```
//!
//! Total overhead per field: 40 bytes. The output is binary-safe (no
//! base64) — we store it in `BLOB` columns directly.

use crate::crypto::kdf::VaultKey;
use crate::error::{Result, SonitusError};
use chacha20poly1305::aead::Aead;
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce};

/// Length of the random nonce (XChaCha20 = 192 bits).
const NONCE_LEN: usize = 24;

/// Encrypt `plaintext` with the vault key, producing `nonce || ciphertext`.
///
/// A fresh random nonce is generated for every call via the OS CSPRNG.
/// **Never** reuse a nonce with the same key — XChaCha20 is not
/// nonce-misuse resistant. Random 192-bit nonces are safe in practice.
pub fn encrypt_field(key: &VaultKey, plaintext: &[u8]) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new_from_slice(key.as_bytes())
        .map_err(|_| SonitusError::Crypto("invalid key length"))?;

    let mut nonce_bytes = [0u8; NONCE_LEN];
    getrandom::fill(&mut nonce_bytes)
        .map_err(|_| SonitusError::Crypto("OS RNG unavailable"))?;
    let nonce = XNonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|_| SonitusError::Crypto("encrypt failed"))?;

    let mut out = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// Decrypt a `nonce || ciphertext` blob produced by [`encrypt_field`].
///
/// Returns an error if the blob is too short, the key is wrong, or the
/// authentication tag does not verify. AEAD failures are catastrophic —
/// they mean tampering or key corruption — so callers should not retry.
pub fn decrypt_field(key: &VaultKey, data: &[u8]) -> Result<Vec<u8>> {
    if data.len() < NONCE_LEN + 16 {
        return Err(SonitusError::CryptoTooShort {
            needed: NONCE_LEN + 16,
            got: data.len(),
        });
    }

    let (nonce_bytes, ciphertext) = data.split_at(NONCE_LEN);
    let nonce = XNonce::from_slice(nonce_bytes);

    let cipher = XChaCha20Poly1305::new_from_slice(key.as_bytes())
        .map_err(|_| SonitusError::Crypto("invalid key length"))?;

    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| SonitusError::Crypto("decrypt or auth-tag verification failed"))
}

/// Encrypt a UTF-8 string. Convenience wrapper over [`encrypt_field`].
pub fn encrypt_string(key: &VaultKey, plaintext: &str) -> Result<Vec<u8>> {
    encrypt_field(key, plaintext.as_bytes())
}

/// Decrypt a UTF-8 string previously produced by [`encrypt_string`].
pub fn decrypt_string(key: &VaultKey, data: &[u8]) -> Result<String> {
    let bytes = decrypt_field(key, data)?;
    String::from_utf8(bytes).map_err(|_| SonitusError::Crypto("decrypted bytes are not valid UTF-8"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_key() -> VaultKey {
        let mut k = [0u8; 32];
        getrandom::fill(&mut k).unwrap();
        VaultKey(k)
    }

    #[test]
    fn round_trip_arbitrary_bytes() {
        let key = fresh_key();
        let pt = b"the quick brown fox jumps over the lazy dog";
        let ct = encrypt_field(&key, pt).unwrap();
        let back = decrypt_field(&key, &ct).unwrap();
        assert_eq!(&back[..], pt);
    }

    #[test]
    fn round_trip_empty_input() {
        let key = fresh_key();
        let ct = encrypt_field(&key, b"").unwrap();
        let back = decrypt_field(&key, &ct).unwrap();
        assert!(back.is_empty());
    }

    #[test]
    fn each_encryption_uses_fresh_nonce() {
        let key = fresh_key();
        let pt = b"identical plaintext";
        let a = encrypt_field(&key, pt).unwrap();
        let b = encrypt_field(&key, pt).unwrap();
        assert_ne!(a, b, "two encryptions of the same value must differ (random nonce)");
    }

    #[test]
    fn wrong_key_fails_to_decrypt() {
        let key1 = fresh_key();
        let key2 = fresh_key();
        let ct = encrypt_field(&key1, b"secret").unwrap();
        assert!(decrypt_field(&key2, &ct).is_err());
    }

    #[test]
    fn tampered_ciphertext_fails_to_decrypt() {
        let key = fresh_key();
        let mut ct = encrypt_field(&key, b"the secret").unwrap();
        // Flip a bit in the ciphertext.
        ct[NONCE_LEN + 1] ^= 0x01;
        assert!(decrypt_field(&key, &ct).is_err(), "AEAD tag should detect tampering");
    }

    #[test]
    fn truncated_blob_returns_too_short_error() {
        let key = fresh_key();
        let result = decrypt_field(&key, &[0u8; 10]);
        assert!(matches!(result, Err(SonitusError::CryptoTooShort { .. })));
    }

    #[test]
    fn string_round_trip_preserves_unicode() {
        let key = fresh_key();
        let s = "🎵 música de mañana — Καλημέρα";
        let ct = encrypt_string(&key, s).unwrap();
        let back = decrypt_string(&key, &ct).unwrap();
        assert_eq!(back, s);
    }
}
