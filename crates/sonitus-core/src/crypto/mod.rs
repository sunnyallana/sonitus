//! Cryptographic primitives for Sonitus.
//!
//! Everything that operates on user secrets — the vault key, OAuth tokens,
//! source passwords — flows through this module. The submodules are
//! deliberately small and auditable:
//!
//! - [`kdf`] — Argon2id password-based key derivation.
//! - [`field`] — Per-field XChaCha20-Poly1305 AEAD encryption.
//! - [`vault`] — SQLite vault initialization (raw key PRAGMA).
//! - [`types`] — `Secret<T>`, `OAuthToken`, `SourcePassword` — all `Zeroize`.
//!
//! ## Design choices
//!
//! - **XChaCha20-Poly1305** over AES-GCM as the primary AEAD. It's
//!   constant-time on every CPU, has no nonce-misuse footguns at our
//!   key-rotation cadence (192-bit random nonces), and the implementation
//!   has been audited by NCC Group.
//! - **Argon2id** with OWASP 2024 parameters (`m=64MiB, t=3, p=4`).
//!   Memory-hard means an attacker needs both lots of CPU *and* lots of RAM.
//! - **No native crypto bindings**. Pure Rust everywhere. We never link
//!   `openssl-sys`, `nettle`, or any other C library — the `deny.toml`
//!   policy enforces this.
//! - **`Zeroize` everywhere**. Secrets are wiped from memory the moment
//!   they go out of scope. The compiler can't optimize this away because
//!   `zeroize::Zeroize` uses `core::ptr::write_volatile`.

pub mod field;
pub mod kdf;
pub mod types;
pub mod vault;

pub use field::{decrypt_field, encrypt_field};
pub use kdf::VaultKey;
pub use types::{OAuthToken, Secret, SourceCredential, SourcePassword};
pub use vault::VaultDb;
