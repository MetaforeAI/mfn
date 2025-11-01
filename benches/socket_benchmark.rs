//! Socket Communication Benchmarks
//!
//! Benchmarks for the unified socket communication system,
//! measuring serialization, deserialization, and end-to-end performance.

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use bytes::Bytes;
use mfn_telepathy::socket::{
    SocketMessage, MessageType, SocketProtocol,
};
use std::time::Duration;

fn benchmark_serialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("serialization");

    // Test different payload sizes
    let payload_sizes = vec![64, 256, 1024, 4096, 16384, 65536];

    for size in payload_sizes {
        let payload = Bytes::from(vec![42u8; size]);
        let message = SocketMessage::new(MessageType::MemoryAdd, 12345, payload.clone());

        group.bench_with_input(
            BenchmarkId::new("binary_protocol", size),
            &message,
            |b, msg| {
                b.iter(|| {
                    let serialized = msg.to_bytes(false).unwrap();
                    black_box(serialized);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("binary_protocol_compressed", size),
            &message,
            |b, msg| {
                b.iter(|| {
                    let serialized = msg.to_bytes(true).unwrap();
                    black_box(serialized);
                });
            },
        );
    }

    group.finish();
}

fn benchmark_deserialization(c: &mut Criterion) {
    let mut group = c.benchmark_group("deserialization");

    let payload_sizes = vec![64, 256, 1024, 4096, 16384, 65536];

    for size in payload_sizes {
        let payload = Bytes::from(vec![42u8; size]);
        let message = SocketMessage::new(MessageType::MemoryAdd, 12345, payload);

        // Serialize once for deserialization benchmark
        let serialized = message.to_bytes(false).unwrap();
        let serialized_compressed = message.to_bytes(true).unwrap();

        group.bench_with_input(
            BenchmarkId::new("binary_protocol", size),
            &serialized,
            |b, data| {
                b.iter(|| {
                    let deserialized = SocketMessage::from_bytes(data).unwrap();
                    black_box(deserialized);
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("binary_protocol_compressed", size),
            &serialized_compressed,
            |b, data| {
                b.iter(|| {
                    let deserialized = SocketMessage::from_bytes(data).unwrap();
                    black_box(deserialized);
                });
            },
        );
    }

    group.finish();
}

fn benchmark_round_trip(c: &mut Criterion) {
    let mut group = c.benchmark_group("round_trip");

    let payload_sizes = vec![64, 1024, 16384];

    for size in payload_sizes {
        let payload = Bytes::from(vec![42u8; size]);
        let message = SocketMessage::new(MessageType::SearchSimilarity, 12345, payload);

        group.bench_with_input(
            BenchmarkId::new("full_cycle", size),
            &message,
            |b, msg| {
                b.iter(|| {
                    // Serialize
                    let serialized = msg.to_bytes(true).unwrap();
                    // Deserialize
                    let deserialized = SocketMessage::from_bytes(&serialized).unwrap();
                    black_box(deserialized);
                });
            },
        );
    }

    group.finish();
}

fn benchmark_message_creation(c: &mut Criterion) {
    c.bench_function("create_small_message", |b| {
        b.iter(|| {
            let msg = SocketMessage::new(
                MessageType::Ping,
                12345,
                Bytes::from("ping"),
            );
            black_box(msg);
        });
    });

    c.bench_function("create_medium_message", |b| {
        let payload = Bytes::from(vec![0u8; 1024]);
        b.iter(|| {
            let msg = SocketMessage::new(
                MessageType::MemoryAdd,
                12345,
                payload.clone(),
            );
            black_box(msg);
        });
    });

    c.bench_function("create_large_message", |b| {
        let payload = Bytes::from(vec![0u8; 65536]);
        b.iter(|| {
            let msg = SocketMessage::new(
                MessageType::BatchRequest,
                12345,
                payload.clone(),
            );
            black_box(msg);
        });
    });
}

fn benchmark_crc32(c: &mut Criterion) {
    let mut group = c.benchmark_group("crc32");

    let data_sizes = vec![64, 256, 1024, 4096, 16384];

    for size in data_sizes {
        let data = vec![42u8; size];

        group.bench_with_input(
            BenchmarkId::new("calculate", size),
            &data,
            |b, d| {
                b.iter(|| {
                    // CRC32 calculation (internal function would be called here)
                    let mut crc = 0xFFFFFFFFu32;
                    for &byte in d.iter() {
                        crc ^= byte as u32;
                        for _ in 0..8 {
                            crc = if crc & 1 != 0 {
                                (crc >> 1) ^ 0xEDB88320
                            } else {
                                crc >> 1
                            };
                        }
                    }
                    black_box(!crc);
                });
            },
        );
    }

    group.finish();
}

fn benchmark_compression(c: &mut Criterion) {
    let mut group = c.benchmark_group("lz4_compression");

    // Test with different data patterns
    let test_cases = vec![
        ("repetitive", vec![42u8; 4096]),
        ("random", {
            use rand::Rng;
            let mut rng = rand::thread_rng();
            (0..4096).map(|_| rng.gen()).collect()
        }),
        ("mixed", {
            let mut data = vec![0u8; 4096];
            for (i, byte) in data.iter_mut().enumerate() {
                *byte = if i % 16 < 8 { 42 } else { (i % 256) as u8 };
            }
            data
        }),
    ];

    for (name, data) in test_cases {
        group.bench_with_input(
            BenchmarkId::new("compress", name),
            &data,
            |b, d| {
                b.iter(|| {
                    let compressed = lz4_flex::compress_prepend_size(d);
                    black_box(compressed);
                });
            },
        );

        let compressed = lz4_flex::compress_prepend_size(&data);
        group.bench_with_input(
            BenchmarkId::new("decompress", name),
            &compressed,
            |b, c| {
                b.iter(|| {
                    let decompressed = lz4_flex::decompress_size_prepended(c).unwrap();
                    black_box(decompressed);
                });
            },
        );
    }

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .measurement_time(Duration::from_secs(10))
        .sample_size(100);
    targets = benchmark_serialization,
              benchmark_deserialization,
              benchmark_round_trip,
              benchmark_message_creation,
              benchmark_crc32,
              benchmark_compression
}

criterion_main!(benches);