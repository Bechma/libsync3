#![feature(portable_simd)]

pub mod rolling;

use rolling::RollingChecksum;
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

#[derive(Clone, Debug, Default)]
pub struct Signatures {
    block_size: usize,
    weak_to_strong: HashMap<u32, Vec<(u128, usize)>>,
}

impl Signatures {
    #[must_use]
    pub fn new(block_size: usize) -> Self {
        Self {
            block_size,
            weak_to_strong: HashMap::new(),
        }
    }

    pub fn insert(&mut self, weak: u32, strong: u128, block_index: usize) {
        self.weak_to_strong
            .entry(weak)
            .or_default()
            .push((strong, block_index));
    }

    #[must_use]
    pub fn weak(&self, weak: u32) -> Option<&Vec<(u128, usize)>> {
        self.weak_to_strong.get(&weak)
    }

    #[must_use]
    pub fn from(&self, data: &[u8]) -> Option<usize> {
        let weak = RollingChecksum::compute(data);
        self.weak_to_strong.get(&weak).and_then(|entries| {
            let strong = xxh3_128(data);
            find_strong_hash(entries, strong)
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

fn find_strong_hash(entries: &[(u128, usize)], strong: u128) -> Option<usize> {
    entries
        .iter()
        .find(|(s, _)| *s == strong)
        .map(|(_, idx)| *idx)
}

fn flush_pending_data(delta: &mut Vec<DeltaCommand>, pending_data: &mut Vec<u8>) {
    if !pending_data.is_empty() {
        delta.push(DeltaCommand::Data(std::mem::take(pending_data)));
    }
}

fn push_or_merge_copy(delta: &mut Vec<DeltaCommand>, new_offset: u64, length: usize) {
    if let Some(DeltaCommand::Copy {
        offset,
        length: last_length,
    }) = delta.last_mut()
        && *offset + (*last_length as u64) == new_offset
    {
        *last_length += length;
        return;
    }

    delta.push(DeltaCommand::Copy {
        offset: new_offset,
        length,
    });
}

fn reset_rolling(
    rolling: &mut RollingChecksum,
    window: &[u8],
    window_start: usize,
    block_size: usize,
) {
    *rolling = RollingChecksum::new();
    rolling.update(&window[window_start..window_start + block_size]);
}

fn emit_copy_for_block_idx(
    delta: &mut Vec<DeltaCommand>,
    pending_data: &mut Vec<u8>,
    block_idx: usize,
    block_size: usize,
    length: usize,
) {
    flush_pending_data(delta, pending_data);
    let new_offset = (block_idx * block_size) as u64;
    push_or_merge_copy(delta, new_offset, length);
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
pub fn generate_signatures_with_block_size<R: Read>(
    mut reader: R,
    block_size: usize,
) -> std::io::Result<Signatures> {
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
    mut reader: R,
) -> std::io::Result<Vec<DeltaCommand>> {
    let block_size = old_signatures.block_size();

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
        if let Some(block_idx) = old_signatures.from(&window[..initial_read]) {
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

            if let Some(entries) = old_signatures.weak(weak) {
                let current_window = &window[window_start..win_end];
                let strong = xxh3_128(current_window);

                if let Some(block_idx) = find_strong_hash(entries, strong) {
                    emit_copy_for_block_idx(
                        &mut delta,
                        &mut pending_data,
                        block_idx,
                        block_size,
                        block_size,
                    );

                    window_start += block_size;

                    if window_len - window_start >= block_size {
                        reset_rolling(&mut rolling, &window, window_start, block_size);
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
            reset_rolling(&mut rolling, &window, window_start, block_size);
        }
    }

    let remaining = &window[window_start..window_len];
    if !remaining.is_empty() {
        if let Some(block_idx) = old_signatures.from(remaining) {
            emit_copy_for_block_idx(
                &mut delta,
                &mut pending_data,
                block_idx,
                block_size,
                remaining.len(),
            );
        } else {
            pending_data.extend_from_slice(remaining);
        }
    }

    flush_pending_data(&mut delta, &mut pending_data);

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
