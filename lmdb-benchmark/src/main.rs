use lmdb::{Environment, Database, WriteFlags, Transaction};
use std::collections::HashMap;
use std::time::Instant;
use std::path::Path;

const NUM_OPERATIONS: usize = 10_000;
const NUM_WARMUP: usize = 1_000;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🧪 LMDB Performance Benchmark");
    println!("{}", "=".repeat(60));
    println!("Testing {} operations (after {} warmup)", NUM_OPERATIONS, NUM_WARMUP);
    println!();

    // Create storage directory
    let db_path = "/tmp/lmdb_benchmark_test";
    std::fs::create_dir_all(db_path)?;

    // Clean up any existing database
    let _ = std::fs::remove_dir_all(db_path);
    std::fs::create_dir_all(db_path)?;

    // Initialize LMDB
    let env = Environment::new()
        .set_map_size(10_485_760) // 10MB
        .set_max_dbs(1)
        .open(Path::new(db_path))?;

    let db: Database = env.open_db(None)?;

    // Baseline: In-Memory HashMap
    println!("📊 Baseline: In-Memory HashMap");
    println!("{}", "-".repeat(60));

    let (read_baseline, write_baseline) = benchmark_hashmap()?;

    println!("  Read:  {:>6.0} ns avg", read_baseline);
    println!("  Write: {:>6.0} ns avg", write_baseline);
    println!();

    // LMDB Benchmark
    println!("💾 LMDB: Persistent Storage");
    println!("{}", "-".repeat(60));

    let (read_lmdb, write_lmdb) = benchmark_lmdb(&env, &db)?;

    println!("  Read:  {:>6.0} ns avg", read_lmdb);
    println!("  Write: {:>6.0} ns avg", write_lmdb);
    println!();

    // Comparison
    println!("📈 Performance Comparison");
    println!("{}", "=".repeat(60));

    let read_overhead = read_lmdb / read_baseline;
    let write_overhead_ns = write_lmdb - write_baseline;
    let write_overhead_us = write_overhead_ns / 1000.0;

    println!("  Read overhead:  {:.2}x slower ({:.0} ns → {:.0} ns)",
        read_overhead, read_baseline, read_lmdb);
    println!("  Write overhead: +{:.1} μs ({:.0} ns → {:.0} ns)",
        write_overhead_us, write_baseline, write_lmdb);
    println!();

    // Verdict
    println!("🎯 Performance Targets");
    println!("{}", "=".repeat(60));

    let read_pass = read_overhead < 2.0;
    let write_pass = write_overhead_us < 100.0;

    println!("  Read overhead < 2x:     {} (actual: {:.2}x)",
        if read_pass { "✅ PASS" } else { "❌ FAIL" }, read_overhead);
    println!("  Write overhead < 100μs: {} (actual: +{:.1}μs)",
        if write_pass { "✅ PASS" } else { "❌ FAIL" }, write_overhead_us);
    println!();

    if read_pass && write_pass {
        println!("✅ VERDICT: PASS - Proceed with LMDB implementation");
        println!("   Performance targets met for persistent storage layer.");
    } else {
        println!("❌ VERDICT: FAIL - Reconsider architecture");
        println!("   Performance targets NOT met. Need alternative approach.");
    }
    println!();

    // Cleanup
    drop(db);
    drop(env);
    std::fs::remove_dir_all(db_path)?;

    Ok(())
}

fn benchmark_hashmap() -> Result<(f64, f64), Box<dyn std::error::Error>> {
    let mut map = HashMap::new();

    // Warmup
    for i in 0..NUM_WARMUP {
        let key = format!("key_{}", i);
        let value = format!("value_{}", i);
        map.insert(key.clone(), value.clone());
        let _ = map.get(&key);
    }

    // Benchmark writes
    let start = Instant::now();
    for i in NUM_WARMUP..(NUM_WARMUP + NUM_OPERATIONS) {
        let key = format!("key_{}", i);
        let value = format!("value_{}", i);
        map.insert(key, value);
    }
    let write_duration = start.elapsed();
    let write_avg_ns = write_duration.as_nanos() as f64 / NUM_OPERATIONS as f64;

    // Benchmark reads
    let start = Instant::now();
    for i in NUM_WARMUP..(NUM_WARMUP + NUM_OPERATIONS) {
        let key = format!("key_{}", i);
        let _ = map.get(&key);
    }
    let read_duration = start.elapsed();
    let read_avg_ns = read_duration.as_nanos() as f64 / NUM_OPERATIONS as f64;

    Ok((read_avg_ns, write_avg_ns))
}

fn benchmark_lmdb(env: &Environment, db: &Database) -> Result<(f64, f64), Box<dyn std::error::Error>> {
    // Warmup with batched writes (100 per txn)
    const BATCH_SIZE: usize = 100;
    for batch_start in (0..NUM_WARMUP).step_by(BATCH_SIZE) {
        let mut txn = env.begin_rw_txn()?;
        for i in batch_start..(batch_start + BATCH_SIZE).min(NUM_WARMUP) {
            let key = format!("key_{}", i);
            let value = format!("value_{}", i);
            txn.put(*db, &key, &value, WriteFlags::empty())?;
        }
        txn.commit()?;
    }

    // Benchmark writes (batched for realistic performance)
    let start = Instant::now();
    for batch_start in (NUM_WARMUP..(NUM_WARMUP + NUM_OPERATIONS)).step_by(BATCH_SIZE) {
        let mut txn = env.begin_rw_txn()?;
        for i in batch_start..(batch_start + BATCH_SIZE).min(NUM_WARMUP + NUM_OPERATIONS) {
            let key = format!("key_{}", i);
            let value = format!("value_{}", i);
            txn.put(*db, &key, &value, WriteFlags::empty())?;
        }
        txn.commit()?;
    }
    let write_duration = start.elapsed();
    let write_avg_ns = write_duration.as_nanos() as f64 / NUM_OPERATIONS as f64;

    // Benchmark reads (use single long-lived RO transaction - LMDB best practice)
    let start = Instant::now();
    let txn = env.begin_ro_txn()?;
    for i in NUM_WARMUP..(NUM_WARMUP + NUM_OPERATIONS) {
        let key = format!("key_{}", i);
        let _value: &[u8] = txn.get(*db, &key)?;
    }
    txn.abort();
    let read_duration = start.elapsed();
    let read_avg_ns = read_duration.as_nanos() as f64 / NUM_OPERATIONS as f64;

    Ok((read_avg_ns, write_avg_ns))
}
