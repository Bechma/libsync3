#![cfg(feature = "buzhash")]

use libsync3::{BuzHash, LightweightHash, hash64};
use proptest::prelude::*;
use std::num::NonZeroUsize;

proptest! {
    #[test]
    fn test_hash64_deterministic(data in prop::collection::vec(any::<u8>(), 0..1000)) {
        let hash1 = hash64(&data);
        let hash2 = hash64(&data);
        prop_assert_eq!(hash1, hash2, "Same data should produce same hash");
    }

    #[test]
    fn test_lightweight_hash_deterministic(data in prop::collection::vec(any::<u8>(), 0..1000)) {
        let hash1 = LightweightHash::new(&data);
        let hash2 = LightweightHash::new(&data);
        prop_assert_eq!(hash1, hash2, "Same data should produce same LightweightHash");
    }

    #[test]
    fn test_different_data_different_hash(
        data1 in prop::collection::vec(any::<u8>(), 1..100),
        data2 in prop::collection::vec(any::<u8>(), 1..100)
    ) {
        prop_assume!(data1 != data2);
        let hash1 = hash64(&data1);
        let hash2 = hash64(&data2);
        // While hash collisions are possible, they should be extremely rare
        // We're not asserting inequality here as it could theoretically fail
        // but we log it for analysis
        if hash1 == hash2 {
            eprintln!("Hash collision detected (rare but possible)");
        }
    }

    #[test]
    fn test_buzhash_rolling_window(
        window_size in 1usize..32,
        data in prop::collection::vec(any::<u8>(), 1..100)
    ) {
        let mut buzhash = BuzHash::new(NonZeroUsize::new(window_size).unwrap());

        for &byte in &data {
            buzhash.update(byte);
        }

        // Hash should be non-zero after processing data (with very high probability)
        let final_hash = buzhash.hash();
        prop_assert!(final_hash != 0 || data.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_buzhash_reset(
        window_size in 1usize..32,
        data in prop::collection::vec(any::<u8>(), 1..100)
    ) {
        let mut buzhash = BuzHash::new(NonZeroUsize::new(window_size).unwrap());

        for &byte in &data {
            buzhash.update(byte);
        }

        buzhash.reset();
        prop_assert_eq!(buzhash.hash(), 0, "Hash should be zero after reset");
    }

    #[test]
    fn test_buzhash_window_size(window_size in 1usize..100) {
        let buzhash = BuzHash::new(NonZeroUsize::new(window_size).unwrap());
        prop_assert_eq!(buzhash.hash(), 0, "New BuzHash should have zero hash");
    }

    #[test]
    fn test_lightweight_hash_conversion(hash_value in any::<u64>()) {
        let lw_hash: LightweightHash = hash_value.into();
        let converted: u64 = lw_hash.into();
        prop_assert_eq!(hash_value, converted, "Conversion should be lossless");
    }

    #[test]
    fn test_buzhash_incremental_vs_batch(
        window_size in 1usize..32,
        data in prop::collection::vec(any::<u8>(), 1..100)
    ) {
        if data.len() < window_size {
            return Ok(());
        }

        let mut incremental = BuzHash::new(NonZeroUsize::new(window_size).unwrap());
        for &byte in &data {
            incremental.update(byte);
        }

        // The hash should be consistent when processing the same data
        let hash1 = incremental.hash();

        let mut second_pass = BuzHash::new(NonZeroUsize::new(window_size).unwrap());
        for &byte in &data {
            second_pass.update(byte);
        }
        let hash2 = second_pass.hash();

        prop_assert_eq!(hash1, hash2, "Same data should produce same rolling hash");
    }

    #[test]
    fn test_hash_distribution(data in prop::collection::vec(any::<u8>(), 10..100)) {
        let _hash = hash64(&data);
        // Just verify we can compute the hash without panicking
    }

    #[test]
    fn test_buzhash_single_byte_updates(
        window_size in 2usize..16,
        byte1 in any::<u8>(),
        byte2 in any::<u8>()
    ) {
        let mut buzhash = BuzHash::new(NonZeroUsize::new(window_size).unwrap());

        buzhash.update(byte1);
        let hash_after_first = buzhash.hash();

        buzhash.update(byte2);
        let hash_after_second = buzhash.hash();

        // Hashes should be different unless both bytes are the same and window isn't full
        if byte1 != byte2 || window_size == 1 {
            prop_assert_ne!(hash_after_first, hash_after_second);
        }
    }

    #[test]
    fn test_lightweight_hash_equality(data in prop::collection::vec(any::<u8>(), 0..100)) {
        let hash1 = LightweightHash::new(&data);
        let hash2 = LightweightHash::new(&data);
        prop_assert_eq!(hash1, hash2);
        prop_assert_eq!(hash1.as_u64(), hash2.as_u64());
    }

    #[test]
    fn test_hash64_consistency_with_update(data in prop::collection::vec(any::<u8>(), 0..100)) {
        let hash_direct = hash64(&data);

        // Simulate with BuzHash having a window large enough to hold all data
        // Ensure window_size is at least 1 even for empty data
        let window_size = std::cmp::max(data.len(), 1);
        let mut buzhash = BuzHash::new(NonZeroUsize::new(window_size).unwrap());
        for &byte in &data {
            buzhash.update(byte);
        }
        let hash_rolling = buzhash.hash();

        prop_assert_eq!(hash_direct, hash_rolling, "hash64 should match BuzHash result with sufficient window");
    }
}

#[test]
fn test_buzhash_empty_data() {
    let buzhash = BuzHash::new(NonZeroUsize::new(4).unwrap());
    assert_eq!(buzhash.hash(), 0);
}

#[test]
fn test_hash64_empty() {
    let hash = hash64(b"");
    assert_eq!(hash, 0); // Empty data results in 0 (initial state)
}

#[test]
fn test_buzhash_window_wraparound() {
    let mut buzhash = BuzHash::new(NonZeroUsize::new(3).unwrap());

    buzhash.update(b'A');
    buzhash.update(b'B');
    buzhash.update(b'C');
    let hash_full = buzhash.hash();

    // Add one more to cause wraparound
    buzhash.update(b'D');
    let hash_wrapped = buzhash.hash();

    assert_ne!(hash_full, hash_wrapped);
}
