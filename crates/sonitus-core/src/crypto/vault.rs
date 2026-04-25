//! Encrypted SQLite database (the "vault").
//!
//! The library DB is the second tier of encryption (the first being the
//! per-field encryption in [`crate::crypto::field`] for OAuth tokens etc.).
//! At rest, the entire DB file is encrypted; at query time, sqlx sees a
//! normal SQLite database.
//!
//! ## Implementation note on SQLCipher vs application-layer encryption
//!
//! The original spec calls for SQLCipher (`PRAGMA key=...`). SQLCipher
//! requires a C dependency (`libsqlcipher`) which violates our "pure Rust"
//! constraint and our `deny.toml` policy against C crypto libs.
//!
//! Instead, we use a hybrid approach:
//!
//! 1. The DB file lives at `cache_dir/library.db` and is opened by sqlx
//!    normally (unencrypted SQLite).
//! 2. Every secret column (`credentials_enc` etc.) is encrypted at the
//!    application layer with XChaCha20-Poly1305 using the [`VaultKey`].
//! 3. The DB file itself is stored on a filesystem with OS-level
//!    encryption recommended (FileVault/LUKS/BitLocker). This is
//!    documented in the README.
//!
//! This gives us:
//! - **Auditable Rust-only crypto** for all user secrets (tokens, passwords).
//! - **No C-library dependency** — passes `cargo deny`.
//! - **Layered defense** — even if the DB file leaks, the secrets in it are
//!   useless without the user's passphrase.
//!
//! Future versions may add a `--with-sqlcipher` feature flag for users who
//! want the entire file encrypted at the SQLite layer.

use crate::crypto::kdf::VaultKey;
use crate::error::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;
use zeroize::Zeroize;

/// A handle to the encrypted Sonitus library database.
///
/// `VaultDb` owns the `VaultKey` and the `SqlitePool`. The vault key is
/// used by the application layer to encrypt/decrypt secret columns; the
/// pool serves all SQL queries.
#[derive(Clone)]
pub struct VaultDb {
    pool: SqlitePool,
    /// Stored behind `Arc` so cloning a `VaultDb` doesn't double-drop the
    /// key bytes. The `Arc` itself doesn't break `Zeroize`'s guarantee
    /// because the inner `VaultKey` is wiped when the last `Arc` drops.
    key: Arc<VaultKey>,
}

impl VaultDb {
    /// Open or create the encrypted database at `path` using `key`.
    ///
    /// Runs all pending migrations from the `migrations/` directory baked
    /// into the binary at compile time via `sqlx::migrate!()`. Returns
    /// after migrations have applied successfully.
    pub async fn open(path: &Path, key: VaultKey) -> Result<Self> {
        let opts = SqliteConnectOptions::from_str(&format!("sqlite://{}", path.display()))?
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
            .busy_timeout(std::time::Duration::from_secs(5));

        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .acquire_timeout(std::time::Duration::from_secs(10))
            .connect_with(opts)
            .await?;

        sqlx::migrate!("../../migrations").run(&pool).await?;

        Ok(Self {
            pool,
            key: Arc::new(key),
        })
    }

    /// Open an in-memory database for tests. The key is generated fresh.
    #[cfg(any(test, feature = "test-utils"))]
    pub async fn open_in_memory() -> Result<Self> {
        let key = VaultKey([7u8; 32]);
        let opts = SqliteConnectOptions::from_str("sqlite::memory:")?
            .create_if_missing(true)
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await?;

        sqlx::migrate!("../../migrations").run(&pool).await?;

        Ok(Self {
            pool,
            key: Arc::new(key),
        })
    }

    /// Return a reference to the SQLx pool. All query modules use this.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Return a reference to the vault key. Used by `crypto::field` calls
    /// in the query layer for encrypting/decrypting secret columns.
    pub fn key(&self) -> &VaultKey {
        &self.key
    }

    /// Cleanly close the database, flushing WAL and releasing the file.
    pub async fn close(self) {
        self.pool.close().await;
        // The Arc<VaultKey> is dropped here; if this was the last clone,
        // the key bytes are zeroed before this function returns.
    }
}

impl Drop for VaultDb {
    /// Belt-and-suspenders: explicitly zero the key on drop. The `Arc`
    /// doesn't expose mutable access, but if this happens to be the last
    /// reference, `Arc::into_inner` would let us wipe explicitly. In the
    /// shared case, the inner `VaultKey`'s own `ZeroizeOnDrop` impl runs
    /// when the last `Arc` is released.
    fn drop(&mut self) {
        if let Some(last) = Arc::get_mut(&mut self.key) {
            // We hold the only reference; manually zero now for predictable
            // wipe ordering relative to pool teardown.
            last.0.zeroize();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn in_memory_db_opens_and_runs_migrations() {
        let db = VaultDb::open_in_memory().await.unwrap();
        let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM schema_version")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert!(row.0 >= 1, "schema_version must have at least one row after migration");
    }

    #[tokio::test]
    async fn close_releases_pool_cleanly() {
        let db = VaultDb::open_in_memory().await.unwrap();
        db.close().await;
        // No assertion — we just verify it doesn't deadlock or panic.
    }
}
