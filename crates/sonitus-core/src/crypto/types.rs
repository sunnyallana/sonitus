//! `Zeroize`-aware wrapper types for secret values.
//!
//! Anywhere we hold a secret in memory — OAuth tokens, source passwords,
//! S3 secret keys — we wrap it in [`Secret`] so:
//!
//! 1. The value is wiped from memory when dropped (`ZeroizeOnDrop`).
//! 2. Accidental `Debug` formatting prints `Secret<***>` instead of the value.
//! 3. The type doesn't implement `Clone` or `Display`, forcing the user to
//!    explicitly call `.expose()` at the point of use — making leaks visible
//!    in code review.
//!
//! The point of these types is to make it ergonomic to do the right thing
//! and slightly awkward to do the wrong thing.

use std::fmt;
use zeroize::{Zeroize, ZeroizeOnDrop};

/// A wrapper for any secret value that should be wiped on drop.
///
/// Uses `ZeroizeOnDrop` so that when the value goes out of scope, its bytes
/// are overwritten with zeros via `core::ptr::write_volatile` — the
/// compiler cannot optimize this away.
///
/// `Secret<T>` does **not** implement `Clone`, `Copy`, `Display`, or
/// `Serialize`. To use the inner value, call [`Secret::expose`] — this
/// makes secret access greppable.
///
/// ```ignore
/// use sonitus_core::crypto::Secret;
///
/// let token = Secret::new(String::from("ya29.a0AfH..."));
/// reqwest_client.bearer_auth(token.expose());
/// // Now `token` is dropped — its bytes are zeroed.
/// ```
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct Secret<T: Zeroize>(T);

impl<T: Zeroize> Secret<T> {
    /// Wrap a value as a secret.
    pub fn new(value: T) -> Self {
        Secret(value)
    }

    /// Borrow the inner value. Use with care — never log this output.
    /// Calling `.expose()` makes secret access greppable in code review.
    pub fn expose(&self) -> &T {
        &self.0
    }

    /// Mutably borrow the inner value. Used for in-place token refresh.
    pub fn expose_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: Zeroize + Default> Secret<T> {
    /// Consume the wrapper, leaving a zeroed `T::default()` in its place,
    /// and return ownership of the inner value to the caller.
    ///
    /// After calling this, the **caller** is responsible for wiping the
    /// returned value when finished — `Secret`'s drop guarantee no longer
    /// applies. Most callers should prefer [`Secret::expose`].
    pub fn into_inner(mut self) -> T {
        std::mem::take(&mut self.0)
        // `self` drops here — the now-default-T inside is zeroized harmlessly.
    }
}

impl<T: Zeroize> fmt::Debug for Secret<T> {
    /// Always renders as `Secret<***>` — the inner value is never shown.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Secret<***>")
    }
}

impl<T: Zeroize> From<T> for Secret<T> {
    fn from(value: T) -> Self {
        Secret(value)
    }
}

// ──────────────────────────────────────────────────────────────────────────
// Concrete aliases
// ──────────────────────────────────────────────────────────────────────────

/// An OAuth 2.0 access or refresh token.
pub type OAuthToken = Secret<String>;

/// A password for an SMB share, S3 bucket, etc.
pub type SourcePassword = Secret<String>;

/// An AWS secret access key.
pub type S3SecretKey = Secret<String>;

/// Bundle of credentials for a source, encrypted as a unit.
///
/// This is what we serialize (post-encryption) into the `credentials_enc`
/// column of the `sources` table. The `kind` discriminator tells the
/// source provider which fields to expect after decryption.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct SourceCredential {
    /// Discriminator: matches `SourceKind` from the source provider module.
    /// Not secret — the kind of source is fine to know.
    #[zeroize(skip)]
    pub kind: String,
    /// Primary credential (token or username).
    pub primary: String,
    /// Optional secondary credential (refresh token, password).
    pub secondary: Option<String>,
    /// Optional expiration timestamp (Unix seconds). Plaintext is fine —
    /// timestamps are not secrets.
    #[zeroize(skip)]
    pub expires_at: Option<i64>,
}

impl fmt::Debug for SourceCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SourceCredential")
            .field("kind", &self.kind)
            .field("primary", &"***")
            .field("secondary", &self.secondary.as_ref().map(|_| "***"))
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

impl SourceCredential {
    /// Serialize this credential to a plaintext byte buffer ready for
    /// encryption. Format is a length-prefixed concatenation:
    /// `[kind_len:u16][kind][primary_len:u32][primary][has_secondary:u8][secondary_len:u32][secondary][expires_at:i64]`.
    /// Stable across versions — bump format byte if changed.
    pub fn to_plaintext(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(64 + self.primary.len()
            + self.secondary.as_ref().map_or(0, String::len));
        // Format version
        out.push(1u8);
        // kind
        let kind_bytes = self.kind.as_bytes();
        out.extend_from_slice(&(kind_bytes.len() as u16).to_le_bytes());
        out.extend_from_slice(kind_bytes);
        // primary
        let p = self.primary.as_bytes();
        out.extend_from_slice(&(p.len() as u32).to_le_bytes());
        out.extend_from_slice(p);
        // secondary
        if let Some(s) = &self.secondary {
            out.push(1u8);
            let sb = s.as_bytes();
            out.extend_from_slice(&(sb.len() as u32).to_le_bytes());
            out.extend_from_slice(sb);
        } else {
            out.push(0u8);
        }
        // expires_at
        out.extend_from_slice(&self.expires_at.unwrap_or(0).to_le_bytes());
        out.push(if self.expires_at.is_some() { 1u8 } else { 0u8 });
        out
    }

    /// Inverse of [`Self::to_plaintext`].
    pub fn from_plaintext(bytes: &[u8]) -> crate::error::Result<Self> {
        use crate::error::SonitusError;
        let mut cursor = 0usize;
        let take = |cursor: &mut usize, n: usize| -> crate::error::Result<&[u8]> {
            if bytes.len() < *cursor + n {
                return Err(SonitusError::Crypto("credential plaintext truncated"));
            }
            let s = &bytes[*cursor..*cursor + n];
            *cursor += n;
            Ok(s)
        };

        let version = take(&mut cursor, 1)?[0];
        if version != 1 {
            return Err(SonitusError::Crypto("unsupported credential format version"));
        }
        let kind_len = u16::from_le_bytes(take(&mut cursor, 2)?.try_into().unwrap()) as usize;
        let kind = std::str::from_utf8(take(&mut cursor, kind_len)?)
            .map_err(|_| SonitusError::Crypto("credential kind not UTF-8"))?
            .to_string();

        let p_len = u32::from_le_bytes(take(&mut cursor, 4)?.try_into().unwrap()) as usize;
        let primary = std::str::from_utf8(take(&mut cursor, p_len)?)
            .map_err(|_| SonitusError::Crypto("credential primary not UTF-8"))?
            .to_string();

        let has_sec = take(&mut cursor, 1)?[0];
        let secondary = if has_sec == 1 {
            let s_len = u32::from_le_bytes(take(&mut cursor, 4)?.try_into().unwrap()) as usize;
            Some(
                std::str::from_utf8(take(&mut cursor, s_len)?)
                    .map_err(|_| SonitusError::Crypto("credential secondary not UTF-8"))?
                    .to_string(),
            )
        } else {
            None
        };

        let exp_raw = i64::from_le_bytes(take(&mut cursor, 8)?.try_into().unwrap());
        let exp_present = take(&mut cursor, 1)?[0];
        let expires_at = if exp_present == 1 { Some(exp_raw) } else { None };

        Ok(Self { kind, primary, secondary, expires_at })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_redacts_secret_string() {
        let s: Secret<String> = Secret::new("ya29.a0AfH-PRIVATE".to_string());
        let dbg = format!("{s:?}");
        assert!(dbg.contains("***"));
        assert!(!dbg.contains("ya29"));
        assert!(!dbg.contains("PRIVATE"));
    }

    #[test]
    fn expose_returns_inner_reference() {
        let s = Secret::new(String::from("hello"));
        assert_eq!(s.expose(), "hello");
    }

    #[test]
    fn into_inner_returns_value_and_default_replaces_secret() {
        let s = Secret::new(String::from("payload"));
        let inner = s.into_inner();
        assert_eq!(inner, "payload");
    }

    #[test]
    fn debug_redacts_source_credential() {
        let c = SourceCredential {
            kind: "google_drive".into(),
            primary: "ya29.aSecretToken".into(),
            secondary: Some("1//refreshToken".into()),
            expires_at: Some(1_700_000_000),
        };
        let dbg = format!("{c:?}");
        assert!(dbg.contains("google_drive"));
        assert!(dbg.contains("***"));
        assert!(!dbg.contains("aSecretToken"));
        assert!(!dbg.contains("refreshToken"));
    }

    #[test]
    fn source_credential_round_trip() {
        let c = SourceCredential {
            kind: "google_drive".into(),
            primary: "access".into(),
            secondary: Some("refresh".into()),
            expires_at: Some(123_456_789),
        };
        let bytes = c.to_plaintext();
        let back = SourceCredential::from_plaintext(&bytes).unwrap();
        assert_eq!(back.kind, c.kind);
        assert_eq!(back.primary, c.primary);
        assert_eq!(back.secondary, c.secondary);
        assert_eq!(back.expires_at, c.expires_at);
    }

    #[test]
    fn source_credential_round_trip_no_secondary() {
        let c = SourceCredential {
            kind: "smb".into(),
            primary: "user".into(),
            secondary: None,
            expires_at: None,
        };
        let bytes = c.to_plaintext();
        let back = SourceCredential::from_plaintext(&bytes).unwrap();
        assert_eq!(back.kind, "smb");
        assert!(back.secondary.is_none());
        assert!(back.expires_at.is_none());
    }
}
