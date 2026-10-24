//! The fuzzer's own random numbers.
//!
//! SplitMix64, written out here rather than taken from a crate, so that a
//! seed printed by one build reproduces the same cases in every later build:
//! a dependency's generator is free to change its stream between releases.

/// A deterministic stream of random numbers from one seed.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// The stream of case `n` of a run seeded with `seed`: each case can be
    /// made again on its own, whatever ran before it.
    #[must_use]
    pub fn for_case(seed: u64, n: u64) -> Self {
        let mut r = Self(seed ^ n.wrapping_mul(0xA076_1D64_78BD_642F));
        r.next_u64();
        r
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `0..n`; 0 when `n` is 0.
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        (self.next_u64() % n as u64) as usize
    }

    /// A number in `lo..=hi`.
    pub fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + self.below((hi - lo + 1) as usize) as i64
    }

    /// True with probability `percent` in 100.
    pub fn chance(&mut self, percent: u32) -> bool {
        self.below(100) < percent as usize
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seed_gives_the_same_stream() {
        let a: Vec<u64> = (0..4).map(|_| Rng::new(7).next_u64()).collect();
        assert!(a.windows(2).all(|w| w[0] == w[1]));
        let mut r = Rng::for_case(1, 2);
        let mut s = Rng::for_case(1, 2);
        assert_eq!(r.next_u64(), s.next_u64());
        assert_ne!(
            Rng::for_case(1, 2).next_u64(),
            Rng::for_case(1, 3).next_u64()
        );
    }

    #[test]
    fn ranges_stay_inside() {
        let mut r = Rng::new(3);
        for _ in 0..1000 {
            let v = r.range(-2, 5);
            assert!((-2..=5).contains(&v));
        }
        assert_eq!(r.below(0), 0);
    }
}
