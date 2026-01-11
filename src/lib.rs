use std::collections::HashMap;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use twox_hash::XxHash3_128;

/// Reads exactly `buf.len()` bytes or until EOF, returning the number of bytes read.
fn read_exact_or_eof<R: Read>(reader: &mut R, buf: &mut [u8]) -> std::io::Result<usize> {
    let mut total = 0;
    while total < buf.len() {
        match reader.read(&mut buf[total..]) {
            Ok(0) => break,
            Ok(n) => total += n,
            Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(total)
}

#[inline]
fn xxh3_128(chunk: &[u8]) -> u128 {
    XxHash3_128::oneshot(chunk)
}

#[derive(Copy, Clone)]
pub struct RsyncConfig {
    pub block_size: usize,
}

impl Default for RsyncConfig {
    fn default() -> Self {
        Self { block_size: 4096 }
    }
}

pub type Signatures = HashMap<u128, usize>;

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DeltaCommand {
    Data(Vec<u8>),
    Copy { offset: usize, length: usize },
}

#[derive(Default, Copy, Clone)]
pub struct BufferRsync {
    config: RsyncConfig,
}

impl BufferRsync {
    #[must_use]
    pub fn new(config: RsyncConfig) -> Self {
        Self { config }
    }

    /// Generate signatures from a reader.
    ///
    /// # Errors
    /// Returns an error if reading from the reader fails.
    pub fn generate_signatures<R: Read>(&self, mut reader: R) -> std::io::Result<Signatures> {
        let mut signatures: Signatures = HashMap::new();
        let mut buffer = vec![0u8; self.config.block_size];

        for block_index in 0.. {
            let bytes_read = read_exact_or_eof(&mut reader, &mut buffer)?;
            if bytes_read == 0 {
                break;
            }

            let chunk = &buffer[..bytes_read];
            let hash = xxh3_128(chunk);
            signatures.entry(hash).or_insert(block_index);
        }

        Ok(signatures)
    }

    /// Generate delta from signatures and a reader containing new data.
    ///
    /// # Errors
    /// Returns an error if reading from the reader fails.
    pub fn generate_delta<R: Read>(
        &self,
        old_signatures: &Signatures,
        mut reader: R,
    ) -> std::io::Result<Vec<DeltaCommand>> {
        let mut delta = Vec::new();
        let mut buffer = vec![0u8; self.config.block_size];

        loop {
            let bytes_read = read_exact_or_eof(&mut reader, &mut buffer)?;
            if bytes_read == 0 {
                break;
            }

            let chunk = &buffer[..bytes_read];
            let hash = xxh3_128(chunk);

            if let Some(&matched) = old_signatures.get(&hash) {
                delta.push(DeltaCommand::Copy {
                    offset: matched,
                    length: bytes_read,
                });
            } else {
                delta.push(DeltaCommand::Data(chunk.into()));
            }
        }

        Ok(delta)
    }

    /// # Errors
    /// Returns an error if the delta contains invalid copy commands (out of bounds or overflow) or if IO operations fail.
    pub fn apply_delta<R: Read + Seek, W: Write>(
        &self,
        mut base_reader: R,
        delta: &[DeltaCommand],
        target_writer: W,
    ) -> std::io::Result<()> {
        const BUF_SIZE: usize = 64 * 1024;
        let mut writer = BufWriter::with_capacity(BUF_SIZE, target_writer);
        let mut current_pos: u64 = 0;

        for command in delta {
            match command {
                DeltaCommand::Data(data) => {
                    writer.write_all(data)?;
                }
                DeltaCommand::Copy { offset, length } => {
                    let start = (*offset * self.config.block_size) as u64;

                    if start != current_pos {
                        base_reader.seek(SeekFrom::Start(start))?;
                    }

                    let len = *length as u64;
                    std::io::copy(&mut (&mut base_reader).take(len), &mut writer)?;
                    current_pos = start + len;
                }
            }
        }
        writer.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_basic_rsync() {
        let rsync = BufferRsync::new(RsyncConfig::default());

        let original = b"Hello, world! This is a test file for rsync.";
        let modified = b"Hello, world! This is a modified test file for rsync.";

        let signatures = rsync.generate_signatures(&original[..]).unwrap();
        let delta = rsync.generate_delta(&signatures, &modified[..]).unwrap();

        let mut reconstructed = Vec::new();
        rsync
            .apply_delta(Cursor::new(original), &delta, &mut reconstructed)
            .unwrap();

        assert_eq!(reconstructed, modified);
    }

    #[test]
    fn test_handles_insertions() {
        let rsync = BufferRsync::new(RsyncConfig { block_size: 8 });

        let original = b"ABCDEFGHabcdefgh";
        let modified = b"ABCXYZDEFGHabcdefgh";

        let signatures = rsync.generate_signatures(&original[..]).unwrap();
        let delta = rsync.generate_delta(&signatures, &modified[..]).unwrap();

        let mut reconstructed = Vec::new();
        rsync
            .apply_delta(Cursor::new(original), &delta, &mut reconstructed)
            .unwrap();

        assert_eq!(reconstructed, modified);
    }

    #[test]
    fn test_unchanged_data() {
        let rsync = BufferRsync::new(RsyncConfig::default());

        let data = b"Hello, world! This is a test file for rsync.";

        let signatures = rsync.generate_signatures(&data[..]).unwrap();
        let delta = rsync.generate_delta(&signatures, &data[..]).unwrap();

        let mut reconstructed = Vec::new();
        rsync
            .apply_delta(Cursor::new(data), &delta, &mut reconstructed)
            .unwrap();

        assert_eq!(reconstructed, data);
    }

    #[test]
    fn test_completely_different_data() {
        let rsync = BufferRsync::new(RsyncConfig::default());

        let original = b"Hello, world!";
        let modified = b"Goodbye, world!";

        let signatures = rsync.generate_signatures(&original[..]).unwrap();
        let delta = rsync.generate_delta(&signatures, &modified[..]).unwrap();

        let mut reconstructed = Vec::new();
        rsync
            .apply_delta(Cursor::new(original), &delta, &mut reconstructed)
            .unwrap();

        assert_eq!(reconstructed, modified);
    }
}
