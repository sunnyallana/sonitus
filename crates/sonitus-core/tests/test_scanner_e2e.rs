//! End-to-end test: write dummy audio files to a temp dir, run the
//! Scanner against a LocalSource, assert the library DB is populated.

use sonitus_core::crypto::VaultDb;
use sonitus_core::library::queries;
use sonitus_core::library::scanner::{ScanProgress, Scanner};
use sonitus_core::sources::local::LocalSource;
use sonitus_core::sources::SourceProvider;
use std::path::Path;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::sync::mpsc;

/// Write an ID3-tagged-but-mostly-empty MP3 stub. Symphonia probe will
/// fall through; the scanner falls back to filename-based metadata.
fn write_dummy_mp3(path: &Path, title: &str) {
    let mut bytes: Vec<u8> = Vec::new();
    // ID3v2 header followed by enough zeros to keep id3 reading happy.
    bytes.extend_from_slice(b"ID3\x04\x00\x00");
    bytes.extend_from_slice(&[0u8; 16]);
    // Title text marker (ad-hoc; not a real ID3 frame, just bytes).
    bytes.extend_from_slice(title.as_bytes());
    bytes.extend_from_slice(&[0u8; 256]);
    std::fs::write(path, bytes).unwrap();
}

#[tokio::test]
async fn scanner_populates_library_for_local_source() {
    // Build a tiny "Music/" tree.
    let dir = TempDir::new().unwrap();
    let music = dir.path().join("Music");
    std::fs::create_dir_all(&music).unwrap();
    let pf = music.join("Pink Floyd");
    std::fs::create_dir(&pf).unwrap();
    write_dummy_mp3(&pf.join("01 - Speak to Me.mp3"), "Speak to Me");
    write_dummy_mp3(&pf.join("02 - Breathe.mp3"), "Breathe");
    let unrelated = music.join("README.txt");
    std::fs::write(&unrelated, b"text").unwrap();

    // In-memory DB with migrations applied.
    let db = VaultDb::open_in_memory().await.unwrap();
    sqlx::query(
        "INSERT INTO sources (id, name, kind, config_json) VALUES ('s1', 'Test', 'local', '{}')",
    )
    .execute(db.pool())
    .await
    .unwrap();

    // Build the source + scanner.
    let source: Arc<dyn SourceProvider> = Arc::new(LocalSource::new("s1", "Test", music.clone()));
    let scanner = Scanner::new(source, db.pool().clone());

    let (tx, mut rx) = mpsc::channel::<ScanProgress>(32);
    let scan_handle = tokio::spawn(async move { scanner.run(tx).await });
    // Drain progress events.
    while rx.recv().await.is_some() {}

    let report = scan_handle.await.unwrap().unwrap();
    assert!(report.files_seen >= 2);
    assert!(report.tracks_added >= 2, "scanner should index both audio files; got {report:?}");

    // The text file should not have been counted.
    let track_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tracks WHERE source_id = ?")
        .bind("s1")
        .fetch_one(db.pool())
        .await
        .unwrap();
    assert_eq!(track_count.0, 2);

    // Re-running the scanner should be idempotent (no new tracks added).
    let source2: Arc<dyn SourceProvider> = Arc::new(LocalSource::new("s1", "Test", music));
    let scanner2 = Scanner::new(source2, db.pool().clone());
    let (tx2, mut rx2) = mpsc::channel::<ScanProgress>(32);
    let h2 = tokio::spawn(async move { scanner2.run(tx2).await });
    while rx2.recv().await.is_some() {}
    let report2 = h2.await.unwrap().unwrap();
    assert_eq!(report2.tracks_added, 0, "second scan should add no new tracks");
    assert_eq!(report2.tracks_removed, 0);
}

#[tokio::test]
async fn scanner_removes_tracks_for_deleted_files() {
    let dir = TempDir::new().unwrap();
    let song1 = dir.path().join("a.mp3");
    let song2 = dir.path().join("b.mp3");
    write_dummy_mp3(&song1, "A");
    write_dummy_mp3(&song2, "B");

    let db = VaultDb::open_in_memory().await.unwrap();
    sqlx::query("INSERT INTO sources (id, name, kind, config_json) VALUES ('s1', 'T', 'local', '{}')")
        .execute(db.pool()).await.unwrap();
    let source: Arc<dyn SourceProvider> = Arc::new(LocalSource::new("s1", "T", dir.path().to_path_buf()));
    let scanner = Scanner::new(source.clone(), db.pool().clone());
    let (tx, mut rx) = mpsc::channel(32);
    let _ = tokio::spawn(async move { while rx.recv().await.is_some() {} });
    scanner.run(tx).await.unwrap();

    let initial: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tracks").fetch_one(db.pool()).await.unwrap();
    assert_eq!(initial.0, 2);

    // Delete one file and rescan.
    std::fs::remove_file(&song2).unwrap();
    let scanner2 = Scanner::new(source, db.pool().clone());
    let (tx2, mut rx2) = mpsc::channel(32);
    let _ = tokio::spawn(async move { while rx2.recv().await.is_some() {} });
    let report = scanner2.run(tx2).await.unwrap();
    assert_eq!(report.tracks_removed, 1);
    let after: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tracks").fetch_one(db.pool()).await.unwrap();
    assert_eq!(after.0, 1);
}
