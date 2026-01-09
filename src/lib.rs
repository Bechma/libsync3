use rayon::prelude::*;
use std::collections::HashMap;
use std::io::Read;
use xxhash_rust::xxh3::xxh3_64;

pub struct RsyncConfig {
    pub block_size: usize,
}

impl Default for RsyncConfig {
    fn default() -> Self {
        Self { block_size: 4096 }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct BlockSignature {
    pub strong_hash: u64,
    pub block_index: usize,
}

#[derive(Debug)]
pub enum DeltaCommand {
    Data(std::sync::Arc<[u8]>),
    Copy { offset: usize, length: usize },
}

pub type Signatures = HashMap<u64, Vec<BlockSignature>>;

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
        let mut data = Vec::new();
        reader.read_to_end(&mut data)?;

        let chunks: Vec<&[u8]> = data.chunks(self.config.block_size).collect();

        let signatures: Signatures = chunks
            .par_iter()
            .enumerate()
            .fold(HashMap::new, |mut acc: Signatures, (i, chunk)| {
                let hash = xxh3_64(chunk);
                acc.entry(hash).or_default().push(BlockSignature {
                    strong_hash: xxh3_64(chunk),
                    block_index: i,
                });
                acc
            })
            .reduce(HashMap::new, |mut acc: Signatures, mut other| {
                for (key, vals) in other.drain() {
                    acc.entry(key).or_default().extend(vals);
                }
                acc
            });

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
        let mut new_data = Vec::new();
        reader.read_to_end(&mut new_data)?;

        Ok(self.generate_delta_from_slice(old_signatures, &new_data))
    }

    #[must_use]
    pub fn generate_delta_from_slice(
        &self,
        old_signatures: &Signatures,
        new_data: &[u8],
    ) -> Vec<DeltaCommand> {
        let mut delta = Vec::with_capacity(new_data.len() / self.config.block_size + 1);
        let block_size = self.config.block_size;
        let data_len = new_data.len();

        if data_len < block_size {
            let hash = xxh3_64(new_data);
            if let Some(matched) = old_signatures.get(&hash)
                && let Some(sig) = matched.iter().find(|s| s.strong_hash == hash)
            {
                return vec![DeltaCommand::Copy {
                    offset: sig.block_index,
                    length: data_len,
                }];
            }
            return vec![DeltaCommand::Data(new_data.into())];
        }

        let mut i = 0;
        while i + block_size <= data_len {
            let window = &new_data[i..i + block_size];
            let hash = xxh3_64(window);

            if let Some(matched) = old_signatures.get(&hash)
                && let Some(sig) = matched.iter().find(|s| s.strong_hash == hash)
            {
                delta.push(DeltaCommand::Copy {
                    offset: sig.block_index,
                    length: block_size,
                });
                i += block_size;
                continue;
            }

            delta.push(DeltaCommand::Data(window.into()));
            i += block_size;
        }

        if i < data_len {
            let window = &new_data[i..];
            let hash = xxh3_64(window);
            if let Some(matched) = old_signatures.get(&hash)
                && let Some(sig) = matched.iter().find(|s| s.strong_hash == hash)
            {
                delta.push(DeltaCommand::Copy {
                    offset: sig.block_index,
                    length: data_len - i,
                });
            } else {
                delta.push(DeltaCommand::Data(window.into()));
            }
        }

        delta
    }

    /// # Panics
    /// Panics if the delta contains invalid copy commands (out of bounds or overflow).
    #[must_use]
    pub fn apply_delta(&self, base_data: &[u8], delta: &[DeltaCommand]) -> Vec<u8> {
        let mut result = Vec::with_capacity(
            base_data.len()
                + delta
                    .iter()
                    .filter_map(|d| {
                        if let DeltaCommand::Data(data) = d {
                            Some(data.len())
                        } else {
                            None
                        }
                    })
                    .sum::<usize>(),
        );

        for command in delta {
            match command {
                DeltaCommand::Data(data) => result.extend_from_slice(data),
                DeltaCommand::Copy { offset, length } => {
                    let start = *offset * self.config.block_size;
                    let end = start
                        .checked_add(*length)
                        .expect("delta copy range overflowed usize");
                    assert!(
                        end <= base_data.len(),
                        "delta copy range out of bounds (offset {}, length {}, base length {})",
                        start,
                        length,
                        base_data.len()
                    );
                    result.extend_from_slice(&base_data[start..end]);
                }
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_rsync() {
        let rsync = BufferRsync::new(RsyncConfig::default());

        let original = b"Hello, world! This is a test file for rsync.";
        let modified = b"Hello, world! This is a modified test file for rsync.";

        let signatures = rsync.generate_signatures(&original[..]).unwrap();
        let delta = rsync.generate_delta(&signatures, &modified[..]).unwrap();
        let reconstructed = rsync.apply_delta(original, &delta);

        assert_eq!(reconstructed, modified);
    }

    #[test]
    fn test_handles_insertions() {
        let rsync = BufferRsync::new(RsyncConfig { block_size: 8 });

        let original = b"ABCDEFGHabcdefgh";
        let modified = b"ABCXYZDEFGHabcdefgh";

        let signatures = rsync.generate_signatures(&original[..]).unwrap();
        let delta = rsync.generate_delta(&signatures, &modified[..]).unwrap();
        let reconstructed = rsync.apply_delta(original, &delta);

        assert_eq!(reconstructed, modified);
    }

    #[test]
    fn test_unchanged_data() {
        let rsync = BufferRsync::new(RsyncConfig::default());

        let data = b"Hello, world! This is a test file for rsync.";

        let signatures = rsync.generate_signatures(&data[..]).unwrap();
        let delta = rsync.generate_delta(&signatures, &data[..]).unwrap();
        let reconstructed = rsync.apply_delta(data, &delta);

        assert_eq!(reconstructed, data);
    }

    #[test]
    fn test_completely_different_data() {
        let rsync = BufferRsync::new(RsyncConfig::default());

        let original = b"Hello, world!";
        let modified = b"Goodbye, world!";

        let signatures = rsync.generate_signatures(&original[..]).unwrap();
        let delta = rsync.generate_delta(&signatures, &modified[..]).unwrap();
        let reconstructed = rsync.apply_delta(original, &delta);

        assert_eq!(reconstructed, modified);
    }
}
