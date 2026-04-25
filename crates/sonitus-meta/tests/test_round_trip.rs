//! End-to-end round trip of `LibraryMeta` through disk.

use sonitus_meta::{schema::*, load, save, validate, CURRENT_SCHEMA_VERSION};
use tempfile::TempDir;

#[test]
fn save_then_load_preserves_all_fields() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("library.sonitus");

    let mut meta = LibraryMeta::default();
    meta.meta.device_name = Some("My Laptop".into());
    meta.privacy.metadata_lookups_enabled = true;
    meta.audio.crossfade_seconds = 2.5;
    meta.appearance.theme = "light".into();
    meta.storage.cache_max_gb = 50;

    meta.sources.push(SourceDef {
        id: "src_001".into(),
        name: "Local".into(),
        kind: "local".into(),
        enabled: true,
        path: Some("/Music".into()),
        root_folder: None,
        bucket: None,
        region: None,
        endpoint_url: None,
        host: None,
        share: None,
        base_path: None,
        url: None,
        tenant: None,
    });

    let now = chrono::Utc::now();
    meta.playlists.push(PlaylistDef {
        id: "pl_001".into(),
        name: "Late Night Drives".into(),
        description: Some("For the 2am commute".into()),
        created_at: now,
        updated_at: now,
        is_smart: false,
        smart_rules: None,
        track_refs: vec![TrackRef {
            source_id: "src_001".into(),
            path: "/Music/song.flac".into(),
        }],
    });

    save(&path, meta.clone()).unwrap();
    let back = load(&path).unwrap();

    assert_eq!(back.meta.device_name.as_deref(), Some("My Laptop"));
    assert!(back.privacy.metadata_lookups_enabled);
    assert_eq!(back.audio.crossfade_seconds, 2.5);
    assert_eq!(back.appearance.theme, "light");
    assert_eq!(back.storage.cache_max_gb, 50);
    assert_eq!(back.sources.len(), 1);
    assert_eq!(back.sources[0].path.as_deref(), Some("/Music"));
    assert_eq!(back.playlists.len(), 1);
    assert_eq!(back.playlists[0].track_refs[0].path, "/Music/song.flac");
    assert_eq!(back.meta.schema_version, CURRENT_SCHEMA_VERSION);
}

#[test]
fn validate_passes_for_default_meta() {
    validate(&LibraryMeta::default()).unwrap();
}

#[test]
fn validate_fails_when_telemetry_enabled() {
    let mut m = LibraryMeta::default();
    m.privacy.telemetry_enabled = true;
    assert!(validate(&m).is_err());
}
