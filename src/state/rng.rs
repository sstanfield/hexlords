//! The game's one source of chance: a small random number generator whose
//! state is part of the game, so that the same seed and the same orders give
//! the same game on every machine (`docs/gameplay.md`: the rules are
//! deterministic). SplitMix64: small, fast, and good enough for a game.

/// A seeded random number generator.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng { state: seed }
    }

    /// Where it is in its numbers: two that are at the same place give the
    /// same numbers from here on.
    pub fn state(&self) -> u64 {
        self.state
    }

    /// The next number, any of the 2^64.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number from 0 to `n - 1`, each as likely as the others (0 if `n` is 0).
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            return 0;
        }
        // Draws past the last whole multiple of `n` would favour the small numbers.
        let zone = u64::MAX - u64::MAX % n;
        loop {
            let x = self.next_u64();
            if x < zone {
                return x % n;
            }
        }
    }

    /// Put `items` in a new order, each order as likely as any other.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i as u64 + 1) as usize;
            items.swap(i, j);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_seed_always_gives_the_same_numbers() {
        let (mut a, mut b) = (Rng::new(7), Rng::new(7));
        let xs: Vec<u64> = (0..20).map(|_| a.next_u64()).collect();
        let ys: Vec<u64> = (0..20).map(|_| b.next_u64()).collect();
        assert_eq!(xs, ys);
        let mut c = Rng::new(8);
        assert_ne!(xs[0], c.next_u64());
    }

    #[test]
    fn below_stays_below_and_reaches_every_number() {
        let mut rng = Rng::new(1);
        let mut seen = [0u32; 6];
        for _ in 0..6000 {
            let x = rng.below(6);
            seen[x as usize] += 1;
        }
        // Each about a thousand times.
        assert!(seen.iter().all(|n| (850..1150).contains(n)), "{seen:?}");
        assert_eq!(rng.below(0), 0);
        assert_eq!(rng.below(1), 0);
    }

    #[test]
    fn a_shuffle_keeps_every_item_and_moves_some() {
        let mut rng = Rng::new(3);
        let mut items: Vec<u32> = (0..10).collect();
        rng.shuffle(&mut items);
        let mut sorted = items.clone();
        sorted.sort();
        assert_eq!(sorted, (0..10).collect::<Vec<_>>());
        assert_ne!(items, (0..10).collect::<Vec<_>>());
        let mut one = [5];
        rng.shuffle(&mut one);
        assert_eq!(one, [5]);
    }
}
