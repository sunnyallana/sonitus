//! Artist queries — CRUD + bio update.

use crate::error::{Result, SonitusError};
use crate::library::models::Artist;
use sqlx::SqlitePool;

/// Insert or update an artist.
pub async fn upsert(pool: &SqlitePool, artist: &Artist) -> Result<Artist> {
    let now = chrono::Utc::now().timestamp();
    sqlx::query(
        r"INSERT INTO artists (
            id, name, sort_name, musicbrainz_id, bio, image_url, image_blob,
            play_count, created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?,  ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
            name           = excluded.name,
            sort_name      = excluded.sort_name,
            musicbrainz_id = COALESCE(excluded.musicbrainz_id, artists.musicbrainz_id),
            bio            = COALESCE(excluded.bio, artists.bio),
            image_url      = COALESCE(excluded.image_url, artists.image_url),
            image_blob     = COALESCE(excluded.image_blob, artists.image_blob),
            updated_at     = excluded.updated_at",
    )
    .bind(&artist.id)
    .bind(&artist.name)
    .bind(&artist.sort_name)
    .bind(&artist.musicbrainz_id)
    .bind(&artist.bio)
    .bind(&artist.image_url)
    .bind(&artist.image_blob)
    .bind(artist.play_count)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;
    by_id(pool, &artist.id).await
}

/// Fetch by ID.
pub async fn by_id(pool: &SqlitePool, id: &str) -> Result<Artist> {
    sqlx::query_as::<_, Artist>("SELECT * FROM artists WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| SonitusError::NotFound { kind: "artist", id: id.to_string() })
}

/// All artists, alphabetically by sort_name.
pub async fn list_all(pool: &SqlitePool, limit: i64, offset: i64) -> Result<Vec<Artist>> {
    Ok(sqlx::query_as::<_, Artist>(
        "SELECT * FROM artists ORDER BY sort_name LIMIT ? OFFSET ?",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?)
}

/// Update an artist's biography.
pub async fn set_bio(pool: &SqlitePool, id: &str, bio: &str) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    sqlx::query("UPDATE artists SET bio = ?, updated_at = ? WHERE id = ?")
        .bind(bio)
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Set or replace the artist photo.
pub async fn set_image(
    pool: &SqlitePool,
    id: &str,
    blob: Option<Vec<u8>>,
    url: Option<String>,
) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    sqlx::query(
        "UPDATE artists
            SET image_blob = ?, image_url = ?, updated_at = ?
          WHERE id = ?",
    )
    .bind(blob)
    .bind(url)
    .bind(now)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Delete an artist. Album/track FKs become NULL.
pub async fn delete(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM artists WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
