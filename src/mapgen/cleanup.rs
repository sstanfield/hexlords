//! Cleanup (`docs/mapgen.md`, stage 5): two passes of a majority filter (a
//! hex five or six of whose neighbours are one other terrain becomes it),
//! which leaves water and mountains as they are and keeps some lone trees;
//! then each land terrain is put back on its quota, so the mix holds.

use crate::state::{Hex, TileType};

use super::Gen;
use super::config::Config;
use super::grid::top;
use super::rng::Rng;

/// Lone forest hexes kept, for every thousand forest hexes (scattered trees).
const LONE_TREES: f64 = 40.0;

/// The terrains the filter and the quotas work on: those of the land but
/// mountains.
const SOFT: [TileType; 5] = [TileType::Grass, TileType::Hill, TileType::Forest, TileType::Swamp, TileType::Desert];

pub fn cleanup(g: &mut Gen, cfg: &Config) {
    let mut rng = Rng::stream(cfg.seed, "cleanup");
    let forest = g.terrain.cells().iter().filter(|t| **t == TileType::Forest).count();
    let mut lone_left = (LONE_TREES * forest as f64 / 1000.0).round() as usize;
    for _ in 0..2 {
        let t = g.terrain.clone();
        for h in t.hexes() {
            let here = *t.get(h);
            if !SOFT.contains(&here) {
                continue;
            }
            let around: Vec<TileType> = t.neighbours(h).map(|n| *t.get(n)).collect();
            let Some(&major) =
                SOFT.iter().filter(|k| **k != here).max_by_key(|k| around.iter().filter(|a| *a == *k).count())
            else {
                continue;
            };
            if around.iter().filter(|a| **a == major).count() < 5 {
                continue;
            }
            // A lone tree may stay.
            let lone = here == TileType::Forest && !around.contains(&TileType::Forest);
            if lone && lone_left > 0 && rng.below(2) == 0 {
                lone_left -= 1;
                continue;
            }
            g.terrain.set(h, major);
        }
    }
    quotas(g, cfg);
}

/// What each terrain's place is scored by, to be added or taken away.
fn score(g: &Gen, t: TileType, h: Hex) -> f64 {
    match t {
        TileType::Forest => *g.moisture.get(h),
        TileType::Swamp => g.moisture.get(h) - g.elevation.get(h),
        TileType::Desert => -g.moisture.get(h),
        TileType::Hill => g.ridge.get(h) + 0.5,
        _ => 0.0,
    }
}

/// Each land terrain on its quota: one over gives its worst hexes to grass;
/// one under takes the best grass beside it (or anywhere).
pub fn quotas(g: &mut Gen, cfg: &Config) {
    let land = g.terrain.cells().iter().filter(|t| **t != TileType::Water).count();
    for (i, t) in [(2, TileType::Hill), (3, TileType::Forest), (4, TileType::Swamp), (5, TileType::Desert)] {
        let want = (cfg.mix[i] / 100.0 * land as f64).round() as usize;
        let snap = g.terrain.clone();
        let have = snap.cells().iter().filter(|k| **k == t).count();
        if have > want {
            let worst: Vec<(usize, f64)> =
                snap.hexes().filter(|h| *snap.get(*h) == t).map(|h| (snap.index(h), -score(g, t, h))).collect();
            for j in top(&worst, have - want) {
                g.terrain.cells_mut()[j] = TileType::Grass;
            }
        } else if have < want {
            let beside = |h: Hex| snap.neighbours(h).any(|n| *snap.get(n) == t);
            let mut best: Vec<(usize, f64)> = snap
                .hexes()
                .filter(|h| *snap.get(*h) == TileType::Grass && beside(*h))
                .map(|h| (snap.index(h), score(g, t, h)))
                .collect();
            if best.len() < want - have {
                let more = snap.hexes().filter(|h| *snap.get(*h) == TileType::Grass && !beside(*h));
                best.extend(more.map(|h| (snap.index(h), score(g, t, h) - 10.0)));
            }
            for j in top(&best, want - have) {
                g.terrain.cells_mut()[j] = t;
            }
        }
    }
}
