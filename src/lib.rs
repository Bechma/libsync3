#![feature(portable_simd)]

pub mod rolling;

use std::collections::HashMap;
use std::io::{BufWriter, Read, Seek, SeekFrom, Write};
use twox_hash::XxHash3_128;
use rolling::RollingChecksum;

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

#[derive(Clone, Debug, Default)]
pub struct Signatures {
    weak_to_strong: HashMap<u32, Vec<(u128, usize)>>,
    block_size: usize,
}

impl Signatures {
    #[must_use]
    pub fn new(block_size: usize) -> Self {
        Self {
            weak_to_strong: HashMap::new(),
            block_size,
        }
    }

    pub fn insert(&mut self, weak: u32, strong: u128, block_index: usize) {
        self.weak_to_strong
            .entry(weak)
            .or_default()
            .push((strong, block_index));
    }

    #[must_use]
    pub fn get(&self, weak: u32, strong: u128) -> Option<usize> {
        self.weak_to_strong.get(&weak).and_then(|entries| {
            entries
                .iter()
                .find(|(s, _)| *s == strong)
                .map(|(_, idx)| *idx)
        })
    }

    #[must_use]
    pub fn contains_weak(&self, weak: u32) -> bool {
        self.weak_to_strong.contains_key(&weak)
    }

    #[must_use]
    pub fn block_size(&self) -> usize {
        self.block_size
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.weak_to_strong.values().map(Vec::len).sum()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.weak_to_strong.is_empty()
    }
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum DeltaCommand {
    Data(Vec<u8>),
    Copy { offset: u64, length: usize },
}

const DEFAULT_BLOCK_SIZE: usize = 4096;

/// Generate signatures from a reader.
///
/// # Errors
/// Returns an error if reading from the reader fails.
pub fn generate_signatures<R: Read>(reader: R) -> std::io::Result<Signatures> {
    generate_signatures_with_block_size(reader, DEFAULT_BLOCK_SIZE)
}

/// Generate signatures from a reader.
///
/// # Errors
/// Returns an error if reading from the reader fails.
pub fn generate_signatures_with_block_size<R: Read>(mut reader: R, block_size: usize) -> std::io::Result<Signatures> {
    let mut signatures = Signatures::new(block_size);
    let mut buffer = vec![0u8; block_size];

    for block_index in 0.. {
        let bytes_read = read_exact_or_eof(&mut reader, &mut buffer)?;
        if bytes_read == 0 {
            break;
        }

        let chunk = &buffer[..bytes_read];
        let weak = RollingChecksum::compute(chunk);
        let strong = xxh3_128(chunk);
        signatures.insert(weak, strong, block_index);
    }

    Ok(signatures)
}

/// Generate delta from signatures and a reader containing new data.
/// Uses a rolling checksum to efficiently find matching blocks at any offset.
/// Reads data in chunks to avoid loading the entire input into memory.
///
/// # Errors
/// Returns an error if reading from the reader fails.
pub fn generate_delta<R: Read>(
    old_signatures: &Signatures,
    reader: R,
) -> std::io::Result<Vec<DeltaCommand>> {
    generate_delta_with_block_size(old_signatures, reader, DEFAULT_BLOCK_SIZE)
}

/// Generate delta from signatures and a reader containing new data.
/// Uses a rolling checksum to efficiently find matching blocks at any offset.
/// Reads data in chunks to avoid loading the entire input into memory.
///
/// # Errors
/// Returns an error if reading from the reader fails or if `block_size` does not match `old_signatures.block_size()`.
pub fn generate_delta_with_block_size<R: Read>(
    old_signatures: &Signatures,
    mut reader: R,
    block_size: usize,
) -> std::io::Result<Vec<DeltaCommand>> {
    if old_signatures.block_size() != block_size {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "block_size does not match signatures",
        ));
    }

    let mut delta = Vec::new();
    let mut pending_data: Vec<u8> = Vec::new();

    let mut window = vec![0u8; block_size * 2];
    let mut window_start = 0;
    let mut window_len;

    let initial_read = read_exact_or_eof(&mut reader, &mut window[..block_size])?;
    if initial_read == 0 {
        return Ok(Vec::new());
    }
    window_len = initial_read;

    if initial_read < block_size {
        let weak = RollingChecksum::compute(&window[..initial_read]);
        let strong = xxh3_128(&window[..initial_read]);
        if let Some(block_idx) = old_signatures.get(weak, strong) {
            return Ok(vec![DeltaCommand::Copy {
                offset: (block_idx * block_size) as u64,
                length: initial_read,
            }]);
        }
        return Ok(vec![DeltaCommand::Data(window[..initial_read].to_vec())]);
    }

    let mut rolling = RollingChecksum::new();
    rolling.update(&window[..block_size]);

    loop {
        while window_len - window_start >= block_size {
            let weak = rolling.value();
            let win_end = window_start + block_size;

            if old_signatures.contains_weak(weak) {
                let current_window = &window[window_start..win_end];
                let strong = xxh3_128(current_window);

                if let Some(block_idx) = old_signatures.get(weak, strong) {
                    if !pending_data.is_empty() {
                        delta.push(DeltaCommand::Data(std::mem::take(&mut pending_data)));
                    }

                    let new_offset = (block_idx * block_size) as u64;
                    if let Some(DeltaCommand::Copy { offset, length }) = delta.last_mut() {
                        if *offset + (*length as u64) == new_offset {
                            *length += block_size;
                        } else {
                            delta.push(DeltaCommand::Copy {
                                offset: new_offset,
                                length: block_size,
                            });
                        }
                    } else {
                        delta.push(DeltaCommand::Copy {
                            offset: new_offset,
                            length: block_size,
                        });
                    }

                    window_start += block_size;

                    if window_len - window_start >= block_size {
                        rolling = RollingChecksum::new();
                        rolling.update(&window[window_start..window_start + block_size]);
                    }
                    continue;
                }
            }

            pending_data.push(window[window_start]);
            let old_byte = window[window_start];
            window_start += 1;

            if window_len - window_start >= block_size {
                let new_byte = window[window_start + block_size - 1];
                rolling.roll(old_byte, new_byte, block_size);
            }
        }

        if window_start > 0 {
            let remaining = window_len - window_start;
            window.copy_within(window_start..window_len, 0);
            window_len = remaining;
            window_start = 0;
        }

        let bytes_read = read_exact_or_eof(&mut reader, &mut window[window_len..block_size * 2])?;
        if bytes_read == 0 {
            break;
        }

        let old_window_len = window_len;
        window_len += bytes_read;

        if old_window_len < block_size && window_len >= block_size {
            rolling = RollingChecksum::new();
            rolling.update(&window[window_start..window_start + block_size]);
        }
    }

    pending_data.extend_from_slice(&window[window_start..window_len]);
    if !pending_data.is_empty() {
        delta.push(DeltaCommand::Data(pending_data));
    }

    Ok(delta)
}

/// # Errors
/// Returns an error if the delta contains invalid copy commands (out of bounds or overflow) or if IO operations fail.
pub fn apply_delta<R: Read + Seek, W: Write>(
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
                let start = *offset;

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
