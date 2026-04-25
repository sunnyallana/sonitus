//! Source queries — CRUD + encrypted credential read/write.

use crate::crypto::{VaultDb, encrypt_field, decrypt_field, types::SourceCredential};
use crate::error::{Result, SonitusError};
use crate::library::models::{ScanState, Source};
use sqlx::SqlitePool;

/// Insert a new source row. Credentials, if provided, are encrypted with
/// the vault key before being stored.
pub async fn insert(
    db: &VaultDb,
    id: &str,
    name: &str,
    kind: &str,
    config_json: &str,
    credentials: Option<&SourceCredential>,
) -> Result<Source> {
    let credentials_enc = match credentials {
        Some(c) => Some(encrypt_field(db.key(), &c.to_plaintext())?),
        None => None,
    };
    let now = chrono::Utc::now().timestamp();
    sqlx::query(
        r"INSERT INTO sources (
            id, name, kind, config_json, credentials_enc,
            scan_state, last_scanned_at, last_error,
            track_count, enabled, created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?,  'idle', NULL, NULL,  0, 1, ?, ?)",
    )
    .bind(id)
    .bind(name)
    .bind(kind)
    .bind(config_json)
    .bind(credentials_enc)
    .bind(now)
    .bind(now)
    .execute(db.pool())
    .await?;
    by_id(db.pool(), id).await
}

/// Fetch a source by ID.
pub async fn by_id(pool: &SqlitePool, id: &str) -> Result<Source> {
    sqlx::query_as::<_, Source>("SELECT * FROM sources WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| SonitusError::NotFound { kind: "source", id: id.to_string() })
}

/// All sources, sorted by name.
pub async fn list_all(pool: &SqlitePool) -> Result<Vec<Source>> {
    Ok(sqlx::query_as::<_, Source>("SELECT * FROM sources ORDER BY name")
        .fetch_all(pool)
        .await?)
}

/// All enabled sources, sorted by name.
pub async fn list_enabled(pool: &SqlitePool) -> Result<Vec<Source>> {
    Ok(
        sqlx::query_as::<_, Source>("SELECT * FROM sources WHERE enabled = 1 ORDER BY name")
            .fetch_all(pool)
            .await?,
    )
}

/// Decrypt the credentials of a source. Returns `None` if no credentials
/// are stored (e.g. local source with no auth).
pub async fn read_credentials(db: &VaultDb, source_id: &str) -> Result<Option<SourceCredential>> {
    let row: Option<(Option<Vec<u8>>,)> =
        sqlx::query_as("SELECT credentials_enc FROM sources WHERE id = ?")
            .bind(source_id)
            .fetch_optional(db.pool())
            .await?;
    let Some((blob_opt,)) = row else { return Ok(None); };
    let Some(blob) = blob_opt else { return Ok(None); };
    let plaintext = decrypt_field(db.key(), &blob)?;
    Ok(Some(SourceCredential::from_plaintext(&plaintext)?))
}

/// Update the encrypted credentials of an existing source.
pub async fn update_credentials(
    db: &VaultDb,
    source_id: &str,
    creds: &SourceCredential,
) -> Result<()> {
    let blob = encrypt_field(db.key(), &creds.to_plaintext())?;
    let now = chrono::Utc::now().timestamp();
    sqlx::query("UPDATE sources SET credentials_enc = ?, updated_at = ? WHERE id = ?")
        .bind(blob)
        .bind(now)
        .bind(source_id)
        .execute(db.pool())
        .await?;
    Ok(())
}

/// Update scan state and timestamps. Used by the scanner.
pub async fn set_scan_state(
    pool: &SqlitePool,
    source_id: &str,
    state: ScanState,
    error: Option<&str>,
) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    let last_scanned = if matches!(state, ScanState::Idle) { Some(now) } else { None };
    sqlx::query(
        r"UPDATE sources
            SET scan_state      = ?,
                last_scanned_at = COALESCE(?, last_scanned_at),
                last_error      = ?,
                updated_at      = ?
          WHERE id = ?",
    )
    .bind(state.to_string())
    .bind(last_scanned)
    .bind(error)
    .bind(now)
    .bind(source_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Refresh `track_count` from a fresh COUNT(*) over `tracks`.
pub async fn refresh_track_count(pool: &SqlitePool, source_id: &str) -> Result<i64> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tracks WHERE source_id = ?")
        .bind(source_id)
        .fetch_one(pool)
        .await?;
    let now = chrono::Utc::now().timestamp();
    sqlx::query("UPDATE sources SET track_count = ?, updated_at = ? WHERE id = ?")
        .bind(count.0)
        .bind(now)
        .bind(source_id)
        .execute(pool)
        .await?;
    Ok(count.0)
}

/// Toggle the `enabled` flag.
pub async fn set_enabled(pool: &SqlitePool, source_id: &str, enabled: bool) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    sqlx::query("UPDATE sources SET enabled = ?, updated_at = ? WHERE id = ?")
        .bind(if enabled { 1 } else { 0 })
        .bind(now)
        .bind(source_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Delete a source. Tracks belonging to the source are cascade-deleted.
pub async fn delete(pool: &SqlitePool, source_id: &str) -> Result<()> {
    sqlx::query("DELETE FROM sources WHERE id = ?")
        .bind(source_id)
        .execute(pool)
        .await?;
    Ok(())
}
