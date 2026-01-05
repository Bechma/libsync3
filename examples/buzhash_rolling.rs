//! Demonstrates rolling hash capabilities of Buzhash

use libsync3::BuzHash;
use std::num::NonZeroUsize;

fn main() {
    println!("=== Buzhash Rolling Hash Example ===\n");

    // Simulate a sliding window over a data stream
    let data = b"The quick brown fox jumps over the lazy dog";
    let window_size = 8;

    println!("Data: {:?}", std::str::from_utf8(data).unwrap());
    println!("Window size: {window_size}\n");

    let mut buzhash = BuzHash::new(NonZeroUsize::new(window_size).unwrap());

    println!("Rolling hash values as window slides:");
    println!("{:<5} {:<20} {:<18}", "Pos", "Window", "Hash");
    println!("{}", "-".repeat(50));

    for (i, &byte) in data.iter().enumerate() {
        buzhash.update(byte);

        // Show the current window content
        let window_start = if i >= window_size {
            i + 1 - window_size
        } else {
            0
        };
        let window_end = i + 1;
        let window = &data[window_start..window_end];

        if let Ok(window_str) = std::str::from_utf8(window) {
            println!(
                "{:<5} {:<20} 0x{:016x}",
                i,
                format!("'{}'", window_str),
                buzhash.hash()
            );
        }
    }

    println!("\n=== Detecting Repeated Patterns ===\n");

    // Use rolling hash to detect repeated patterns
    let pattern_data = b"ABCABCABCXYZXYZXYZ";
    let pattern_window = 3;

    println!("Data: {:?}", std::str::from_utf8(pattern_data).unwrap());
    println!("Looking for repeated patterns with window size: {pattern_window}\n");

    let mut pattern_hash = BuzHash::new(NonZeroUsize::new(pattern_window).unwrap());
    let mut hash_positions: std::collections::HashMap<u64, Vec<usize>> =
        std::collections::HashMap::new();

    for (i, &byte) in pattern_data.iter().enumerate() {
        pattern_hash.update(byte);

        if i >= pattern_window - 1 {
            let hash = pattern_hash.hash();
            hash_positions
                .entry(hash)
                .or_default()
                .push(i + 1 - pattern_window);
        }
    }

    println!("Repeated hash values (potential pattern matches):");
    for (hash, positions) in &hash_positions {
        if positions.len() > 1 {
            println!("  Hash 0x{hash:016x} found at positions: {positions:?}");
            for &pos in positions {
                let end = (pos + pattern_window).min(pattern_data.len());
                if let Ok(pattern) = std::str::from_utf8(&pattern_data[pos..end]) {
                    println!("    Position {pos}: '{pattern}'");
                }
            }
        }
    }

    println!("\n=== Content-Defined Chunking Simulation ===\n");

    // Simulate content-defined chunking using rolling hash
    let chunk_data = b"This is a longer piece of text that we want to split into chunks based on content boundaries rather than fixed sizes.";
    let chunk_window = 4;
    let chunk_mask = 0x0FFF; // Trigger chunk boundary when hash & mask == 0

    println!("Data: {:?}", std::str::from_utf8(chunk_data).unwrap());
    println!("Window size: {chunk_window}");
    println!("Chunk boundary mask: 0x{chunk_mask:04x}\n");

    let mut chunk_hash = BuzHash::new(NonZeroUsize::new(chunk_window).unwrap());
    let mut chunk_boundaries = vec![0];

    for (i, &byte) in chunk_data.iter().enumerate() {
        chunk_hash.update(byte);

        if i >= chunk_window - 1 {
            let hash = chunk_hash.hash();
            if (hash & chunk_mask) == 0 {
                chunk_boundaries.push(i + 1);
                println!(
                    "Chunk boundary at position {}: hash = 0x{:016x}",
                    i + 1,
                    hash
                );
            }
        }
    }

    chunk_boundaries.push(chunk_data.len());

    println!("\nChunks created:");
    for i in 0..chunk_boundaries.len() - 1 {
        let start = chunk_boundaries[i];
        let end = chunk_boundaries[i + 1];
        let chunk = &chunk_data[start..end];
        if let Ok(chunk_str) = std::str::from_utf8(chunk) {
            println!(
                "  Chunk {}: [{}..{}] ({} bytes) '{}'",
                i,
                start,
                end,
                end - start,
                chunk_str
            );
        }
    }
}
