//! Cleanup (`docs/mapgen.md`, stage 5): two passes of a majority filter (a
//! hex five or six of whose neighbours are one other terrain becomes it),
//! which leaves water and mountains as they are and keeps some lone trees;
//! then each land terrain is put back on its quota, so the mix holds.

use crate::state::{Hex, TileType};

use super::Gen;
use super::config::Config;
use super::grid::{cheapest_path, pieces, top};
use super::rng::Rng;

/// Lone forest hexes kept, for every thousand forest hexes (scattered trees).
const LONE_TREES: f64 = 40.0;

/// The terrains the filter and the quotas work on: those of the land but
/// mountains.
const SOFT: [TileType; 5] = [TileType::Grass, TileType::Hill, TileType::Forest, TileType::Swamp, TileType::Desert];

/// The ring of hills put back where the rivers took it: grass and forest
/// beside the mountains (those beside the most first) become hills until
/// `hill_ring` of the mountains' edges border hills; as many hills away from
/// the mountains, where the ridge score is lowest, become grass.
fn ring(g: &mut Gen, cfg: &Config) {
    let t = g.terrain.clone();
    let mountain = |h: Hex| *t.get(h) == TileType::Mountain;
    let (mut all, mut covered) = (0usize, 0usize);
    for h in t.hexes().filter(|h| mountain(*h)) {
        for n in t.neighbours(h).filter(|n| !mountain(*n)) {
            all += 1;
            covered += usize::from(*t.get(n) == TileType::Hill);
        }
    }
    let want = (cfg.hill_ring * all as f64).ceil() as usize;
    if covered >= want {
        return;
    }
    let mut ring: Vec<(Hex, usize)> = t
        .hexes()
        .filter(|h| matches!(*t.get(*h), TileType::Grass | TileType::Forest))
        .map(|h| (h, t.neighbours(h).filter(|n| mountain(*n)).count()))
        .filter(|(_, n)| *n > 0)
        .collect();
    ring.sort_by(|a, b| {
        b.1.cmp(&a.1).then(g.ridge.get(b.0).total_cmp(g.ridge.get(a.0))).then(t.index(a.0).cmp(&t.index(b.0)))
    });
    let mut added = 0;
    for (h, edges) in ring {
        if covered >= want {
            break;
        }
        g.terrain.set(h, TileType::Hill);
        covered += edges;
        added += 1;
    }
    let free: Vec<(usize, f64)> = t
        .hexes()
        .filter(|h| *t.get(*h) == TileType::Hill && !t.neighbours(*h).any(mountain))
        .map(|h| (t.index(h), -g.ridge.get(h)))
        .collect();
    for i in top(&free, added) {
        g.terrain.cells_mut()[i] = TileType::Grass;
    }
}

/// The least land walled off on foot that is given a way in (smaller bits
/// of shore behind a range are left).
pub const JOIN_MIN: usize = 12;
/// The most water (not a river) a piece of land is joined across.
pub const JOIN_WATER: usize = 2;

/// Land a walker cannot reach from the main land, walled off by mountains
/// and the sea or a lake (a river is bridged later, so it is no wall), is
/// wasted: no city goes where no army can. Each piece of it gets a way in,
/// biggest first: the range cut where the ridge is lowest (a saddle), the
/// hexes cut hills; or, where only water walls it, across no more than
/// `JOIN_WATER` hexes of it, made grass (a neck of land). Land farther out is
/// an island. The islands raised for cities at sea are left to ships.
pub fn join_land(g: &mut Gen) {
    let (rows, cols) = (g.rows(), g.cols());
    for _ in 0..64 {
        let t = g.terrain.clone();
        let walk = |h: Hex| !matches!(*t.get(h), TileType::Water | TileType::Mountain);
        let (piece, _) = pieces(rows, cols, |h| walk(h) || *g.river.get(h));
        let mut land: Vec<Vec<Hex>> = Vec::new();
        for h in t.hexes().filter(|h| walk(*h) && !*g.island.get(*h)) {
            if let Some(p) = *piece.get(h) {
                if land.len() <= p {
                    land.resize(p + 1, Vec::new());
                }
                land[p].push(h);
            }
        }
        let Some(main) = (0..land.len()).max_by_key(|p| (land[*p].len(), std::cmp::Reverse(*p))) else { return };
        let mut cut_off: Vec<usize> = (0..land.len()).filter(|p| *p != main && land[*p].len() >= JOIN_MIN).collect();
        cut_off.sort_by_key(|p| (std::cmp::Reverse(land[*p].len()), *p));
        let mut joined = false;
        let sea = |h: Hex| *t.get(h) == TileType::Water && !*g.river.get(h);
        for p in cut_off {
            let way = |water: Option<u32>| {
                cheapest_path(
                    rows,
                    cols,
                    &land[p],
                    |h| match *t.get(h) {
                        TileType::Mountain => Some(10 + (g.ridge.get(h) * 20.0) as u32),
                        TileType::Water if *g.river.get(h) => Some(6),
                        TileType::Water => water,
                        _ => Some(0),
                    },
                    |h| walk(h) && *piece.get(h) == Some(main),
                )
            };
            let way =
                way(None).or_else(|| way(Some(40)).filter(|w| w.iter().filter(|h| sea(**h)).count() <= JOIN_WATER));
            if let Some(way) = way {
                for h in way {
                    if *t.get(h) == TileType::Mountain {
                        g.terrain.set(h, TileType::Hill);
                    } else if sea(h) {
                        g.terrain.set(h, TileType::Grass);
                        g.fresh.set(h, false);
                    } else {
                        continue;
                    }
                    g.keep.set(h, true);
                    for n in t.neighbours(h).filter(|n| walk(*n)) {
                        g.keep.set(n, true);
                    }
                }
                joined = true;
                break;
            }
        }
        if !joined {
            return;
        }
    }
}

pub fn cleanup(g: &mut Gen, cfg: &Config) {
    ring(g, cfg);
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
/// one under takes the best grass beside it (or anywhere). A city and the
/// hexes round it are left as they are.
pub fn quotas(g: &mut Gen, cfg: &Config) {
    let snap0 = g.terrain.clone();
    let keep = g.keep.clone();
    let free = |h: Hex| {
        *snap0.get(h) != TileType::City
            && !snap0.neighbours(h).any(|n| *snap0.get(n) == TileType::City)
            && !*keep.get(h)
    };
    let land = g.terrain.cells().iter().filter(|t| **t != TileType::Water).count();
    for (i, t) in [(2, TileType::Hill), (3, TileType::Forest), (4, TileType::Swamp), (5, TileType::Desert)] {
        let want = (cfg.mix[i] / 100.0 * land as f64).round() as usize;
        let snap = g.terrain.clone();
        let have = snap.cells().iter().filter(|k| **k == t).count();
        if have > want {
            let worst: Vec<(usize, f64)> = snap
                .hexes()
                .filter(|h| *snap.get(*h) == t && free(*h))
                .map(|h| (snap.index(h), -score(g, t, h)))
                .collect();
            for j in top(&worst, have - want) {
                g.terrain.cells_mut()[j] = TileType::Grass;
            }
        } else if have < want {
            let beside = |h: Hex| snap.neighbours(h).any(|n| *snap.get(n) == t);
            let mut best: Vec<(usize, f64)> = snap
                .hexes()
                .filter(|h| *snap.get(*h) == TileType::Grass && beside(*h) && free(*h))
                .map(|h| (snap.index(h), score(g, t, h)))
                .collect();
            if best.len() < want - have {
                let more = snap.hexes().filter(|h| *snap.get(*h) == TileType::Grass && !beside(*h) && free(*h));
                best.extend(more.map(|h| (snap.index(h), score(g, t, h) - 10.0)));
            }
            for j in top(&best, want - have) {
                g.terrain.cells_mut()[j] = t;
            }
        }
    }
}
