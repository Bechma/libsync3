const MOD: u32 = 65521;

pub struct RollingChecksum {
    a: u32,
    b: u32,
    adler32: simd_adler32::imp::Adler32Imp,
}

impl Default for RollingChecksum {
    fn default() -> Self {
        Self::new()
    }
}

impl RollingChecksum {
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self {
            a: 1,
            b: 0,
            adler32: simd_adler32::imp::get_imp(),
        }
    }

    #[inline]
    #[must_use]
    pub fn value(&self) -> u32 {
        (self.b % MOD) << 16 | (self.a % MOD)
    }

    #[allow(clippy::cast_possible_truncation)]
    #[inline]
    pub fn update(&mut self, data: &[u8]) {
        let (a, b) = (self.adler32)(self.a as u16, self.b as u16, data);
        (self.a, self.b) = (u32::from(a), u32::from(b));
    }

    #[inline]
    pub fn roll(&mut self, old_byte: u8, new_byte: u8, window_size: usize) {
        let old = u32::from(old_byte);
        let new = u32::from(new_byte);
        #[allow(clippy::cast_possible_truncation)]
        let n = window_size as u32;

        // Use wrapping arithmetic and defer modulo to value() for better performance
        self.a = self.a.wrapping_sub(old).wrapping_add(new);
        self.b = self
            .b
            .wrapping_sub(n.wrapping_mul(old))
            .wrapping_add(self.a)
            .wrapping_sub(1);
    }

    #[inline]
    pub const fn reset(&mut self) {
        (self.a, self.b) = (1, 0);
    }

    #[inline]
    #[must_use]
    pub fn compute(data: &[u8]) -> u32 {
        simd_adler32::adler32(&data)
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
