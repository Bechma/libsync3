//! Buzhash implementation for lightweight rolling hash
//!
//! Buzhash is a rolling hash algorithm that's fast and has good distribution properties.
//! It's particularly useful for content-defined chunking in file synchronization.

use crate::{DEFAULT_CHUNK_SIZE, Delta, DeltaOp, read_exact_or_eof};
use std::collections::HashMap;
use std::io::{self, Read};
use std::num::NonZeroUsize;

/// A 64-bit Buzhash implementation
#[derive(Debug, Clone)]
pub struct BuzHash {
    /// Current hash value
    hash: u64,
    /// Window size (number of bytes in the rolling window)
    window_size: NonZeroUsize,
    /// Circular buffer of bytes in the current window
    window: Vec<u8>,
    /// Current position in the circular buffer
    pos: usize,
    /// Whether the window is full yet
    window_full: bool,
}

impl BuzHash {
    /// Create a new `BuzHash` with the specified window size
    #[must_use]
    pub fn new(window_size: NonZeroUsize) -> Self {
        Self {
            hash: 0,
            window_size,
            window: vec![0; window_size.get()],
            pos: 0,
            window_full: false,
        }
    }

    /// Get the current hash value
    #[must_use]
    pub fn hash(&self) -> u64 {
        self.hash
    }

    /// Reset the hash state
    pub fn reset(&mut self) {
        self.hash = 0;
        self.pos = 0;
        self.window_full = false;
        for byte in &mut self.window {
            *byte = 0;
        }
    }

    /// Update the hash with a new byte (rolling hash)
    pub fn update(&mut self, byte: u8) {
        // Rotate the current hash to the left by 1
        self.hash = self.hash.rotate_left(1);

        // Add the new byte
        self.hash ^= Self::map_byte(byte);

        // Remove the byte that's sliding out
        if self.window_full {
            let old_byte = self.window[self.pos];
            // The old byte was added window_size steps ago.
            // Since we rotate left by 1 at each step, the contribution of old_byte
            // has been rotated left by window_size.
            // Safe cast because we mod by 64, so the value is in 0..64 range which fits in u32
            #[allow(clippy::cast_possible_truncation)]
            let shift = (self.window_size.get() % 64) as u32;
            self.hash ^= Self::map_byte(old_byte).rotate_left(shift);
        }

        // Store the new byte in the window
        self.window[self.pos] = byte;
        self.pos = (self.pos + 1) % self.window_size.get();

        // Mark window as full if we've wrapped around
        if self.pos == 0 {
            self.window_full = true;
        }
    }

    /// Compute the hash contribution of a byte (pseudo-random mapping)
    fn map_byte(byte: u8) -> u64 {
        // SplitMix64-like mixing to map byte to random u64
        // Constants from SplitMix64
        let mut x = u64::from(byte) ^ 0x9E37_79B9_7F4A_7C15;
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        x ^ (x >> 31)
    }

    /// Compute hash of a byte slice (non-rolling)
    /// This emulates feeding the slice into a `BuzHash` with `window_size` >= `slice.len()`
    #[must_use]
    pub fn hash_slice(data: &[u8]) -> u64 {
        let mut hash = 0u64;
        for &byte in data {
            hash = hash.rotate_left(1);
            hash ^= Self::map_byte(byte);
        }
        hash
    }
}

/// Convenience function to compute a 64-bit hash of a byte slice
#[must_use]
pub fn hash64(data: &[u8]) -> u64 {
    BuzHash::hash_slice(data)
}

/// A lightweight 64-bit hash wrapper for Buzhash
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LightweightHash(pub u64);

impl LightweightHash {
    /// Create a new lightweight hash from a byte slice
    #[must_use]
    pub fn new(data: &[u8]) -> Self {
        Self(hash64(data))
    }

    /// Get the underlying 64-bit hash value
    #[must_use]
    pub fn as_u64(&self) -> u64 {
        self.0
    }
}

impl From<u64> for LightweightHash {
    fn from(hash: u64) -> Self {
        Self(hash)
    }
}

impl From<LightweightHash> for u64 {
    fn from(hash: LightweightHash) -> Self {
        hash.0
    }
}

/// A lightweight signature using Buzhash (64-bit hashes)
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LightweightSignature {
    pub chunk_size: usize,
    pub chunks: Vec<LightweightChunkSignature>,
}

/// A chunk signature using Buzhash (64-bit hash)
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LightweightChunkSignature {
    pub index: usize,
    pub hash: LightweightHash,
}

/// Creates a lightweight signature using Buzhash (64-bit) from a reader by using `DEFAULT_CHUNK_SIZE`.
///
/// # Errors
///
/// Returns an error if reading from the reader fails.
pub fn lightweight_signature<R: Read>(reader: R) -> io::Result<LightweightSignature> {
    lightweight_signature_with_chunk_size(reader, DEFAULT_CHUNK_SIZE)
}

/// Creates a lightweight signature using Buzhash (64-bit) from a reader by using a custom chunk size.
///
/// # Errors
///
/// Returns an error if reading from the reader fails.
pub fn lightweight_signature_with_chunk_size<R: Read>(
    mut reader: R,
    chunk_size: usize,
) -> io::Result<LightweightSignature> {
    let mut chunks = Vec::new();
    let mut buf = vec![0u8; chunk_size];
    let mut index = 0;

    loop {
        let bytes_read = read_exact_or_eof(&mut reader, &mut buf)?;
        if bytes_read == 0 {
            break;
        }

        chunks.push(LightweightChunkSignature {
            index,
            hash: LightweightHash::new(&buf[..bytes_read]),
        });
        index += 1;
    }

    Ok(LightweightSignature { chunk_size, chunks })
}

const TARGET_BATCH_SIZE: usize = 256 * 1024;

/// Computes a delta between new data (from reader) and an existing lightweight signature.
///
/// # Errors
///
/// Returns an error if reading from the reader fails.
pub fn lightweight_delta<R: Read>(
    mut new_data: R,
    sig: &LightweightSignature,
) -> io::Result<Delta> {
    let mut hash_to_index: HashMap<LightweightHash, usize> =
        HashMap::with_capacity(sig.chunks.len());
    hash_to_index.extend(sig.chunks.iter().map(|chunk| (&chunk.hash, &chunk.index)));

    let chunk_size = sig.chunk_size;
    if chunk_size == 0 {
        return Ok(Delta {
            chunk_size: 0,
            ops: Vec::new(),
            final_size: 0,
        });
    }

    let mut ops = Vec::new();
    let mut total_size = 0usize;

    // Use a larger buffer to reduce I/O calls
    // Target a buffer size of around 64KB to 256KB for efficiency
    let batch_size = if chunk_size >= 256 * 1024 {
        chunk_size
    } else {
        let multiple = TARGET_BATCH_SIZE / chunk_size;
        let s = multiple * chunk_size;
        if s == 0 { chunk_size } else { s }
    };

    let mut buffer = vec![0u8; batch_size];
    let mut pending_literal: Vec<u8> = Vec::new();

    loop {
        let bytes_read = read_exact_or_eof(&mut new_data, &mut buffer)?;
        if bytes_read == 0 {
            break;
        }

        total_size += bytes_read;
        let valid_buffer = &buffer[..bytes_read];

        // Iterate over chunks
        let mut literal_start = 0;
        for (i, chunk) in valid_buffer.chunks(chunk_size).enumerate() {
            let hash = LightweightHash::new(chunk);

            if let Some(&index) = hash_to_index.get(&hash) {
                let chunk_offset = i * chunk_size;

                // Append pending literal data from the current buffer before this chunk
                if chunk_offset > literal_start {
                    pending_literal.extend_from_slice(&valid_buffer[literal_start..chunk_offset]);
                }

                // Flush pending_literal
                if !pending_literal.is_empty() {
                    ops.push(DeltaOp::Insert(std::mem::take(&mut pending_literal)));
                }

                ops.push(DeltaOp::Copy(index));
                literal_start = chunk_offset + chunk.len();
            }
        }

        // Append remaining data in buffer to pending_literal
        if literal_start < valid_buffer.len() {
            pending_literal.extend_from_slice(&valid_buffer[literal_start..]);
        }
    }

    // Flush remaining literal
    if !pending_literal.is_empty() {
        ops.push(DeltaOp::Insert(pending_literal));
    }

    Ok(Delta {
        chunk_size,
        ops,
        final_size: total_size,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buzhash_basic() {
        let mut buzhash = BuzHash::new(NonZeroUsize::new(4).unwrap());

        // Test initial state
        assert_eq!(buzhash.hash(), 0);

        // Add some bytes
        buzhash.update(b'A');
        buzhash.update(b'B');
        buzhash.update(b'C');
        buzhash.update(b'D');

        // Hash should be non-zero after adding bytes
        assert_ne!(buzhash.hash(), 0);
    }

    #[test]
    fn test_buzhash_rolling() {
        let mut buzhash = BuzHash::new(NonZeroUsize::new(3).unwrap());

        // Fill the window
        buzhash.update(b'A');
        buzhash.update(b'B');
        buzhash.update(b'C');
        let hash1 = buzhash.hash();

        // Add another byte (should roll out 'A')
        buzhash.update(b'D');
        let hash2 = buzhash.hash();

        // Hashes should be different
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_hash_slice() {
        let data1 = b"Hello";
        let data2 = b"World";
        let data3 = b"Hello";

        let hash1 = BuzHash::hash_slice(data1);
        let hash2 = BuzHash::hash_slice(data2);
        let hash3 = BuzHash::hash_slice(data3);

        // Same data should produce same hash
        assert_eq!(hash1, hash3);
        // Different data should produce different hash
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_hash64_convenience() {
        let data = b"test data";
        let hash1 = hash64(data);
        let hash2 = BuzHash::hash_slice(data);

        assert_eq!(hash1, hash2);
    }

    #[test]
    fn test_reset() {
        let mut buzhash = BuzHash::new(NonZeroUsize::new(4).unwrap());

        buzhash.update(b'A');
        buzhash.update(b'B');
        assert_ne!(buzhash.hash(), 0);

        buzhash.reset();
        assert_eq!(buzhash.hash(), 0);
        assert!(!buzhash.window_full);
        assert_eq!(buzhash.pos, 0);
    }
}
