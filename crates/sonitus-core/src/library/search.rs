//! Full-text search over the library via SQLite FTS5.
//!
//! The `tracks_fts` virtual table is kept in sync with `tracks` by the
//! triggers in migration 003. This module is just the query layer.
//!
//! ## Query syntax
//!
//! We sanitize user input minimally:
//!
//! - Quote each token to disable FTS5 operator parsing (`AND`, `OR`, `NEAR`).
//! - Strip control characters.
//! - Append `*` to the last token to enable prefix matching ("led" matches
//!   "Led Zeppelin").

use crate::error::Result;
use crate::library::models::{Album, Artist};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use strum::Display;

/// What kind of object a search result references.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Display, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum SearchKind {
    /// A track row.
    Track,
    /// An album row.
    Album,
    /// An artist row.
    Artist,
}

/// One item in the search result list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchResult {
    /// What kind of result this is.
    pub kind: SearchKind,
    /// Primary key of the underlying row.
    pub id: String,
    /// Display title (track title / album title / artist name).
    pub title: String,
    /// Optional secondary line (artist for tracks/albums; nothing for artists).
    pub subtitle: Option<String>,
    /// FTS5 rank — lower is better. Used for ordering.
    pub rank: f64,
}

/// Full search across tracks (FTS5), albums (LIKE), and artists (LIKE).
/// Results are interleaved by relevance — tracks first, then albums, then artists.
pub async fn search(pool: &SqlitePool, query: &str, limit: i64) -> Result<Vec<SearchResult>> {
    let q = sanitize(query);
    if q.is_empty() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    out.extend(track_search(pool, &q, limit).await?);
    out.extend(album_search(pool, query, limit).await?);
    out.extend(artist_search(pool, query, limit).await?);
    out.sort_by(|a, b| a.rank.partial_cmp(&b.rank).unwrap_or(std::cmp::Ordering::Equal));
    out.truncate(limit as usize);
    Ok(out)
}

async fn track_search(pool: &SqlitePool, q: &str, limit: i64) -> Result<Vec<SearchResult>> {
    let rows: Vec<(String, String, Option<String>, Option<String>, f64)> = sqlx::query_as(
        r"SELECT t.id, t.title,
                 (SELECT name  FROM artists WHERE id = t.artist_id) AS artist_name,
                 (SELECT title FROM albums  WHERE id = t.album_id)  AS album_title,
                 bm25(tracks_fts) AS rank
            FROM tracks_fts
            JOIN tracks t ON t.id = tracks_fts.track_id
           WHERE tracks_fts MATCH ?
           ORDER BY rank
           LIMIT ?",
    )
    .bind(q)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, title, artist, album, rank)| SearchResult {
            kind: SearchKind::Track,
            id,
            title,
            subtitle: match (artist, album) {
                (Some(a), Some(b)) => Some(format!("{a} — {b}")),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                (None, None) => None,
            },
            rank,
        })
        .collect())
}

async fn album_search(pool: &SqlitePool, q: &str, limit: i64) -> Result<Vec<SearchResult>> {
    // Albums use LIKE rather than FTS — keep the index small.
    let needle = format!("%{q}%");
    let albums: Vec<Album> = sqlx::query_as(
        "SELECT * FROM albums
          WHERE title LIKE ? COLLATE NOCASE
          ORDER BY title LIMIT ?",
    )
    .bind(&needle)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    let mut results = Vec::new();
    for (i, album) in albums.into_iter().enumerate() {
        let artist_name = if let Some(aid) = &album.artist_id {
            let r: Option<(String,)> = sqlx::query_as("SELECT name FROM artists WHERE id = ?")
                .bind(aid)
                .fetch_optional(pool)
                .await?;
            r.map(|(n,)| n)
        } else { None };
        results.push(SearchResult {
            kind: SearchKind::Album,
            id: album.id,
            title: album.title,
            subtitle: artist_name,
            // Albums get a slight rank penalty so tracks lead.
            rank: 5.0 + (i as f64) * 0.01,
        });
    }
    Ok(results)
}

async fn artist_search(pool: &SqlitePool, q: &str, limit: i64) -> Result<Vec<SearchResult>> {
    let needle = format!("%{q}%");
    let artists: Vec<Artist> = sqlx::query_as(
        "SELECT * FROM artists
          WHERE name LIKE ? COLLATE NOCASE
             OR sort_name LIKE ? COLLATE NOCASE
          ORDER BY sort_name LIMIT ?",
    )
    .bind(&needle)
    .bind(&needle)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(artists
        .into_iter()
        .enumerate()
        .map(|(i, a)| SearchResult {
            kind: SearchKind::Artist,
            id: a.id,
            title: a.name,
            subtitle: None,
            rank: 10.0 + (i as f64) * 0.01,
        })
        .collect())
}

/// Sanitize a raw user query for FTS5: quote tokens, strip control chars,
/// add prefix-match suffix to the final token.
fn sanitize(input: &str) -> String {
    let cleaned: String = input
        .chars()
        .filter(|c| !c.is_control() && *c != '"')
        .collect();
    let tokens: Vec<&str> = cleaned.split_whitespace().collect();
    if tokens.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    for (i, t) in tokens.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        if i + 1 == tokens.len() {
            // Last token: prefix match.
            out.push('"');
            out.push_str(t);
            out.push_str("\"*");
        } else {
            out.push('"');
            out.push_str(t);
            out.push('"');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_quotes_each_token() {
        assert_eq!(sanitize("led zeppelin"), "\"led\" \"zeppelin\"*");
    }

    #[test]
    fn sanitize_handles_empty_input() {
        assert_eq!(sanitize(""), "");
        assert_eq!(sanitize("   "), "");
    }

    #[test]
    fn sanitize_strips_quotes_to_avoid_fts_injection() {
        // No raw quotes in the output other than our own wrappers.
        let s = sanitize("hello \"world\"");
        assert!(!s.contains("\"world\"*"));
        assert!(s.contains("\"hello\""));
    }

    #[test]
    fn sanitize_strips_control_chars() {
        let s = sanitize("foo\u{0007}bar");
        assert!(!s.contains('\u{0007}'));
    }
}
