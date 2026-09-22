//! The game's random number generator: two 31-bit shift registers, each stepped 31
//! times per draw. Every consumer takes explicit draws from a `Rng` it is handed, so
//! the order of draws is visible in the code.

pub const POOL_SIZE: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Rng {
    iv1: u32,
    iv2: u32,
    /// The last 100 7-bit values, used where the original draws "from the pool".
    pool: Vec<u8>,
    pool_index: usize,
}

impl Default for Rng {
    fn default() -> Self {
        Self::from_seed(0x5465_7687, 0x7264_1663)
    }
}

impl Rng {
    pub fn from_seed(iv1: u32, iv2: u32) -> Self {
        Self {
            iv1,
            iv2,
            pool: vec![0; POOL_SIZE],
            pool_index: 0,
        }
    }

    /// Value `index` places ahead in the pool of recent draws.
    pub fn from_pool(&self, index: usize) -> i32 {
        self.pool[(self.pool_index + index) % POOL_SIZE] as i32
    }

    /// Refills the whole pool with fresh draws.
    pub fn generate_pool(&mut self) {
        self.pool_index = 0;
        for _ in 0..POOL_SIZE {
            self.next();
        }
    }

    /// Advances both registers.
    pub fn next(&mut self) {
        self.pool[self.pool_index] = self.byte() as u8;
        self.pool_index = (self.pool_index + 1) % POOL_SIZE;
        for _ in 0..31 {
            let r1 = ((self.iv1 & 0x10) >> 4 ^ self.iv1) & 1;
            let r2 = ((self.iv2 & 0x10) >> 4 ^ self.iv2) & 1;
            self.iv1 >>= 1;
            self.iv2 >>= 1;
            if r1 != 0 {
                self.iv1 |= 0x4000_0000;
            }
            if r2 != 0 {
                self.iv2 |= 0x4000_0000;
            }
        }
    }

    /// Low 7 bits of the first register (0..=127), as the original's `random_byte`.
    pub fn byte(&self) -> i32 {
        (self.iv1 & 0x7f) as i32
    }

    /// Low 7 bits of the second register.
    pub fn byte_alt(&self) -> i32 {
        (self.iv2 & 0x7f) as i32
    }

    /// Low 15 bits of the first register.
    pub fn short(&self) -> i32 {
        (self.iv1 & 0x7fff) as i32
    }

    /// Advances, then returns a value in `0..n`.
    pub fn below(&mut self, n: i32) -> i32 {
        self.next();
        if n <= 0 { 0 } else { self.short() % n }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_sequence() {
        let mut a = Rng::default();
        let mut b = Rng::default();
        for _ in 0..100 {
            a.next();
            b.next();
        }
        assert_eq!(a, b);
        assert!(a.byte() < 128 && a.short() < 32768);
    }
}
