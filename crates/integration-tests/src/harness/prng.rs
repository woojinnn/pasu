//! SplitMix64 — a tiny, dependency-free, deterministic PRNG.
//!
//! The workspace has no `rand` dependency, and `Math.random`/`Date::now` are
//! unavailable in the broader runtime, so the fuzzer rolls its own. SplitMix64
//! is fast, has good statistical quality for fuzzing, and is fully reproducible
//! from a `u64` seed — a failing fuzz case can be replayed from its
//! `(callkey, seed)` pair.

/// A reproducible 64-bit PRNG (Vigna's SplitMix64).
#[derive(Clone, Debug)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// Construct from a seed.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Next pseudo-random `u64`.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniformly-ish in `[0, n)`. Returns `0` when `n == 0`.
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next_u64() % n
        }
    }

    /// Random `bool`.
    pub fn boolean(&mut self) -> bool {
        self.next_u64() & 1 == 1
    }

    /// Fill `buf` with pseudo-random bytes.
    pub fn fill_bytes(&mut self, buf: &mut [u8]) {
        for chunk in buf.chunks_mut(8) {
            let word = self.next_u64().to_le_bytes();
            let n = chunk.len();
            chunk.copy_from_slice(&word[..n]);
        }
    }

    /// Allocate `n` pseudo-random bytes.
    pub fn bytes(&mut self, n: usize) -> Vec<u8> {
        let mut v = vec![0u8; n];
        self.fill_bytes(&mut v);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::SplitMix64;

    #[test]
    fn deterministic_for_same_seed() {
        let mut a = SplitMix64::new(42);
        let mut b = SplitMix64::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = SplitMix64::new(1);
        let mut b = SplitMix64::new(2);
        assert_ne!(a.next_u64(), b.next_u64());
    }

    #[test]
    fn below_respects_bound() {
        let mut r = SplitMix64::new(7);
        for _ in 0..1000 {
            assert!(r.below(10) < 10);
        }
        assert_eq!(r.below(0), 0);
    }
}
