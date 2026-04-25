//! Download queue queries — used by the download manager.

use crate::error::{Result, SonitusError};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use sqlx::SqlitePool;

/// One download queue entry.
#[derive(Debug, Clone, FromRow, Serialize, Deserialize)]
pub struct Download {
    /// UUID v4.
    pub id: String,
    /// Track being downloaded.
    pub track_id: String,
    /// Status: `queued | downloading | paused | done | failed | cancelled`.
    pub status: String,
    /// Progress 0.0..=1.0.
    pub progress: f64,
    /// Total bytes if Content-Length was reported, else NULL.
    pub bytes_total: Option<i64>,
    /// Bytes received so far.
    pub bytes_done: i64,
    /// Instantaneous speed estimate in bytes/sec.
    pub speed_bps: Option<i64>,
    /// Path on disk where the partial/complete file lives.
    pub local_path: Option<String>,
    /// Error message, if `status == "failed"`.
    pub error_msg: Option<String>,
    /// How many times the download has been retried.
    pub retry_count: i32,
    /// Unix epoch of enqueue.
    pub queued_at: i64,
    /// Unix epoch of first byte (NULL if not started).
    pub started_at: Option<i64>,
    /// Unix epoch of completion or failure.
    pub finished_at: Option<i64>,
}

/// Enqueue a fresh download for a track.
pub async fn enqueue(pool: &SqlitePool, track_id: &str, local_path: &str) -> Result<Download> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    sqlx::query(
        r"INSERT INTO downloads (
            id, track_id, status, progress, bytes_total, bytes_done,
            speed_bps, local_path, error_msg, retry_count,
            queued_at, started_at, finished_at
         ) VALUES (?, ?, 'queued', 0, NULL, 0,  NULL, ?, NULL, 0,  ?, NULL, NULL)",
    )
    .bind(&id)
    .bind(track_id)
    .bind(local_path)
    .bind(now)
    .execute(pool)
    .await?;
    by_id(pool, &id).await
}

/// Fetch by ID.
pub async fn by_id(pool: &SqlitePool, id: &str) -> Result<Download> {
    sqlx::query_as::<_, Download>("SELECT * FROM downloads WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| SonitusError::NotFound { kind: "download", id: id.to_string() })
}

/// All downloads, most recent first.
pub async fn list_all(pool: &SqlitePool) -> Result<Vec<Download>> {
    Ok(sqlx::query_as::<_, Download>(
        "SELECT * FROM downloads ORDER BY queued_at DESC",
    )
    .fetch_all(pool)
    .await?)
}

/// All currently-active (queued or downloading or paused) downloads.
pub async fn list_active(pool: &SqlitePool) -> Result<Vec<Download>> {
    Ok(sqlx::query_as::<_, Download>(
        "SELECT * FROM downloads
          WHERE status IN ('queued','downloading','paused')
          ORDER BY queued_at",
    )
    .fetch_all(pool)
    .await?)
}

/// Pop the oldest queued download and mark it as downloading. Returns
/// None if none are queued.
pub async fn claim_next_queued(pool: &SqlitePool) -> Result<Option<Download>> {
    let mut tx = pool.begin().await?;
    let row: Option<Download> = sqlx::query_as::<_, Download>(
        "SELECT * FROM downloads WHERE status = 'queued' ORDER BY queued_at LIMIT 1",
    )
    .fetch_optional(&mut *tx)
    .await?;
    if let Some(d) = &row {
        let now = chrono::Utc::now().timestamp();
        sqlx::query(
            "UPDATE downloads
                SET status = 'downloading', started_at = ?
              WHERE id = ?",
        )
        .bind(now)
        .bind(&d.id)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(row)
}

/// Update progress + speed.
pub async fn update_progress(
    pool: &SqlitePool,
    id: &str,
    bytes_done: i64,
    bytes_total: Option<i64>,
    speed_bps: Option<i64>,
) -> Result<()> {
    let progress = match bytes_total {
        Some(t) if t > 0 => (bytes_done as f64) / (t as f64),
        _ => 0.0,
    };
    sqlx::query(
        r"UPDATE downloads
            SET bytes_done  = ?,
                bytes_total = COALESCE(?, bytes_total),
                speed_bps   = ?,
                progress    = ?
          WHERE id = ?",
    )
    .bind(bytes_done)
    .bind(bytes_total)
    .bind(speed_bps)
    .bind(progress)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Mark a download as done.
pub async fn mark_done(pool: &SqlitePool, id: &str) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    sqlx::query(
        r"UPDATE downloads
            SET status = 'done', progress = 1.0, finished_at = ?
          WHERE id = ?",
    )
    .bind(now)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Mark a download as failed.
pub async fn mark_failed(pool: &SqlitePool, id: &str, error: &str) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    sqlx::query(
        r"UPDATE downloads
            SET status      = 'failed',
                error_msg   = ?,
                finished_at = ?,
                retry_count = retry_count + 1
          WHERE id = ?",
    )
    .bind(error)
    .bind(now)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Cancel a download.
pub async fn mark_cancelled(pool: &SqlitePool, id: &str) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    sqlx::query(
        r"UPDATE downloads SET status = 'cancelled', finished_at = ? WHERE id = ?",
    )
    .bind(now)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Pause a download (keeps partial bytes for later resume).
pub async fn mark_paused(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query("UPDATE downloads SET status = 'paused' WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Resume a paused download.
pub async fn mark_resumed(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query("UPDATE downloads SET status = 'queued' WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Delete completed/failed/cancelled downloads from the queue.
pub async fn purge_terminal(pool: &SqlitePool) -> Result<u64> {
    let r = sqlx::query("DELETE FROM downloads WHERE status IN ('done','failed','cancelled')")
        .execute(pool)
        .await?;
    Ok(r.rows_affected())
}
