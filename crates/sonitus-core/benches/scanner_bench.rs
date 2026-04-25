//! Microbenchmarks for the local scanner over synthetic directory trees.

use criterion::{Criterion, criterion_group, criterion_main};
use sonitus_core::sources::{SourceProvider, local::LocalSource};
use std::path::PathBuf;
use tempfile::TempDir;

fn fresh_tree(file_count: usize) -> TempDir {
    let dir = TempDir::new().unwrap();
    for i in 0..file_count {
        let f = dir.path().join(format!("track-{i:04}.mp3"));
        std::fs::write(f, b"ID3\x04\x00\x00\x00\x00\x00\x00stub").unwrap();
    }
    dir
}

fn bench_list_files(c: &mut Criterion) {
    let dir = fresh_tree(1000);
    let path = dir.path().to_path_buf();

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    c.bench_function("local_list_files_1000", |b| {
        b.iter(|| {
            let path = PathBuf::from(&path);
            let src = LocalSource::new("s1", "bench", path);
            rt.block_on(src.list_files()).unwrap()
        });
    });
}

criterion_group!(benches, bench_list_files);
criterion_main!(benches);
