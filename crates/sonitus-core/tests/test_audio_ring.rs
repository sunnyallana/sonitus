//! Integration test: producer/consumer behavior of the audio ringbuffer.

#![cfg(not(target_arch = "wasm32"))]

use sonitus_core::player::output_native::SampleSource;
use sonitus_core::player::AudioRing;
use std::thread;
use std::time::Duration;

#[test]
fn concurrent_producer_consumer_stays_in_sync() {
    let ring = AudioRing::new();
    ring.set_format(48_000, 2);

    // Producer pushes 10k samples in chunks of 100.
    let producer_ring = ring.clone();
    let producer = thread::spawn(move || {
        for i in 0..100u32 {
            let chunk: Vec<f32> = (0..100).map(|j| (i * 100 + j) as f32).collect();
            let mut written = 0;
            while written < chunk.len() {
                written += producer_ring.push(&chunk[written..]);
                if written < chunk.len() {
                    thread::sleep(Duration::from_micros(50));
                }
            }
        }
        producer_ring.mark_eof();
    });

    // Consumer drains in chunks of 64; total bytes should equal 10_000.
    let mut consumer_ring = ring.clone();
    let consumer = thread::spawn(move || {
        let mut total: usize = 0;
        let mut buf = [0.0f32; 64];
        loop {
            let n = consumer_ring.fill(&mut buf);
            total += n;
            if total >= 10_000 { break; }
            if n == 0 {
                thread::sleep(Duration::from_micros(100));
            }
        }
        total
    });

    producer.join().unwrap();
    let total = consumer.join().unwrap();
    assert_eq!(total, 10_000);
}

#[test]
fn frames_written_advances_with_pushes() {
    let ring = AudioRing::new();
    ring.set_format(48_000, 2);
    assert_eq!(ring.frames_written(), 0);
    ring.push(&[0.0, 0.0, 0.0, 0.0]); // 2 stereo frames
    assert_eq!(ring.frames_written(), 2);
    ring.push(&[0.0; 200]); // 100 stereo frames
    assert_eq!(ring.frames_written(), 102);
}

#[test]
fn clear_resets_frames_and_buffer() {
    let ring = AudioRing::new();
    ring.set_format(48_000, 2);
    ring.push(&[1.0, 2.0, 3.0, 4.0]);
    assert!(ring.buffered_samples() > 0);
    ring.clear();
    assert_eq!(ring.buffered_samples(), 0);
    assert_eq!(ring.frames_written(), 0);
}
