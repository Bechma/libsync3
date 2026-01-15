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
#[must_use]
pub fn xxh3_128(chunk: &[u8]) -> u128 {
    XxHash3_128::oneshot(chunk)
}

#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SignatureStrong {
    pub strong: u128,
    pub block_index: usize,
}

pub type SignatureWeak = u32;

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Signatures {
    block_size: usize,
    weak_to_strong: HashMap<SignatureWeak, Vec<SignatureStrong>>,
}

impl Signatures {
    #[must_use]
    pub fn new(block_size: usize) -> Self {
        Self {
            block_size,
            weak_to_strong: HashMap::new(),
        }
    }

    #[inline]
    pub fn extend(&mut self, new_mapping: HashMap<SignatureWeak, Vec<SignatureStrong>>) {
        self.weak_to_strong.extend(new_mapping);
    }

    #[inline]
    pub fn insert(&mut self, weak: SignatureWeak, strong: SignatureStrong) {
        self.weak_to_strong.entry(weak).or_default().push(strong);
    }

    #[inline]
    #[must_use]
    pub fn weak(&self, weak: SignatureWeak) -> Option<&Vec<SignatureStrong>> {
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

    #[inline]
    #[must_use]
    pub fn block_size(&self) -> usize {
        self.block_size
    }

    #[inline]
    #[must_use]
    pub fn len(&self) -> usize {
        self.weak_to_strong.values().map(Vec::len).sum()
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.weak_to_strong.is_empty()
    }
}

#[inline]
fn find_strong_hash(entries: &[SignatureStrong], strong_hash: u128) -> Option<usize> {
    for entry in entries {
        if entry.strong == strong_hash {
            return Some(entry.block_index);
        }
    }
    None
}

#[inline]
fn flush_pending_data<F: FnMut(DeltaCommand) -> std::io::Result<()>>(
    last_copy: &mut Option<(u64, usize)>,
    pending_data: &mut Vec<u8>,
    cb: &mut F,
) -> std::io::Result<()> {
    if !pending_data.is_empty() {
        flush_last_copy(last_copy, cb)?;
        cb(DeltaCommand::Data(std::mem::take(pending_data)))?;
    }
    Ok(())
}

#[inline]
fn flush_last_copy<F: FnMut(DeltaCommand) -> std::io::Result<()>>(
    last_copy: &mut Option<(u64, usize)>,
    cb: &mut F,
) -> std::io::Result<()> {
    if let Some((offset, length)) = last_copy.take() {
        cb(DeltaCommand::Copy { offset, length })?;
    }
    Ok(())
}

#[inline]
fn push_or_merge_copy<F: FnMut(DeltaCommand) -> std::io::Result<()>>(
    last_copy: &mut Option<(u64, usize)>,
    new_offset: u64,
    length: usize,
    cb: &mut F,
) -> std::io::Result<()> {
    if let Some((offset, last_length)) = last_copy.as_mut() {
        if *offset + (*last_length as u64) == new_offset {
            *last_length += length;
            return Ok(());
        }
        cb(DeltaCommand::Copy {
            offset: *offset,
            length: *last_length,
        })?;
    }
    *last_copy = Some((new_offset, length));
    Ok(())
}

#[inline]
fn reset_rolling(
    rolling: &mut RollingChecksum,
    window: &[u8],
    window_start: usize,
    block_size: usize,
) {
    rolling.reset();
    rolling.update(&window[window_start..window_start + block_size]);
}

#[inline]
fn emit_copy_for_block_idx<F: FnMut(DeltaCommand) -> std::io::Result<()>>(
    last_copy: &mut Option<(u64, usize)>,
    pending_data: &mut Vec<u8>,
    block_idx: usize,
    block_size: usize,
    length: usize,
    cb: &mut F,
) -> std::io::Result<()> {
    flush_pending_data(last_copy, pending_data, cb)?;
    let new_offset = (block_idx * block_size) as u64;
    push_or_merge_copy(last_copy, new_offset, length, cb)
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
    let mut rolling = RollingChecksum::new();

    for block_index in 0.. {
        rolling.reset();
        let bytes_read = read_exact_or_eof(&mut reader, &mut buffer)?;
        if bytes_read == 0 {
            break;
        }

        let chunk = &buffer[..bytes_read];
        rolling.update(chunk);
        let weak = rolling.value();
        let strong = xxh3_128(chunk);
        signatures.insert(
            weak,
            SignatureStrong {
                strong,
                block_index,
            },
        );
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
    let mut result = Vec::new();
    generate_delta_with_cb(old_signatures, reader, |cmd| {
        result.push(cmd);
        Ok(())
    })?;
    Ok(result)
}

/// Same as `generate_delta`, but allows for custom callback when a new delta is located.
///
/// # Errors
/// Returns an error if the callback returns an error or if reading from the reader fails.
pub fn generate_delta_with_cb<R: Read, F: FnMut(DeltaCommand) -> std::io::Result<()>>(
    old_signatures: &Signatures,
    mut reader: R,
    mut cb: F,
) -> std::io::Result<()> {
    let block_size = old_signatures.block_size();
    let buffer_size = block_size * 2;

    let mut last_copy: Option<(u64, usize)> = None;
    let mut pending_data: Vec<u8> = Vec::new();

    let mut window = vec![0u8; buffer_size];
    let mut window_start = 0;
    let mut window_len;

    let initial_read = read_exact_or_eof(&mut reader, &mut window[..block_size])?;
    if initial_read == 0 {
        return Ok(());
    }
    window_len = initial_read;

    if initial_read < block_size {
        if let Some(block_idx) = old_signatures.from(&window[..initial_read]) {
            cb(DeltaCommand::Copy {
                offset: (block_idx * block_size) as u64,
                length: initial_read,
            })?;
            return Ok(());
        }
        cb(DeltaCommand::Data(window[..initial_read].to_vec()))?;
        return Ok(());
    }

    let mut rolling = RollingChecksum::new();
    rolling.update(&window[..block_size]);

    loop {
        while window_len - window_start >= block_size {
            let weak = rolling.value();

            if let Some(entries) = old_signatures.weak(weak) {
                let strong = xxh3_128(&window[window_start..window_start + block_size]);

                if let Some(block_idx) = find_strong_hash(entries, strong) {
                    emit_copy_for_block_idx(
                        &mut last_copy,
                        &mut pending_data,
                        block_idx,
                        block_size,
                        block_size,
                        &mut cb,
                    )?;

                    window_start += block_size;

                    if window_len - window_start >= block_size {
                        reset_rolling(&mut rolling, &window, window_start, block_size);
                    }
                    continue;
                }
            }

            let old_byte = window[window_start];
            pending_data.push(old_byte);
            window_start += 1;

            if window_len - window_start >= block_size {
                rolling.roll(old_byte, window[window_start + block_size - 1], block_size);
            }
        }

        if window_start > 0 {
            let remaining = window_len - window_start;
            window.copy_within(window_start..window_len, 0);
            window_len = remaining;
            window_start = 0;
        }

        let bytes_read = read_exact_or_eof(&mut reader, &mut window[window_len..buffer_size])?;
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
                &mut last_copy,
                &mut pending_data,
                block_idx,
                block_size,
                remaining.len(),
                &mut cb,
            )?;
        } else {
            pending_data.extend_from_slice(remaining);
        }
    }

    flush_pending_data(&mut last_copy, &mut pending_data, &mut cb)?;
    flush_last_copy(&mut last_copy, &mut cb)?;

    Ok(())
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
