//! Argon2id password-based key derivation.
//!
//! The user's passphrase is the only secret the user ever has to remember.
//! Everything else — OAuth tokens, source passwords, the SQLite vault — is
//! protected by a key derived from that passphrase via Argon2id.
//!
//! ## Parameters
//!
//! Per OWASP's 2024 recommendations for password-based encryption:
//!
//! - **Memory cost** (`m`): 64 MiB (`65536` KiB)
//! - **Time cost** (`t`): 3 iterations
//! - **Parallelism** (`p`): 4 lanes
//! - **Output length**: 32 bytes (256 bits)
//! - **Variant**: Argon2id (resistant to both GPU and side-channel attacks)
//!
//! On a 2024-era laptop (M2 / Ryzen 7), derivation takes ~600ms — slow
//! enough to throttle attackers, fast enough that users don't notice.
//!
//! ## Salt management
//!
//! The salt is **not secret**. It's stored in plaintext at
//! `~/.config/sonitus/vault.salt` (32 random bytes). Only the passphrase
//! itself needs to be kept private. Generating a fresh salt on first run
//! ensures rainbow tables can't be precomputed against Sonitus users
//! collectively.

use crate::error::{Result, SonitusError};
use argon2::{Algorithm, Argon2, Params, Version};
use std::path::Path;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// A 32-byte symmetric key derived from the user's passphrase.
///
/// `VaultKey` derives `Zeroize` and `ZeroizeOnDrop` so the key bytes are
/// wiped from memory when the value goes out of scope. The compiler cannot
/// optimize this away because `Zeroize::zeroize` uses `write_volatile`.
///
/// ```ignore
/// use sonitus_core::crypto::VaultKey;
///
/// let salt = VaultKey::generate_salt();
/// let key = VaultKey::derive("correct horse battery staple", &salt)?;
/// // key is dropped — its bytes are now zero in memory.
/// ```
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct VaultKey(pub [u8; 32]);

/// Argon2id memory cost (KiB). 65536 = 64 MiB.
const ARGON2_M_COST: u32 = 65_536;
/// Argon2id time cost (iterations).
const ARGON2_T_COST: u32 = 3;
/// Argon2id parallelism (lanes).
const ARGON2_P_COST: u32 = 4;
/// Output key length in bytes.
const KEY_LEN: usize = 32;
/// Salt length in bytes.
pub const SALT_LEN: usize = 32;

impl VaultKey {
    /// Derive a vault key from a passphrase and salt.
    ///
    /// This is intentionally slow (~600ms on a modern laptop) — that
    /// slowness is the security property. Don't call this in a hot loop.
    pub fn derive(passphrase: &str, salt: &[u8; SALT_LEN]) -> Result<Self> {
        let params = Params::new(ARGON2_M_COST, ARGON2_T_COST, ARGON2_P_COST, Some(KEY_LEN))
            .map_err(|e| SonitusError::KdfFailed(e.to_string()))?;
        let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
        let mut key = [0u8; KEY_LEN];
        argon2
            .hash_password_into(passphrase.as_bytes(), salt, &mut key)
            .map_err(|e| SonitusError::KdfFailed(e.to_string()))?;
        Ok(VaultKey(key))
    }

    /// Generate a fresh 32-byte random salt using the OS CSPRNG.
    ///
    /// Panics if the OS RNG is unavailable — this is intentional. There's
    /// no recovery path: if `/dev/urandom`, `BCryptGenRandom`, or
    /// `getentropy` cannot be reached, the device is in an unsafe state.
    pub fn generate_salt() -> [u8; SALT_LEN] {
        let mut salt = [0u8; SALT_LEN];
        getrandom::fill(&mut salt).expect("OS CSPRNG must be available");
        salt
    }

    /// Read a salt from disk, generating and persisting one if it doesn't exist.
    ///
    /// The salt file is intentionally written with `0o600` permissions where
    /// supported, even though the contents are not secret — least-privilege
    /// hygiene.
    pub fn load_or_generate_salt(path: &Path) -> Result<[u8; SALT_LEN]> {
        if path.exists() {
            let bytes = std::fs::read(path)?;
            if bytes.len() != SALT_LEN {
                return Err(SonitusError::Crypto("salt file has wrong length"));
            }
            let mut salt = [0u8; SALT_LEN];
            salt.copy_from_slice(&bytes);
            Ok(salt)
        } else {
            let salt = Self::generate_salt();
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(path, salt)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = std::fs::metadata(path)?.permissions();
                perms.set_mode(0o600);
                std::fs::set_permissions(path, perms)?;
            }
            Ok(salt)
        }
    }

    /// Borrow the raw key bytes. Use with care — never log this output.
    pub fn as_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

impl std::fmt::Debug for VaultKey {
    /// Debug output never reveals the key. We show only the byte length.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "VaultKey([REDACTED; {KEY_LEN}])")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_is_deterministic_given_same_inputs() {
        let salt = [42u8; SALT_LEN];
        let a = VaultKey::derive("hunter2", &salt).unwrap();
        let b = VaultKey::derive("hunter2", &salt).unwrap();
        assert_eq!(a.0, b.0, "Argon2id must be deterministic for matching inputs");
    }

    #[test]
    fn derive_differs_when_salt_differs() {
        let a = VaultKey::derive("hunter2", &[1u8; SALT_LEN]).unwrap();
        let b = VaultKey::derive("hunter2", &[2u8; SALT_LEN]).unwrap();
        assert_ne!(a.0, b.0, "different salts must yield different keys");
    }

    #[test]
    fn derive_differs_when_passphrase_differs() {
        let salt = [42u8; SALT_LEN];
        let a = VaultKey::derive("hunter2", &salt).unwrap();
        let b = VaultKey::derive("hunter3", &salt).unwrap();
        assert_ne!(a.0, b.0, "different passphrases must yield different keys");
    }

    #[test]
    fn generate_salt_is_random() {
        let a = VaultKey::generate_salt();
        let b = VaultKey::generate_salt();
        assert_ne!(a, b, "OS RNG must produce distinct salts (P(collision) ≈ 2^-256)");
    }

    #[test]
    fn debug_does_not_reveal_key_bytes() {
        let key = VaultKey([1u8; 32]);
        let s = format!("{key:?}");
        assert!(s.contains("REDACTED"));
        assert!(!s.contains('1'), "raw bytes leaked into Debug output");
    }
}
