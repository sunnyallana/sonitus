//! Track queries — CRUD, FTS5 search, joined-view fetches.

use crate::error::{Result, SonitusError};
use crate::library::models::Track;
use sqlx::SqlitePool;

/// Insert or update a track row. Returns the resulting Track row.
pub async fn upsert(pool: &SqlitePool, track: &Track) -> Result<Track> {
    let now = chrono::Utc::now().timestamp();
    sqlx::query(
        r"INSERT INTO tracks (
            id, title, artist_id, album_artist_id, album_id, source_id,
            remote_path, local_cache_path, duration_ms, track_number, disc_number,
            genre, year, bpm, replay_gain_track, replay_gain_album,
            file_size_bytes, format, bitrate_kbps, sample_rate_hz, bit_depth,
            channels, content_hash, musicbrainz_id, play_count, last_played_at,
            rating, loved, created_at, updated_at
        ) VALUES (
            ?, ?, ?, ?, ?, ?,  ?, ?, ?, ?, ?,  ?, ?, ?, ?, ?,
            ?, ?, ?, ?, ?,  ?, ?, ?, ?, ?,  ?, ?, ?, ?
        )
        ON CONFLICT(id) DO UPDATE SET
            title             = excluded.title,
            artist_id         = excluded.artist_id,
            album_artist_id   = excluded.album_artist_id,
            album_id          = excluded.album_id,
            source_id         = excluded.source_id,
            remote_path       = excluded.remote_path,
            local_cache_path  = excluded.local_cache_path,
            duration_ms       = excluded.duration_ms,
            track_number      = excluded.track_number,
            disc_number       = excluded.disc_number,
            genre             = excluded.genre,
            year              = excluded.year,
            bpm               = excluded.bpm,
            replay_gain_track = excluded.replay_gain_track,
            replay_gain_album = excluded.replay_gain_album,
            file_size_bytes   = excluded.file_size_bytes,
            format            = excluded.format,
            bitrate_kbps      = excluded.bitrate_kbps,
            sample_rate_hz    = excluded.sample_rate_hz,
            bit_depth         = excluded.bit_depth,
            channels          = excluded.channels,
            content_hash      = excluded.content_hash,
            musicbrainz_id    = COALESCE(excluded.musicbrainz_id, tracks.musicbrainz_id),
            updated_at        = excluded.updated_at",
    )
    .bind(&track.id)
    .bind(&track.title)
    .bind(&track.artist_id)
    .bind(&track.album_artist_id)
    .bind(&track.album_id)
    .bind(&track.source_id)
    .bind(&track.remote_path)
    .bind(&track.local_cache_path)
    .bind(track.duration_ms)
    .bind(track.track_number)
    .bind(track.disc_number)
    .bind(&track.genre)
    .bind(track.year)
    .bind(track.bpm)
    .bind(track.replay_gain_track)
    .bind(track.replay_gain_album)
    .bind(track.file_size_bytes)
    .bind(&track.format)
    .bind(track.bitrate_kbps)
    .bind(track.sample_rate_hz)
    .bind(track.bit_depth)
    .bind(track.channels)
    .bind(&track.content_hash)
    .bind(&track.musicbrainz_id)
    .bind(track.play_count)
    .bind(track.last_played_at)
    .bind(track.rating)
    .bind(track.loved)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await?;

    by_id(pool, &track.id).await
}

/// Fetch a track by primary key.
pub async fn by_id(pool: &SqlitePool, id: &str) -> Result<Track> {
    sqlx::query_as::<_, Track>("SELECT * FROM tracks WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| SonitusError::NotFound { kind: "track", id: id.to_string() })
}

/// Fetch a track by source + path. Useful from the watcher when an FS
/// event fires for a known file.
pub async fn by_source_path(pool: &SqlitePool, source_id: &str, path: &str) -> Result<Option<Track>> {
    Ok(sqlx::query_as::<_, Track>(
        "SELECT * FROM tracks WHERE source_id = ? AND remote_path = ?",
    )
    .bind(source_id)
    .bind(path)
    .fetch_optional(pool)
    .await?)
}

/// All tracks on an album, ordered by disc + track number.
pub async fn by_album(pool: &SqlitePool, album_id: &str) -> Result<Vec<Track>> {
    Ok(sqlx::query_as::<_, Track>(
        "SELECT * FROM tracks
         WHERE album_id = ?
         ORDER BY disc_number, track_number, title",
    )
    .bind(album_id)
    .fetch_all(pool)
    .await?)
}

/// All tracks by an artist (as primary or album-artist), title-ordered.
pub async fn by_artist(pool: &SqlitePool, artist_id: &str) -> Result<Vec<Track>> {
    Ok(sqlx::query_as::<_, Track>(
        "SELECT * FROM tracks
         WHERE artist_id = ? OR album_artist_id = ?
         ORDER BY title",
    )
    .bind(artist_id)
    .bind(artist_id)
    .fetch_all(pool)
    .await?)
}

/// All tracks for a source. Used by the source detail page and rescan logic.
pub async fn by_source(pool: &SqlitePool, source_id: &str) -> Result<Vec<Track>> {
    Ok(sqlx::query_as::<_, Track>(
        "SELECT * FROM tracks WHERE source_id = ? ORDER BY remote_path",
    )
    .bind(source_id)
    .fetch_all(pool)
    .await?)
}

/// Recently-added tracks (created_at desc), capped at `limit`.
pub async fn recently_added(pool: &SqlitePool, limit: i64) -> Result<Vec<Track>> {
    Ok(sqlx::query_as::<_, Track>(
        "SELECT * FROM tracks ORDER BY created_at DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

/// Recently-played tracks. NULL last_played_at sorts last.
pub async fn recently_played(pool: &SqlitePool, limit: i64) -> Result<Vec<Track>> {
    Ok(sqlx::query_as::<_, Track>(
        "SELECT * FROM tracks
         WHERE last_played_at IS NOT NULL
         ORDER BY last_played_at DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

/// Most-played tracks (play_count desc).
pub async fn most_played(pool: &SqlitePool, limit: i64) -> Result<Vec<Track>> {
    Ok(sqlx::query_as::<_, Track>(
        "SELECT * FROM tracks WHERE play_count > 0 ORDER BY play_count DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

/// Mark a track as played: increments `play_count`, sets `last_played_at = now`.
pub async fn mark_played(pool: &SqlitePool, id: &str) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    sqlx::query(
        "UPDATE tracks
            SET play_count     = play_count + 1,
                last_played_at = ?,
                updated_at     = ?
          WHERE id = ?",
    )
    .bind(now)
    .bind(now)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// One track row enriched with the names of its artist + album, ready
/// for tabular display without N+1 lookups.
#[derive(Debug, Clone, PartialEq, sqlx::FromRow, serde::Serialize, serde::Deserialize)]
pub struct TrackView {
    /// Track ID.
    pub id: String,
    /// Track title.
    pub title: String,
    /// Joined artist name (NULL if track has no artist or it was deleted).
    pub artist_id: Option<String>,
    /// Joined artist display name.
    pub artist_name: Option<String>,
    /// Album ID, if any.
    pub album_id: Option<String>,
    /// Joined album title.
    pub album_title: Option<String>,
    /// Genre tag.
    pub genre: Option<String>,
    /// Release year.
    pub year: Option<i32>,
    /// Duration in milliseconds.
    pub duration_ms: Option<i64>,
    /// Created-at unix timestamp.
    pub created_at: i64,
}

/// Most-recently-added enriched track rows for the main Tracks list.
pub async fn recently_added_view(pool: &SqlitePool, limit: i64) -> Result<Vec<TrackView>> {
    Ok(sqlx::query_as::<_, TrackView>(
        r"SELECT
            t.id, t.title,
            t.artist_id,
            (SELECT name FROM artists WHERE id = t.artist_id) AS artist_name,
            t.album_id,
            (SELECT title FROM albums WHERE id = t.album_id) AS album_title,
            t.genre, t.year, t.duration_ms, t.created_at
          FROM tracks t
          ORDER BY t.created_at DESC
          LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?)
}

/// Update only the duration of a track. Used by the player engine to
/// backfill durations discovered at playback time (e.g. CBR mp3 packet
/// walks) so the tracks list shows the correct time without requiring
/// the user to play each track first.
pub async fn set_duration_ms(pool: &SqlitePool, id: &str, duration_ms: i64) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    sqlx::query("UPDATE tracks SET duration_ms = ?, updated_at = ? WHERE id = ?")
        .bind(duration_ms)
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Set the user rating for a track (0-5 or NULL to clear).
pub async fn set_rating(pool: &SqlitePool, id: &str, rating: Option<i32>) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    sqlx::query("UPDATE tracks SET rating = ?, updated_at = ? WHERE id = ?")
        .bind(rating)
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Toggle the "loved" flag.
pub async fn set_loved(pool: &SqlitePool, id: &str, loved: bool) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    sqlx::query("UPDATE tracks SET loved = ?, updated_at = ? WHERE id = ?")
        .bind(if loved { 1 } else { 0 })
        .bind(now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Delete a track. The FTS row is removed automatically by the trigger.
pub async fn delete(pool: &SqlitePool, id: &str) -> Result<()> {
    sqlx::query("DELETE FROM tracks WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// All tracks of a given genre, alpha-sorted.
pub async fn by_genre(pool: &SqlitePool, genre: &str) -> Result<Vec<Track>> {
    Ok(sqlx::query_as::<_, Track>(
        "SELECT * FROM tracks WHERE genre = ? ORDER BY title",
    )
    .bind(genre)
    .fetch_all(pool)
    .await?)
}

/// Distinct genre tags present in the library, sorted by track count desc.
pub async fn genres(pool: &SqlitePool) -> Result<Vec<(String, i64)>> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT genre, COUNT(*) AS n
           FROM tracks
          WHERE genre IS NOT NULL AND genre != ''
          GROUP BY genre
          ORDER BY n DESC, genre ASC",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
