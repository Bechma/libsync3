//! Basic Buzhash usage examples

use libsync3::{BuzHash, LightweightHash, hash64};
use std::num::NonZeroUsize;

fn main() {
    println!("=== Basic Buzhash Examples ===\n");

    // Example 1: Simple hash computation
    println!("1. Simple hash computation:");
    let data = b"Hello, Buzhash!";
    let hash = hash64(data);
    println!("   Data: {:?}", std::str::from_utf8(data).unwrap());
    println!("   Hash: 0x{hash:016x}");
    println!();

    // Example 2: Using LightweightHash wrapper
    println!("2. LightweightHash wrapper:");
    let lw_hash = LightweightHash::new(data);
    println!("   LightweightHash: 0x{:016x}", lw_hash.as_u64());
    println!("   Conversion to u64: 0x{:016x}", u64::from(lw_hash));
    println!();

    // Example 3: Rolling hash with small window
    println!("3. Rolling hash demonstration (window size = 4):");
    let mut buzhash = BuzHash::new(NonZeroUsize::new(4).unwrap());
    let text = b"ABCDEFGH";

    println!("   Processing: {:?}", std::str::from_utf8(text).unwrap());
    for (i, &byte) in text.iter().enumerate() {
        buzhash.update(byte);
        println!(
            "   After '{}' (pos {}): 0x{:016x}",
            byte as char,
            i,
            buzhash.hash()
        );
    }
    println!();

    // Example 4: Reset functionality
    println!("4. Reset functionality:");
    println!("   Hash before reset: 0x{:016x}", buzhash.hash());
    buzhash.reset();
    println!("   Hash after reset:  0x{:016x}", buzhash.hash());
    println!();

    // Example 5: Comparing hashes
    println!("5. Hash comparison:");
    let data1 = b"identical";
    let data2 = b"identical";
    let data3 = b"different";

    let hash1 = hash64(data1);
    let hash2 = hash64(data2);
    let hash3 = hash64(data3);

    println!("   Hash of 'identical': 0x{hash1:016x}");
    println!("   Hash of 'identical': 0x{hash2:016x}");
    println!("   Hash of 'different': 0x{hash3:016x}");
    println!("   hash1 == hash2: {}", hash1 == hash2);
    println!("   hash1 == hash3: {}", hash1 == hash3);
    println!();

    // Example 6: Different window sizes
    println!("6. Effect of window size:");
    for window_size in [2, 4, 8, 16] {
        let mut buz = BuzHash::new(NonZeroUsize::new(window_size).unwrap());
        for &byte in b"test" {
            buz.update(byte);
        }
        println!("   Window size {}: 0x{:016x}", window_size, buz.hash());
    }
}
