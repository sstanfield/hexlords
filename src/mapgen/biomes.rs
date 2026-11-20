//! Forest, swamp, desert and grass (`docs/mapgen.md`, stage 4): a moisture
//! from a slow noise and the water near; swamps grown as regions from a few
//! seeds on low wet land by the water (one big, a few small); forests where
//! it is wettest; desert where it is driest, far from water and from swamps;
//! grass the rest.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::state::{Hex, TileType};

use super::Gen;
use super::config::Config;
use super::grid::{Grid, steps_from, top};
use super::noise::{center, fbm};
use super::rng::Rng;

/// How big the wet and dry lands are, in the noise's units.
const MOISTURE: f64 = 26.0;

/// The most swamps a map has.
const MAX_SWAMPS: usize = 6;

/// What the moisture and the land's quotas are.
pub fn biomes(g: &mut Gen, cfg: &Config) {
    let mut rng = Rng::stream(cfg.seed, "biomes");
    let seed = rng.next_u64();
    let (rows, cols) = (g.rows(), g.cols());
    let t = g.terrain.clone();
    let water = |h: Hex| *t.get(h) == TileType::Water;
    let to_water = steps_from(rows, cols, t.hexes().filter(|h| water(*h)), |_| true);
    let f = 1.0 / (MOISTURE * cfg.feature_scale.max(0.05));
    let gain = 0.4 + 0.2 * cfg.roughness;
    g.moisture = Grid::from_fn(rows, cols, |h| {
        let (x, y) = center(h);
        let near = to_water.get(h).map_or(0.0, |d| ((6.0 - f64::from(d)) / 5.0).clamp(0.0, 1.0));
        fbm(seed, x * f, y * f, 4, gain) + 0.25 * near
    });
    let land = t.cells().iter().filter(|k| **k != TileType::Water).count();
    let quota = |share: f64| (share / 100.0 * land as f64).round() as usize;
    swamps(g, &to_water, quota(cfg.mix[4]), &mut rng);
    // Forests: the wettest grass.
    let t = g.terrain.clone();
    let grass: Vec<(usize, f64)> =
        t.hexes().filter(|h| *t.get(*h) == TileType::Grass).map(|h| (t.index(h), *g.moisture.get(h))).collect();
    for i in top(&grass, quota(cfg.mix[3])) {
        g.terrain.cells_mut()[i] = TileType::Forest;
    }
    // Desert: the driest grass, 6 hexes or more from water, not by a swamp.
    let t = g.terrain.clone();
    let dry: Vec<(usize, f64)> = t
        .hexes()
        .filter(|h| *t.get(*h) == TileType::Grass && to_water.get(*h).is_none_or(|d| d >= 6))
        .filter(|h| !t.neighbours(*h).any(|n| *t.get(n) == TileType::Swamp))
        .map(|h| (t.index(h), -g.moisture.get(h)))
        .collect();
    for i in top(&dry, quota(cfg.mix[5])) {
        g.terrain.cells_mut()[i] = TileType::Desert;
    }
}

/// Swamps: grown from one to three seeds where the land is lowest and
/// wettest within 4 hexes of water, the first taking most of the quota.
fn swamps(g: &mut Gen, to_water: &Grid<Option<u32>>, quota: usize, rng: &mut Rng) {
    if quota == 0 {
        return;
    }
    let t = g.terrain.clone();
    let score = |h: Hex| g.moisture.get(h) - g.elevation.get(h);
    // It starts within 4 hexes of water, and grows within 8.
    let fit = |h: Hex| *t.get(h) == TileType::Grass && to_water.get(h).is_some_and(|d| d <= 4);
    let grows = |h: Hex| *t.get(h) == TileType::Grass && to_water.get(h).is_some_and(|d| d <= 8);
    let candidates: Vec<(usize, f64)> = t.hexes().filter(|h| fit(*h)).map(|h| (t.index(h), score(h))).collect();
    // One to three swamps (one, if it is small), more if one cannot grow to
    // its size.
    let mut seeds = if quota < 120 { 1 } else { 1 + rng.below(3) as usize };
    let (rows, cols) = (t.rows(), t.cols());
    let mut left = quota;
    let mut taken: Vec<bool> = vec![false; t.len()];
    let mut started = 0;
    // How far each hex is from the swamps grown so far.
    let mut near: Option<Grid<Option<u32>>> = None;
    for start in top(&candidates, candidates.len()) {
        if started == seeds || left == 0 {
            break;
        }
        // A new swamp keeps away from the others.
        if near.as_ref().is_some_and(|d| d.get(t.hex(start)).is_some_and(|d| d <= 8)) {
            continue;
        }
        started += 1;
        // The first takes most; the others share what is left.
        let size = match started {
            n if n == seeds => left,
            1 => (left * 8).div_ceil(10),
            _ => left / 2,
        };
        // Growth: always the best hex beside the swamp, by its score and how
        // near it is to the seed (so the swamp comes out round).
        let seed_hex = t.hex(start);
        let radius = (size as f64).sqrt().max(1.0);
        let prize = |h: Hex| score(h) - f64::from(h.distance(seed_hex)) / radius * 0.3;
        let mut heap = BinaryHeap::new();
        heap.push((Key(prize(seed_hex)), Reverse(start)));
        let mut grown = 0;
        while let Some((_, Reverse(i))) = heap.pop() {
            if grown >= size {
                break;
            }
            if taken[i] {
                continue;
            }
            taken[i] = true;
            g.terrain.cells_mut()[i] = TileType::Swamp;
            grown += 1;
            for nb in t.neighbours(t.hex(i)) {
                let j = t.index(nb);
                if !taken[j] && grows(nb) {
                    heap.push((Key(prize(nb)), Reverse(j)));
                }
            }
        }
        left -= grown.min(left);
        if grown < size && seeds < MAX_SWAMPS {
            seeds += 1;
        }
        let swamp: Vec<Hex> = t.hexes().filter(|h| taken[t.index(*h)]).collect();
        near = Some(steps_from(rows, cols, swamp, |_| true));
    }
}

/// A score to order by, highest first (f64 by its total order).
#[derive(Copy, Clone, PartialEq)]
struct Key(f64);
impl Eq for Key {}
impl PartialOrd for Key {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Key {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}
