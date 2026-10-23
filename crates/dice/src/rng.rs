//! The game's one source of randomness: a small seeded generator (SplitMix64),
//! so the same seed and the same moves always give the same game.

#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform in 0..n. `n` must be positive.
    pub fn below(&mut self, n: usize) -> usize {
        (self.unit() * n as f64) as usize
    }

    /// An index drawn in proportion to `weights`. Falls back to uniform when
    /// every weight is zero.
    pub fn weighted(&mut self, weights: &[f64]) -> usize {
        let total: f64 = weights.iter().sum();
        if total <= 0.0 {
            return self.below(weights.len());
        }
        let mut r = self.unit() * total;
        for (i, &w) in weights.iter().enumerate() {
            if r < w {
                return i;
            }
            r -= w;
        }
        // Rounding can leave r a hair above the last bucket.
        weights.iter().rposition(|&w| w > 0.0).unwrap()
    }
}
