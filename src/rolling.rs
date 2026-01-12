use std::simd::prelude::*;

const MOD: u32 = 65521;
const BLOCK_SIZE: usize = 32; // 256-bit—maps
#[allow(clippy::cast_possible_truncation)]
const BLOCK_SIZE_U32: u32 = BLOCK_SIZE as u32;
const NMAX: usize = 5552;
const CHUNK_SIZE: usize = (NMAX / BLOCK_SIZE) * BLOCK_SIZE;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct RollingChecksum {
    a: u32,
    b: u32,
    count: usize,
}

impl RollingChecksum {
    #[inline]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            a: 1,
            b: 0,
            count: 0,
        }
    }

    #[inline]
    #[must_use]
    pub const fn value(&self) -> u32 {
        (self.b << 16) | self.a
    }

    pub fn update(&mut self, data: &[u8]) {
        let mut a = self.a;
        let mut b = self.b;

        // Process large chunks, applying modulo only once per chunk
        for chunk in data.chunks(CHUNK_SIZE) {
            process_chunk(&mut a, &mut b, chunk);
            a %= MOD;
            b %= MOD;
        }

        self.a = a;
        self.b = b;
        self.count += data.len();
    }

    #[inline]
    pub fn roll(&mut self, old_byte: u8, new_byte: u8, window_size: usize) {
        let old = u32::from(old_byte);
        let new = u32::from(new_byte);
        #[allow(clippy::cast_possible_truncation)]
        let n = window_size as u32;

        self.a = (self.a + MOD - old + new) % MOD;
        self.b = (self.b + MOD + self.a - 1 - (n * old % MOD)) % MOD;
    }

    #[inline]
    #[must_use]
    pub fn compute(data: &[u8]) -> u32 {
        let mut checksum = Self::new();
        checksum.update(data);
        checksum.value()
    }
}

/// Weights for computing `b`: position 0 gets weight `BLOCK_SIZE`, position 31 gets weight 1
const WEIGHTS: Simd<u32, BLOCK_SIZE> = Simd::from_array([
    32, 31, 30, 29, 28, 27, 26, 25, 24, 23, 22, 21, 20, 19, 18, 17, 16, 15, 14, 13, 12, 11, 10, 9,
    8, 7, 6, 5, 4, 3, 2, 1,
]);

#[inline]
fn process_chunk(a: &mut u32, b: &mut u32, data: &[u8]) {
    let blocks = data.chunks_exact(BLOCK_SIZE);
    let remainder = blocks.remainder();
    #[allow(clippy::cast_possible_truncation)]
    let block_count = blocks.len() as u32;

    // p tracks prefix sums: initial `a` contributes to each of the N blocks
    let mut p = *a * block_count;
    let mut a_vec = Simd::<u32, BLOCK_SIZE>::splat(0);
    let mut b_acc: u32 = *b;

    for block in blocks {
        let v: Simd<u8, BLOCK_SIZE> = Simd::from_slice(block);
        let v32: Simd<u32, BLOCK_SIZE> = v.cast();

        // Accumulate prefix contribution before adding this block's sum
        p += a_vec.reduce_sum();

        // Accumulate byte sums in vector form
        a_vec += v32;

        // Weighted sum for b (position-dependent)
        b_acc += (v32 * WEIGHTS).reduce_sum();
    }

    // Final reduction
    *a += a_vec.reduce_sum();
    *b = b_acc + p * BLOCK_SIZE_U32;

    // Scalar tail for remaining bytes
    for &byte in remainder {
        *a += u32::from(byte);
        *b += *a;
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn adler32_scalar(data: &[u8]) -> u32 {
        let mut a: u32 = 1;
        let mut b: u32 = 0;
        for &byte in data {
            a = (a + u32::from(byte)) % MOD;
            b = (b + a) % MOD;
        }
        (b << 16) | a
    }

    #[test]
    fn test_correctness() {
        let data: Vec<u8> = (0..1_000_000).map(|i| i as u8).collect();
        assert_eq!(RollingChecksum::compute(&data), adler32_scalar(&data));
    }
}
