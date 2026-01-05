//! A simple, pure Rust implementation of the rsync algorithm using BLAKE3 for hashing.
//!
//! This library allows you to efficiently synchronize files by calculating the differences (delta) between two versions of a file and applying those differences to update the old version.
//!
//! ## Features
//!
//! - **BLAKE3 hashing**: Cryptographically strong 256-bit hashes for maximum reliability
//! - **Lightweight Buzhash** (optional): Fast 64-bit rolling hashes for performance-critical scenarios
//! - **Serde support** (optional): Serialization/deserialization for all data types
//! - **Chunking**: Configurable chunk sizes for optimal performance
//!
//! ## Examples
//!
//! ### Using BLAKE3 (default, cryptographically strong)
//!
//! ```
//! use std::io::Cursor;
//! use libsync3::{signature, delta, apply, apply_to_vec};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let old_data = b"Hello World";
//!     let new_data = b"Hello Rust World";
//!
//!     // 1. Generate signature of the old data
//!     let sig = signature(Cursor::new(old_data))?;
//!
//!     // 2. Compute delta between new data and the signature
//!     let diff = delta(Cursor::new(new_data), &sig)?;
//!
//!     // 3. Apply delta to old data to get new data
//!     let result = apply_to_vec(Cursor::new(old_data), &diff)?;
//!
//!     assert_eq!(result, new_data);
//!
//!     // Optionally:
//!     let mut output = Vec::with_capacity(diff.final_size);
//!     apply(Cursor::new(old_data), &diff, &mut output)?;
//!     assert_eq!(output, new_data);
//!
//!     Ok(())
//! }
//! ```
//!
//! ### Using Lightweight Buzhash (fast, 64-bit)
//!
//! This functionality is available when the `buzhash` feature is enabled.
//!
//! ```ignore
//! use std::io::Cursor;
//! use libsync3::{lightweight_signature, lightweight_delta, apply, apply_to_vec};
//!
//! fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let old_data = b"Hello World";
//!     let new_data = b"Hello Rust World";
//!
//!     // 1. Generate lightweight signature of the old data
//!     let sig = lightweight_signature(Cursor::new(old_data))?;
//!
//!     // 2. Compute delta between new data and the signature
//!     let diff = lightweight_delta(Cursor::new(new_data), &sig)?;
//!
//!     // 3. Apply delta to old data to get new data
//!     let result = apply_to_vec(Cursor::new(old_data), &diff)?;
//!
//!     assert_eq!(result, new_data);
//!
//!     Ok(())
//! }
//! ```
use blake3::Hash;
use std::collections::HashMap;
use std::io::{self, Read, Seek, SeekFrom, Write};

#[cfg(feature = "buzhash")]
mod buzhash;

#[cfg(feature = "buzhash")]
pub use buzhash::{
    BuzHash, LightweightChunkSignature, LightweightHash, LightweightSignature, hash64,
    lightweight_delta, lightweight_signature, lightweight_signature_with_chunk_size,
};

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Signature {
    pub chunk_size: usize,
    pub chunks: Vec<ChunkSignature>,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ChunkSignature {
    pub index: usize,
    pub hash: Hash,
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DeltaOp {
    Copy(usize),
    Insert(Vec<u8>),
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Delta {
    pub chunk_size: usize,
    pub ops: Vec<DeltaOp>,
    pub final_size: usize,
}

/// Default chunk size for signatures (4096 bytes)
pub const DEFAULT_CHUNK_SIZE: usize = 4096;

/// Creates a BLAKE3 signature from a reader by using `DEFAULT_CHUNK_SIZE`.
///
/// # Errors
///
/// Returns an error if reading from the reader fails.
pub fn signature<R: Read>(reader: R) -> io::Result<Signature> {
    signature_with_chunk_size(reader, DEFAULT_CHUNK_SIZE)
}

/// Creates a BLAKE3 signature from a reader by using a custom chunk size.
///
/// # Errors
///
/// Returns an error if reading from the reader fails.
pub fn signature_with_chunk_size<R: Read>(
    mut reader: R,
    chunk_size: usize,
) -> io::Result<Signature> {
    let mut chunks = Vec::new();
    let mut buf = vec![0u8; chunk_size];
    let mut index = 0;

    loop {
        let bytes_read = read_exact_or_eof(&mut reader, &mut buf)?;
        if bytes_read == 0 {
            break;
        }

        chunks.push(ChunkSignature {
            index,
            hash: blake3::hash(&buf[..bytes_read]),
        });
        index += 1;
    }

    Ok(Signature { chunk_size, chunks })
}

const TARGET_BATCH_SIZE: usize = 256 * 1024;

/// Computes a delta between new data (from reader) and an existing signature.
///
/// # Errors
///
/// Returns an error if reading from the reader fails.
pub fn delta<R: Read>(mut new_data: R, sig: &Signature) -> io::Result<Delta> {
    let mut hash_to_index: HashMap<Hash, usize> = HashMap::with_capacity(sig.chunks.len());
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
        // Find the largest multiple of chunk_size that fits in TARGET_BATCH_SIZE
        // But ensure we have at least one chunk (which is covered by the else if above, but good to be safe)
        // Actually, we want to be close to TARGET_BATCH_SIZE
        // Let's take (TARGET_BATCH_SIZE / chunk_size) * chunk_size
        // If that is 0 (shouldn't be since chunk_size < TARGET), we take chunk_size
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
            let hash = blake3::hash(chunk);

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

/// Applies a delta to `old_data` (from seekable reader) and writes to output.
///
/// # Errors
///
/// Returns an error if reading from `old_data` or writing to `output` fails.
pub fn apply<R, W>(mut old_data: R, dlt: &Delta, mut output: W) -> io::Result<()>
where
    R: Read + Seek,
    W: Write,
{
    let chunk_size = dlt.chunk_size;
    let mut buf = vec![0u8; chunk_size];

    for op in &dlt.ops {
        match op {
            DeltaOp::Copy(index) => {
                let offset = (*index as u64) * (chunk_size as u64);
                old_data.seek(SeekFrom::Start(offset))?;
                let bytes_read = read_exact_or_eof(&mut old_data, &mut buf)?;
                output.write_all(&buf[..bytes_read])?;
            }
            DeltaOp::Insert(data) => {
                output.write_all(data)?;
            }
        }
    }

    output.flush()?;
    Ok(())
}

/// Convenience: apply delta and return `Vec<u8>`.
///
/// # Errors
///
/// Returns an error if reading from `original` fails.
pub fn apply_to_vec<R: Read + Seek>(original: R, delta: &Delta) -> io::Result<Vec<u8>> {
    let mut output = Vec::with_capacity(delta.final_size);
    apply(original, delta, &mut output)?;
    Ok(output)
}

/// Reads up to `buf.len()` bytes, returns actual count (0 on EOF).
pub(crate) fn read_exact_or_eof<R: Read>(reader: &mut R, buf: &mut [u8]) -> io::Result<usize> {
    let mut total = 0;
    while total < buf.len() {
        match reader.read(&mut buf[total..]) {
            Ok(0) => break,
            Ok(n) => total += n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(total)
}

#[must_use]
pub fn suggest_chunk_size(file_size: usize) -> usize {
    match file_size {
        0..=65_536 => 512,                        // <64KB: small chunks
        65_537..=1_048_576 => DEFAULT_CHUNK_SIZE, // 64KB-1MB: default
        1_048_577..=104_857_600 => 8192,          // 1MB-100MB
        _ => 16384,                               // >100MB
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_identical_data() {
        let data = b"Hello, world! This is some test data.";
        let sig = signature(Cursor::new(data)).unwrap();
        let d = delta(Cursor::new(data), &sig).unwrap();
        let result = apply_to_vec(Cursor::new(data), &d).unwrap();
        assert_eq!(data.as_slice(), result.as_slice());
    }

    #[test]
    fn test_small_change() {
        let original = b"AAAA BBBB CCCC DDDD EEEE";
        let modified = b"AAAA XXXX CCCC DDDD EEEE";

        let sig = signature_with_chunk_size(Cursor::new(original), 5).unwrap();
        let d = delta(Cursor::new(modified), &sig).unwrap();
        let result = apply_to_vec(Cursor::new(original), &d).unwrap();

        assert_eq!(modified.as_slice(), result.as_slice());
    }

    #[test]
    fn test_completely_different() {
        let original = b"Original content here";
        let modified = b"Completely different";

        let sig = signature(Cursor::new(original)).unwrap();
        let d = delta(Cursor::new(modified), &sig).unwrap();
        let result = apply_to_vec(Cursor::new(original), &d).unwrap();

        assert_eq!(modified.as_slice(), result.as_slice());
    }

    #[test]
    fn test_with_writer() {
        let original = b"Test data here";
        let modified = b"Test data modified";

        let sig = signature(Cursor::new(original)).unwrap();
        let d = delta(Cursor::new(modified), &sig).unwrap();

        let mut output = Vec::new();
        apply(Cursor::new(original), &d, &mut output).unwrap();

        assert_eq!(modified.as_slice(), output.as_slice());
    }
}
