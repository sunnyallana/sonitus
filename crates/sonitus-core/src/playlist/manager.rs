//! High-level playlist operations layered over `library::queries::playlists`.

use crate::error::Result;
use crate::library::{Playlist, Track, queries};
use sqlx::SqlitePool;

/// Playlist manager — owns a pool, exposes ergonomic operations.
#[derive(Clone)]
pub struct PlaylistManager {
    pool: SqlitePool,
}

/// Options for M3U8 export.
#[derive(Debug, Clone, Copy)]
pub struct M3uExportOptions {
    /// Use full source-relative paths if true; just filenames otherwise.
    pub include_source_paths: bool,
    /// Emit `#EXTINF` track-info lines (recommended).
    pub include_extinf: bool,
}

impl Default for M3uExportOptions {
    fn default() -> Self {
        Self {
            include_source_paths: true,
            include_extinf: true,
        }
    }
}

impl PlaylistManager {
    /// Construct over a pool.
    pub fn new(pool: SqlitePool) -> Self { Self { pool } }

    /// Clone an existing playlist (manual or smart) under a new name.
    pub async fn clone_playlist(&self, src_id: &str, new_name: &str) -> Result<Playlist> {
        let src = queries::playlists::by_id(&self.pool, src_id).await?;
        let dest = if src.is_smart() {
            queries::playlists::create_smart(
                &self.pool,
                new_name,
                src.description.as_deref(),
                src.smart_rules.as_deref().unwrap_or("{}"),
            )
            .await?
        } else {
            let dest = queries::playlists::create_manual(
                &self.pool,
                new_name,
                src.description.as_deref(),
            )
            .await?;
            for t in queries::playlists::tracks_of(&self.pool, &src.id).await? {
                queries::playlists::append_track(&self.pool, &dest.id, &t.id).await?;
            }
            dest
        };
        Ok(dest)
    }

    /// Export a playlist as M3U8 text.
    pub async fn export_m3u8(&self, playlist_id: &str, opts: M3uExportOptions) -> Result<String> {
        let tracks = queries::playlists::tracks_of(&self.pool, playlist_id).await?;
        let mut out = String::from("#EXTM3U\n");
        for t in tracks {
            if opts.include_extinf {
                let secs = (t.duration_ms.unwrap_or(0) / 1000).max(0);
                let title = &t.title;
                let artist_name = match &t.artist_id {
                    Some(aid) => queries::artists::by_id(&self.pool, aid)
                        .await
                        .map(|a| a.name)
                        .unwrap_or_default(),
                    None => String::new(),
                };
                let by = if !artist_name.is_empty() { format!("{artist_name} - ") } else { String::new() };
                out.push_str(&format!("#EXTINF:{secs},{by}{title}\n"));
            }
            let path_text = if opts.include_source_paths {
                format!("{}#{}", t.source_id, t.remote_path)
            } else {
                t.remote_path.clone()
            };
            out.push_str(&path_text);
            out.push('\n');
        }
        Ok(out)
    }

    /// Import an M3U8 file. Tracks are matched against the library by
    /// `source_id#remote_path`; tracks not in the library are skipped.
    pub async fn import_m3u8(&self, name: &str, m3u: &str) -> Result<Playlist> {
        let p = queries::playlists::create_manual(&self.pool, name, None).await?;
        for line in m3u.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') { continue; }
            // Expect "source_id#path" — fall back to literal path match
            // against any source if no `#`.
            let track = if let Some((src, path)) = line.split_once('#') {
                queries::tracks::by_source_path(&self.pool, src, path).await?
            } else {
                // Lookup any track whose remote_path matches.
                let row: Option<Track> = sqlx::query_as::<_, Track>(
                    "SELECT * FROM tracks WHERE remote_path = ? LIMIT 1",
                )
                .bind(line)
                .fetch_optional(&self.pool)
                .await?;
                row
            };
            if let Some(t) = track {
                queries::playlists::append_track(&self.pool, &p.id, &t.id).await?;
            }
        }
        Ok(p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::VaultDb;

    #[tokio::test]
    async fn export_empty_playlist_yields_just_extm3u_header() {
        let db = VaultDb::open_in_memory().await.unwrap();
        let mgr = PlaylistManager::new(db.pool().clone());
        let p = queries::playlists::create_manual(db.pool(), "Empty", None).await.unwrap();
        let text = mgr.export_m3u8(&p.id, M3uExportOptions::default()).await.unwrap();
        assert_eq!(text.trim(), "#EXTM3U");
    }
}
