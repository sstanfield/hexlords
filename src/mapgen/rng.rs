//! The generator's chance: SplitMix64, small and the same on every machine,
//! in streams: each stage draws from its own, made from the seed and the
//! stage's name, so a change to one stage leaves the others' draws as they
//! were.

/// A stream of chance.
#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
}

/// The FNV-1a hash of a name: a stream's start, with the seed.
fn hash(name: &str) -> u64 {
    name.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

/// SplitMix64's mix: a good scramble of a number.
pub fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

impl Rng {
    /// The stream called `name` of the map with `seed`.
    pub fn stream(seed: u64, name: &str) -> Rng {
        Rng { state: mix(seed ^ mix(hash(name))) }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        mix(self.state)
    }

    /// A number from 0 to `n - 1` (0 if `n` is 0).
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next_u64() % n }
    }

    /// A number from `lo` to `hi`, both included.
    pub fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.below(hi.saturating_sub(lo) + 1)
    }

    /// A number from 0 up to 1, not 1.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Something of a list, if it has anything.
    pub fn pick<'a, T>(&mut self, list: &'a [T]) -> Option<&'a T> {
        list.get(self.below(list.len() as u64) as usize)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streams_are_their_own_and_the_same_every_time() {
        let draws = |seed, name| {
            let mut r = Rng::stream(seed, name);
            (0..4).map(|_| r.next_u64()).collect::<Vec<_>>()
        };
        assert_eq!(draws(7, "land"), draws(7, "land"));
        assert_ne!(draws(7, "land"), draws(7, "relief"));
        assert_ne!(draws(7, "land"), draws(8, "land"));
        // The same numbers on every machine: SplitMix64 is integers only.
        // (Pinned: a change here changes every map a seed makes.)
        assert_eq!(Rng::stream(1, "x").next_u64(), 3_891_469_214_930_438_231);
    }

    #[test]
    fn numbers_fall_where_asked() {
        let mut r = Rng::stream(3, "test");
        for _ in 0..1000 {
            assert!(r.below(6) < 6);
            let x = r.unit();
            assert!((0.0..1.0).contains(&x));
            assert!((2..=5).contains(&r.range(2, 5)));
        }
        assert_eq!(r.below(0), 0);
        assert!(r.pick::<u8>(&[]).is_none());
    }
}
