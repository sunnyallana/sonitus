//! Integration test: build a playlist, export it as M3U8, parse the
//! result, confirm the round trip.

use sonitus_core::crypto::VaultDb;
use sonitus_core::library::{Track, queries};
use sonitus_core::playlist::manager::{M3uExportOptions, PlaylistManager};

fn fresh_track(id: &str, source: &str, path: &str, title: &str, dur_ms: i64) -> Track {
    Track {
        id: id.into(),
        title: title.into(),
        artist_id: None,
        album_artist_id: None,
        album_id: None,
        source_id: source.into(),
        remote_path: path.into(),
        local_cache_path: None,
        duration_ms: Some(dur_ms),
        track_number: None,
        disc_number: 1,
        genre: None,
        year: None,
        bpm: None,
        replay_gain_track: None,
        replay_gain_album: None,
        file_size_bytes: None,
        format: Some("mp3".into()),
        bitrate_kbps: None,
        sample_rate_hz: None,
        bit_depth: None,
        channels: None,
        content_hash: None,
        musicbrainz_id: None,
        play_count: 0,
        last_played_at: None,
        rating: None,
        loved: 0,
        created_at: 0,
        updated_at: 0,
    }
}

#[tokio::test]
async fn m3u_export_includes_extm3u_header_and_extinf_lines() {
    let db = VaultDb::open_in_memory().await.unwrap();
    sqlx::query("INSERT INTO sources (id, name, kind, config_json) VALUES ('src_a', 'L', 'local', '{}')")
        .execute(db.pool()).await.unwrap();

    let t1 = fresh_track("t1", "src_a", "/a.mp3", "Song A", 180_000);
    let t2 = fresh_track("t2", "src_a", "/b.mp3", "Song B", 240_500);
    queries::tracks::upsert(db.pool(), &t1).await.unwrap();
    queries::tracks::upsert(db.pool(), &t2).await.unwrap();

    let pl = queries::playlists::create_manual(db.pool(), "Mix", None).await.unwrap();
    queries::playlists::append_track(db.pool(), &pl.id, "t1").await.unwrap();
    queries::playlists::append_track(db.pool(), &pl.id, "t2").await.unwrap();

    let mgr = PlaylistManager::new(db.pool().clone());
    let m3u = mgr.export_m3u8(&pl.id, M3uExportOptions::default()).await.unwrap();

    assert!(m3u.starts_with("#EXTM3U"));
    assert!(m3u.contains("#EXTINF:180,"));
    assert!(m3u.contains("Song A"));
    assert!(m3u.contains("#EXTINF:240,"));
    assert!(m3u.contains("Song B"));
    // Source-relative path format is "source_id#path".
    assert!(m3u.contains("src_a#/a.mp3"));
}

#[tokio::test]
async fn m3u_import_round_trips_through_manager() {
    let db = VaultDb::open_in_memory().await.unwrap();
    sqlx::query("INSERT INTO sources (id, name, kind, config_json) VALUES ('src_a', 'L', 'local', '{}')")
        .execute(db.pool()).await.unwrap();

    let t1 = fresh_track("t1", "src_a", "/a.mp3", "Song A", 100_000);
    let t2 = fresh_track("t2", "src_a", "/b.mp3", "Song B", 100_000);
    queries::tracks::upsert(db.pool(), &t1).await.unwrap();
    queries::tracks::upsert(db.pool(), &t2).await.unwrap();

    let mgr = PlaylistManager::new(db.pool().clone());
    let m3u = "\
#EXTM3U
#EXTINF:100,Song A
src_a#/a.mp3
#EXTINF:100,Song B
src_a#/b.mp3
";
    let imported = mgr.import_m3u8("Imported", m3u).await.unwrap();
    let tracks = queries::playlists::tracks_of(db.pool(), &imported.id).await.unwrap();
    let titles: Vec<_> = tracks.iter().map(|t| t.title.clone()).collect();
    assert_eq!(titles, vec!["Song A".to_string(), "Song B".to_string()]);
}

#[tokio::test]
async fn m3u_import_skips_unknown_tracks() {
    let db = VaultDb::open_in_memory().await.unwrap();
    sqlx::query("INSERT INTO sources (id, name, kind, config_json) VALUES ('src_a', 'L', 'local', '{}')")
        .execute(db.pool()).await.unwrap();

    let t1 = fresh_track("t1", "src_a", "/a.mp3", "Song A", 100_000);
    queries::tracks::upsert(db.pool(), &t1).await.unwrap();

    let mgr = PlaylistManager::new(db.pool().clone());
    let m3u = "\
#EXTM3U
src_a#/a.mp3
src_a#/missing.mp3
";
    let imported = mgr.import_m3u8("Partial", m3u).await.unwrap();
    let tracks = queries::playlists::tracks_of(db.pool(), &imported.id).await.unwrap();
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[0].title, "Song A");
}
