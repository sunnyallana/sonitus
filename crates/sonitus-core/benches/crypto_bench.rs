//! Microbenchmarks for the crypto layer.

use criterion::{Criterion, criterion_group, criterion_main, BenchmarkId, Throughput};
use sonitus_core::crypto::{VaultKey, encrypt_field, decrypt_field};
use sonitus_core::crypto::kdf::SALT_LEN;

fn bench_encrypt(c: &mut Criterion) {
    let salt = [0u8; SALT_LEN];
    let key = VaultKey::derive("benchmark", &salt).unwrap();
    let mut group = c.benchmark_group("encrypt_field");
    for size in [64usize, 1024, 65_536] {
        let plaintext = vec![0u8; size];
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &plaintext, |b, pt| {
            b.iter(|| encrypt_field(&key, pt).unwrap());
        });
    }
    group.finish();
}

fn bench_decrypt(c: &mut Criterion) {
    let salt = [0u8; SALT_LEN];
    let key = VaultKey::derive("benchmark", &salt).unwrap();
    let mut group = c.benchmark_group("decrypt_field");
    for size in [64usize, 1024, 65_536] {
        let plaintext = vec![0u8; size];
        let ciphertext = encrypt_field(&key, &plaintext).unwrap();
        group.throughput(Throughput::Bytes(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &ciphertext, |b, ct| {
            b.iter(|| decrypt_field(&key, ct).unwrap());
        });
    }
    group.finish();
}

criterion_group!(benches, bench_encrypt, bench_decrypt);
criterion_main!(benches);
