//! Injected randomness. The state machine only ever draws from an [`Rng`], so a seeded generator makes
//! every blink and wander reproducible in tests and in the emulator.

pub trait Rng {
    fn next_u32(&mut self) -> u32;

    /// Uniform in `[0, 1)`.
    fn unit(&mut self) -> f32 {
        // 24 bits is exactly an f32 mantissa, so every value is representable and below 1.0.
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Uniform in `[low, high)`.
    fn between(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * self.unit()
    }
}

/// SplitMix64: tiny, fast, and well distributed for animation purposes. Not cryptographic.
#[derive(Clone, Debug)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }
}

impl Rng for SplitMix64 {
    fn next_u32(&mut self) -> u32 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        ((z ^ (z >> 31)) >> 32) as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_gives_same_sequence() {
        let mut first = SplitMix64::new(42);
        let mut second = SplitMix64::new(42);
        let a: [u32; 8] = core::array::from_fn(|_| first.next_u32());
        let b: [u32; 8] = core::array::from_fn(|_| second.next_u32());
        assert_eq!(a, b);
    }

    #[test]
    fn different_seeds_diverge() {
        assert_ne!(SplitMix64::new(1).next_u32(), SplitMix64::new(2).next_u32());
    }

    #[test]
    fn unit_stays_in_half_open_interval() {
        let mut rng = SplitMix64::new(7);
        assert!((0..10_000).all(|_| (0.0..1.0).contains(&rng.unit())));
    }

    #[test]
    fn between_stays_in_range() {
        let mut rng = SplitMix64::new(9);
        assert!((0..10_000).all(|_| (2.0..5.0).contains(&rng.between(2.0, 5.0))));
    }
}
