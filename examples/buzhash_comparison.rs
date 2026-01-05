//! Compares Buzhash performance with BLAKE3 for different use cases

use libsync3::{hash64, lightweight_delta, lightweight_signature, signature};
use std::io::Cursor;
use std::time::Instant;

fn main() {
    println!("=== Buzhash vs BLAKE3 Comparison ===\n");

    // Test data of various sizes
    let small_data = b"Small test data";
    let medium_data = vec![b'X'; 1024]; // 1KB
    let large_data = vec![b'Y'; 10_240]; // 10KB

    println!("1. Hash computation comparison:\n");

    // Small data
    println!("Small data ({} bytes):", small_data.len());
    compare_hash_speed(small_data);
    println!();

    // Medium data
    println!("Medium data ({} bytes):", medium_data.len());
    compare_hash_speed(&medium_data);
    println!();

    // Large data
    println!("Large data ({} bytes):", large_data.len());
    compare_hash_speed(&large_data);
    println!();

    println!("2. Signature and delta comparison:\n");

    let old_data = b"The quick brown fox jumps over the lazy dog. ";
    let new_data = b"The quick brown fox leaps over the lazy cat. ";

    println!("Old data: {:?}", std::str::from_utf8(old_data).unwrap());
    println!("New data: {:?}", std::str::from_utf8(new_data).unwrap());
    println!();

    // BLAKE3 signature
    let start = Instant::now();
    let blake3_sig = signature(Cursor::new(old_data)).unwrap();
    let blake3_sig_time = start.elapsed();

    let start = Instant::now();
    let blake3_delta = libsync3::delta(Cursor::new(new_data), &blake3_sig).unwrap();
    let blake3_delta_time = start.elapsed();

    println!("BLAKE3:");
    println!("  Signature time: {blake3_sig_time:?}");
    println!("  Delta time:     {blake3_delta_time:?}");
    println!("  Chunks:         {}", blake3_sig.chunks.len());
    println!("  Delta ops:      {}", blake3_delta.ops.len());
    println!();

    // Buzhash signature
    let start = Instant::now();
    let buzhash_sig = lightweight_signature(Cursor::new(old_data)).unwrap();
    let buzhash_sig_time = start.elapsed();

    let start = Instant::now();
    let buzhash_delta = lightweight_delta(Cursor::new(new_data), &buzhash_sig).unwrap();
    let buzhash_delta_time = start.elapsed();

    println!("Buzhash:");
    println!("  Signature time: {buzhash_sig_time:?}");
    println!("  Delta time:     {buzhash_delta_time:?}");
    println!("  Chunks:         {}", buzhash_sig.chunks.len());
    println!("  Delta ops:      {}", buzhash_delta.ops.len());
    println!();

    println!("3. Hash size comparison:\n");
    println!("BLAKE3 hash size:      256 bits (32 bytes)");
    println!("Buzhash hash size:     64 bits (8 bytes)");
    println!("Size reduction:        75%");
    println!();

    println!("4. Use case recommendations:\n");
    println!("Use BLAKE3 when:");
    println!("  - Cryptographic security is required");
    println!("  - Data integrity verification is critical");
    println!("  - Hash collision resistance is paramount");
    println!();
    println!("Use Buzhash when:");
    println!("  - Performance is critical");
    println!("  - Memory usage needs to be minimized");
    println!("  - Content-defined chunking is needed");
    println!("  - Rolling hash capabilities are beneficial");
}

fn compare_hash_speed(data: &[u8]) {
    // Buzhash
    let start = Instant::now();
    let buzhash = hash64(data);
    let buzhash_time = start.elapsed();

    // BLAKE3
    let start = Instant::now();
    let blake3 = blake3::hash(data);
    let blake3_time = start.elapsed();

    println!("  Buzhash: 0x{buzhash:016x} in {buzhash_time:?}");
    println!("  BLAKE3:  {blake3} in {blake3_time:?}");

    if buzhash_time < blake3_time {
        #[allow(clippy::cast_precision_loss)]
        let speedup = blake3_time.as_nanos() as f64 / buzhash_time.as_nanos() as f64;
        println!("  Buzhash is {speedup:.2}x faster");
    }
}
