//! Integration test: the local source provider end-to-end against a temp dir.

use sonitus_core::sources::{SourceProvider, local::LocalSource};
use std::path::PathBuf;
use tempfile::TempDir;

fn make_dummy_audio(path: &std::path::Path) {
    // ID3 + a few bytes; just enough to be recognized as an audio file by extension.
    std::fs::write(path, b"ID3\x04\x00\x00\x00\x00\x00\x00stub").unwrap();
}

#[tokio::test]
async fn local_source_lists_and_streams() {
    let dir = TempDir::new().unwrap();
    make_dummy_audio(&dir.path().join("a.mp3"));
    make_dummy_audio(&dir.path().join("b.flac"));
    std::fs::write(dir.path().join("readme.txt"), b"not audio").unwrap();
    let nested = dir.path().join("nested");
    std::fs::create_dir(&nested).unwrap();
    make_dummy_audio(&nested.join("c.opus"));

    let src = LocalSource::new("s1", "Test", dir.path().to_path_buf());
    src.ping().await.unwrap();

    let files = src.list_files().await.unwrap();
    assert_eq!(files.len(), 3);

    let bytes = src.read_bytes("/a.mp3", 4).await.unwrap();
    assert_eq!(&bytes[..3], b"ID3");
}

#[tokio::test]
async fn local_source_download_writes_to_dest() {
    let src_dir = TempDir::new().unwrap();
    let dest_dir = TempDir::new().unwrap();
    let src_file = src_dir.path().join("track.mp3");
    std::fs::write(&src_file, b"audio bytes").unwrap();
    let dest = dest_dir.path().join("downloaded.mp3");

    let provider = LocalSource::new("s1", "Test", src_dir.path().to_path_buf());

    let (tx, mut rx) = tokio::sync::mpsc::channel(8);
    let download_task = tokio::spawn(async move {
        provider.download("/track.mp3", &PathBuf::from(dest), tx).await
    });

    // Drain progress while the download runs.
    while rx.recv().await.is_some() {}

    download_task.await.unwrap().unwrap();
    let downloaded = std::fs::read(dest_dir.path().join("downloaded.mp3")).unwrap();
    assert_eq!(downloaded, b"audio bytes");
}
