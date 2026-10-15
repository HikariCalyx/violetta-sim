//! A tiny random number generator, so the game needs no extra dependency.
//!
//! Only two things are random in a round: where the characters appear and where
//! they walk to. Xorshift64* is small, has no state worth persisting, and is far
//! better than this needs.

use std::sync::atomic::{AtomicU64, Ordering};

/// A seed for a new game, mixed from the clock and a run counter so that two
/// games started in the same instant still differ.
pub fn seed(time: f64) -> u64 {
    static RUNS: AtomicU64 = AtomicU64::new(0);
    let run = RUNS.fetch_add(1, Ordering::Relaxed);
    time.to_bits() ^ run.wrapping_mul(0x9E37_79B9_7F4A_7C15)
}

/// The generator used for the character positions and the swaps.
#[derive(Debug, Clone, Copy)]
pub struct Rng(u64);

impl Rng {
    /// Creates a generator from `seed`.
    ///
    /// A zero seed would be a fixed point of the xorshift step, so it is nudged
    /// off it. The first few values are discarded because a xorshift needs a
    /// moment to spread the bits of a small seed.
    pub fn new(seed: u64) -> Self {
        let mut rng = Self(seed | 1);
        rng.next_u64();
        rng.next_u64();
        rng.next_u64();
        rng
    }

    /// A value in `0..bound`.
    pub fn below(&mut self, bound: usize) -> usize {
        let bound = bound.max(1) as u64;
        (self.next_u64() % bound) as usize
    }

    /// Shuffles `items` in place, leaving every order equally likely.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for index in (1..items.len()).rev() {
            let other = self.below(index + 1);
            items.swap(index, other);
        }
    }

    /// The next value of the xorshift64* sequence.
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}
