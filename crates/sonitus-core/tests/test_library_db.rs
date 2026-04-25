//! Integration test: full DB lifecycle on an in-memory vault.

use sonitus_core::crypto::VaultDb;
use sonitus_core::library::{Album, Artist, Track, queries};

#[tokio::test]
async fn migrations_apply_to_in_memory_db() {
    let db = VaultDb::open_in_memory().await.unwrap();
    let counts: (i64, i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM tracks),
            (SELECT COUNT(*) FROM albums),
            (SELECT COUNT(*) FROM artists)",
    )
    .fetch_one(db.pool())
    .await
    .unwrap();
    assert_eq!(counts, (0, 0, 0));
}

#[tokio::test]
async fn upsert_artist_album_track_then_search() {
    let db = VaultDb::open_in_memory().await.unwrap();

    // Insert source so the FK on tracks resolves.
    sqlx::query(
        "INSERT INTO sources (id, name, kind, config_json) VALUES ('s1', 'Test', 'local', '{}')",
    )
    .execute(db.pool())
    .await
    .unwrap();

    let sort_name = Artist::sort_name_for("Pink Floyd");
    let artist = Artist {
        id: Artist::derive_id(&sort_name),
        name: "Pink Floyd".into(),
        sort_name: sort_name.clone(),
        musicbrainz_id: None,
        bio: None,
        image_url: None,
        image_blob: None,
        play_count: 0,
        created_at: 0,
        updated_at: 0,
    };
    queries::artists::upsert(db.pool(), &artist).await.unwrap();

    let album = Album {
        id: Album::derive_id(Some(&artist.id), "Dark Side of the Moon"),
        title: "Dark Side of the Moon".into(),
        artist_id: Some(artist.id.clone()),
        year: Some(1973),
        genre: Some("Rock".into()),
        cover_art_blob: None,
        cover_art_url: None,
        cover_art_hash: None,
        musicbrainz_id: None,
        total_tracks: Some(10),
        disc_count: 1,
        play_count: 0,
        created_at: 0,
        updated_at: 0,
    };
    queries::albums::upsert(db.pool(), &album).await.unwrap();

    let track = Track {
        id: Track::derive_id("s1", "/dsotm/01.flac"),
        title: "Speak to Me".into(),
        artist_id: Some(artist.id.clone()),
        album_artist_id: Some(artist.id.clone()),
        album_id: Some(album.id.clone()),
        source_id: "s1".into(),
        remote_path: "/dsotm/01.flac".into(),
        local_cache_path: None,
        duration_ms: Some(67_000),
        track_number: Some(1),
        disc_number: 1,
        genre: Some("Rock".into()),
        year: Some(1973),
        bpm: None,
        replay_gain_track: None,
        replay_gain_album: None,
        file_size_bytes: Some(12_345_678),
        format: Some("flac".into()),
        bitrate_kbps: Some(800),
        sample_rate_hz: Some(44_100),
        bit_depth: Some(16),
        channels: Some(2),
        content_hash: None,
        musicbrainz_id: None,
        play_count: 0,
        last_played_at: None,
        rating: None,
        loved: 0,
        created_at: 0,
        updated_at: 0,
    };
    queries::tracks::upsert(db.pool(), &track).await.unwrap();

    let by_album = queries::tracks::by_album(db.pool(), &album.id).await.unwrap();
    assert_eq!(by_album.len(), 1);

    let results = sonitus_core::library::search(db.pool(), "speak", 10).await.unwrap();
    assert!(results.iter().any(|r| r.title.contains("Speak")));
}

#[tokio::test]
async fn playlist_membership_round_trips() {
    let db = VaultDb::open_in_memory().await.unwrap();
    sqlx::query(
        "INSERT INTO sources (id, name, kind, config_json) VALUES ('s1', 'Test', 'local', '{}')",
    )
    .execute(db.pool())
    .await
    .unwrap();

    // Add 3 tracks.
    for i in 0..3 {
        let t = Track {
            id: format!("t{i}"),
            title: format!("Track {i}"),
            artist_id: None,
            album_artist_id: None,
            album_id: None,
            source_id: "s1".into(),
            remote_path: format!("/{i}.mp3"),
            local_cache_path: None,
            duration_ms: Some(180_000),
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
        };
        queries::tracks::upsert(db.pool(), &t).await.unwrap();
    }

    let pl = queries::playlists::create_manual(db.pool(), "Mix", None).await.unwrap();
    queries::playlists::append_track(db.pool(), &pl.id, "t0").await.unwrap();
    queries::playlists::append_track(db.pool(), &pl.id, "t1").await.unwrap();
    queries::playlists::append_track(db.pool(), &pl.id, "t2").await.unwrap();

    let in_order = queries::playlists::tracks_of(db.pool(), &pl.id).await.unwrap();
    let ids: Vec<_> = in_order.iter().map(|t| t.id.clone()).collect();
    assert_eq!(ids, vec!["t0", "t1", "t2"]);

    queries::playlists::move_track(db.pool(), &pl.id, "t2", 0).await.unwrap();
    let after_move = queries::playlists::tracks_of(db.pool(), &pl.id).await.unwrap();
    let ids2: Vec<_> = after_move.iter().map(|t| t.id.clone()).collect();
    assert_eq!(ids2, vec!["t2", "t0", "t1"]);

    queries::playlists::remove_track(db.pool(), &pl.id, "t0").await.unwrap();
    let after_remove = queries::playlists::tracks_of(db.pool(), &pl.id).await.unwrap();
    let ids3: Vec<_> = after_remove.iter().map(|t| t.id.clone()).collect();
    assert_eq!(ids3, vec!["t2", "t1"]);
}
