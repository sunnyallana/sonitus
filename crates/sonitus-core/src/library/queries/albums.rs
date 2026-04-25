//! Album queries — CRUD plus genre/year filters.

use crate::error::{Result, SonitusError};
use crate::library::models::Album;
use sqlx::SqlitePool;

/// Insert or update an album row.
pub async fn upsert(pool: &SqlitePool, album: &Album) -> Result<Album> {
    let now = chrono::Utc::now().timestamp();
    sqlx::query(
        r"INSERT INTO albums (
            id, title, artist_id, year, genre,
            cover_art_blob, cover_art_url, cover_art_hash,
            musicbrainz_id, total_tracks, disc_count, play_count,
            created_at, updated_at
         ) VALUES (?, ?, ?, ?, ?,  ?, ?, ?,  ?, ?, ?, ?,  ?, ?)
         ON CONFLICT(id) DO UPDATE SET
            title          = excluded.title,
            artist_id      = excluded.artist_id,
            year           = COALESCE(excluded.year, albums.year),
            genre          = COALESCE(excluded.genre, albums.genre),
            cover_art_blob = COALESCE(excluded.cover_art_blob, albums.cover_art_blob),
            cover_art_url  = COALESCE(excluded.cover_art_url, albums.cover_art_url),
            cover_art_hash = COALESCE(excluded.cover_art_hash, albums.cover_art_hash),
            musicbrainz_id = COALESCE(excluded.musicbrainz_id, albums.musicbrainz_id),
            total_tracks   = COALESCE(excluded.total_tracks, albums.total_tracks),
            disc_count     = MAX(excluded.disc_count, albums.disc_count),
            updated_at     = excluded.updated_at",
    )
    .bind(&album.id)
    .bind(&album.title)
    .bind(&album.artist_id)
    .bind(album.year)
    .bind(&album.genre)
    .bind(&album.cover_art_blob)
    .bind(&album.cover_art_url)
    .bind(&album.cover_art_hash)
    .bind(&album.musicbrainz_id)
    .bind(album.total_tracks)
    .bind(album.disc_count)
    .bind(album.play_count)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;
    by_id(pool, &album.id).await
}

/// Fetch an album by ID.
pub async fn by_id(pool: &SqlitePool, id: &str) -> Result<Album> {
    sqlx::query_as::<_, Album>("SELECT * FROM albums WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| SonitusError::NotFound { kind: "album", id: id.to_string() })
}

/// All albums by an artist, year-sorted.
pub async fn by_artist(pool: &SqlitePool, artist_id: &str) -> Result<Vec<Album>> {
    Ok(sqlx::query_as::<_, Album>(
        "SELECT * FROM albums WHERE artist_id = ? ORDER BY year, title",
    )
    .bind(artist_id)
    .fetch_all(pool)
    .await?)
}

/// All albums, optionally filtered by genre, sorted by title.
pub async fn list(pool: &SqlitePool, genre: Option<&str>, limit: i64, offset: i64) -> Result<Vec<Album>> {
    if let Some(g) = genre {
        Ok(sqlx::query_as::<_, Album>(
            "SELECT * FROM albums WHERE genre = ? ORDER BY title LIMIT ? OFFSET ?",
        )
        .bind(g)
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?)
    } else {
        Ok(sqlx::query_as::<_, Album>(
            "SELECT * FROM albums ORDER BY title LIMIT ? OFFSET ?",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(pool)
        .await?)
    }
}

/// Recently added albums (by created_at desc).
pub async fn recently_added(pool: &SqlitePool, limit: i64) -> Result<Vec<Album>> {
    Ok(sqlx::query_as::<_, Album>(
        "SELECT * FROM albums ORDER BY created_at DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

/// Set or replace the cover art for an album.
pub async fn set_cover_art(
    pool: &SqlitePool,
    album_id: &str,
    blob: Vec<u8>,
    hash: String,
) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    sqlx::query(
        "UPDATE albums
            SET cover_art_blob = ?,
                cover_art_hash = ?,
                updated_at     = ?
          WHERE id = ?",
    )
    .bind(blob)
    .bind(hash)
    .bind(now)
    .bind(album_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Delete an album. Tracks remain (FK is `ON DELETE SET NULL`).
pub async fn delete(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM albums WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
