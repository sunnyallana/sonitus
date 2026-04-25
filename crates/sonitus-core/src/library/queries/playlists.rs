//! Playlist queries — CRUD, membership, reorder.

use crate::error::{Result, SonitusError};
use crate::library::models::{Playlist, Track};
use sqlx::SqlitePool;

/// Create a new manual playlist with a fresh UUID. Returns the row.
pub async fn create_manual(pool: &SqlitePool, name: &str, description: Option<&str>) -> Result<Playlist> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    sqlx::query(
        r"INSERT INTO playlists (
            id, name, description, is_smart, smart_rules,
            track_count, total_duration_ms, created_at, updated_at
         ) VALUES (?, ?, ?, 0, NULL, 0, 0, ?, ?)",
    )
    .bind(&id)
    .bind(name)
    .bind(description)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;
    by_id(pool, &id).await
}

/// Create a smart playlist with serialized rules JSON.
pub async fn create_smart(
    pool: &SqlitePool,
    name: &str,
    description: Option<&str>,
    rules_json: &str,
) -> Result<Playlist> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    sqlx::query(
        r"INSERT INTO playlists (
            id, name, description, is_smart, smart_rules,
            track_count, total_duration_ms, created_at, updated_at
         ) VALUES (?, ?, ?, 1, ?, 0, 0, ?, ?)",
    )
    .bind(&id)
    .bind(name)
    .bind(description)
    .bind(rules_json)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;
    by_id(pool, &id).await
}

/// Fetch a playlist row by ID.
pub async fn by_id(pool: &SqlitePool, id: &str) -> Result<Playlist> {
    sqlx::query_as::<_, Playlist>("SELECT * FROM playlists WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| SonitusError::NotFound { kind: "playlist", id: id.to_string() })
}

/// All playlists, ordered alphabetically.
pub async fn list_all(pool: &SqlitePool) -> Result<Vec<Playlist>> {
    Ok(sqlx::query_as::<_, Playlist>("SELECT * FROM playlists ORDER BY name")
        .fetch_all(pool)
        .await?)
}

/// Rename a playlist.
pub async fn rename(pool: &SqlitePool, id: &str, new_name: &str) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    sqlx::query("UPDATE playlists SET name = ?, updated_at = ? WHERE id = ?")
        .bind(new_name)
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Update a playlist's description.
pub async fn set_description(pool: &SqlitePool, id: &str, description: Option<&str>) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    sqlx::query("UPDATE playlists SET description = ?, updated_at = ? WHERE id = ?")
        .bind(description)
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Delete a playlist (and its membership rows via FK cascade).
pub async fn delete(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM playlists WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

// ── Membership operations ────────────────────────────────────────────────

/// Append a track to the end of a manual playlist. Returns the new
/// position. No-op if the track is already in the playlist.
pub async fn append_track(pool: &SqlitePool, playlist_id: &str, track_id: &str) -> Result<i64> {
    let mut tx = pool.begin().await?;
    let max_pos: (Option<i64>,) = sqlx::query_as(
        "SELECT MAX(position) FROM playlist_tracks WHERE playlist_id = ?",
    )
    .bind(playlist_id)
    .fetch_one(&mut *tx)
    .await?;
    let pos = max_pos.0.unwrap_or(-1) + 1;
    let now = chrono::Utc::now().timestamp();

    sqlx::query(
        r"INSERT INTO playlist_tracks (playlist_id, track_id, position, added_at, added_by)
          VALUES (?, ?, ?, ?, 'user')
          ON CONFLICT (playlist_id, track_id) DO NOTHING",
    )
    .bind(playlist_id)
    .bind(track_id)
    .bind(pos)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    refresh_aggregates(&mut tx, playlist_id).await?;
    tx.commit().await?;
    Ok(pos)
}

/// Remove a track from a playlist. Closes the gap in `position`.
pub async fn remove_track(pool: &SqlitePool, playlist_id: &str, track_id: &str) -> Result<()> {
    let mut tx = pool.begin().await?;
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT position FROM playlist_tracks WHERE playlist_id = ? AND track_id = ?",
    )
    .bind(playlist_id)
    .bind(track_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((removed_pos,)) = row else {
        tx.rollback().await?;
        return Ok(());
    };

    sqlx::query("DELETE FROM playlist_tracks WHERE playlist_id = ? AND track_id = ?")
        .bind(playlist_id)
        .bind(track_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE playlist_tracks SET position = position - 1
          WHERE playlist_id = ? AND position > ?",
    )
    .bind(playlist_id)
    .bind(removed_pos)
    .execute(&mut *tx)
    .await?;

    refresh_aggregates(&mut tx, playlist_id).await?;
    tx.commit().await?;
    Ok(())
}

/// Move a track to a new position in the playlist. Other tracks shift to
/// fill the gap and to make room.
pub async fn move_track(
    pool: &SqlitePool,
    playlist_id: &str,
    track_id: &str,
    new_position: i64,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let row: Option<(i64,)> = sqlx::query_as(
        "SELECT position FROM playlist_tracks WHERE playlist_id = ? AND track_id = ?",
    )
    .bind(playlist_id)
    .bind(track_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((old_pos,)) = row else {
        tx.rollback().await?;
        return Err(SonitusError::NotFound {
            kind: "playlist_track",
            id: format!("{playlist_id}:{track_id}"),
        });
    };

    if new_position == old_pos {
        tx.rollback().await?;
        return Ok(());
    }

    if new_position > old_pos {
        // Shift down the items in (old_pos, new_position].
        sqlx::query(
            "UPDATE playlist_tracks
                SET position = position - 1
              WHERE playlist_id = ? AND position > ? AND position <= ?",
        )
        .bind(playlist_id)
        .bind(old_pos)
        .bind(new_position)
        .execute(&mut *tx)
        .await?;
    } else {
        // Shift up the items in [new_position, old_pos).
        sqlx::query(
            "UPDATE playlist_tracks
                SET position = position + 1
              WHERE playlist_id = ? AND position >= ? AND position < ?",
        )
        .bind(playlist_id)
        .bind(new_position)
        .bind(old_pos)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query(
        "UPDATE playlist_tracks SET position = ? WHERE playlist_id = ? AND track_id = ?",
    )
    .bind(new_position)
    .bind(playlist_id)
    .bind(track_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

/// All tracks in a playlist, ordered by `position`.
pub async fn tracks_of(pool: &SqlitePool, playlist_id: &str) -> Result<Vec<Track>> {
    Ok(sqlx::query_as::<_, Track>(
        "SELECT t.* FROM tracks t
           JOIN playlist_tracks pt ON pt.track_id = t.id
          WHERE pt.playlist_id = ?
          ORDER BY pt.position",
    )
    .bind(playlist_id)
    .fetch_all(pool)
    .await?)
}

/// Internal: refresh `track_count` and `total_duration_ms` denormalized columns.
async fn refresh_aggregates(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    playlist_id: &str,
) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    sqlx::query(
        r"UPDATE playlists SET
            track_count = (
                SELECT COUNT(*) FROM playlist_tracks WHERE playlist_id = playlists.id
            ),
            total_duration_ms = (
                SELECT COALESCE(SUM(t.duration_ms), 0)
                  FROM playlist_tracks pt
                  JOIN tracks t ON t.id = pt.track_id
                 WHERE pt.playlist_id = playlists.id
            ),
            updated_at = ?
         WHERE id = ?",
    )
    .bind(now)
    .bind(playlist_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
