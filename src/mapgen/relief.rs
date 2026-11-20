//! Mountains and hills (`docs/mapgen.md`, stage 2): a ridge score on the
//! land, from ridged noise drawn out along a few directions that blend from
//! region to region (ranges run long and bend; `massifs`: plain noise, round
//! lumps); its best hexes are the mountains, as many as the mix asks, never
//! on the coast; around them a ring of hills as much as `hill_ring` asks, and
//! the rest of the hills in free patches.

use crate::state::{Hex, TileType};

use super::Gen;
use super::config::{Config, RangeStyle};
use super::grid::{Grid, cheapest_path, pieces, top};
use super::noise::{center, fbm, ridged};
use super::rng::Rng;

/// The directions a range may run in: whole-number triangles, so turning a
/// point needs no `sin` or `cos`.
const DIRECTIONS: [(f64, f64); 8] =
    [(1.0, 0.0), (0.8, 0.6), (0.6, 0.8), (0.0, 1.0), (-0.6, 0.8), (-0.8, 0.6), (0.28, 0.96), (-0.28, 0.96)];

/// How wide a range's noise is across it, and how much longer along it, in
/// the noise's units at `feature_scale` 1; how big a region of one direction
/// is; how far the ranges are bent about.
const ACROSS: f64 = 31.0;
const ALONG: f64 = 4.0;
const REGION: f64 = 90.0;
const BEND: f64 = 36.0;

/// How much of the ridge score is lumps (clumps of mountains, ranges thick
/// in places), against lines.
const CLUMPS: f64 = 0.25;

/// A hollow in the mountains this small fills with mountain; a bigger pocket
/// walled in by them gets a way out.
const HOLLOW: usize = 6;

/// The patches of mountain smaller than this become hills.
const MIN_RANGE: usize = 4;

/// Noise pushed away from its middle (an average of octaves keeps near it),
/// so its ridges stand out.
fn spread(n: f64) -> f64 {
    ((n - 0.5) * 2.4 + 0.5).clamp(0.0, 1.0)
}

/// The ridge score of every hex: two directions of ranges, each holding
/// sway in its own regions, their lines bent by a slow warp.
pub fn ridge_score(g: &Gen, cfg: &Config) -> Grid<f64> {
    let mut rng = Rng::stream(cfg.seed, "relief");
    let (a, b) = (rng.next_u64(), rng.next_u64());
    let (warp_x, warp_y, sway) = (rng.next_u64(), rng.next_u64(), rng.next_u64());
    let (mask, lumps) = (rng.next_u64(), rng.next_u64());
    let first = rng.below(DIRECTIONS.len() as u64) as usize;
    // The second at least two places round the table from the first.
    let second = (first + 2 + rng.below(DIRECTIONS.len() as u64 - 3) as usize) % DIRECTIONS.len();
    let scale = cfg.feature_scale.max(0.05);
    let f = 1.0 / (ACROSS * scale);
    let gain = 0.4 + 0.2 * cfg.roughness;
    let (fr, fw) = (1.0 / (REGION * scale), 1.0 / (2.5 * REGION * scale));
    Grid::from_fn(g.rows(), g.cols(), |h| {
        let (x, y) = center(h);
        let e = *g.elevation.get(h);
        let score = match cfg.range_style {
            RangeStyle::Massifs => spread(fbm(a, x * f * 1.5, y * f * 1.5, 4, gain)),
            RangeStyle::Ridges => {
                let bend = BEND * scale;
                let wx = x + bend * (fbm(warp_x, x * fw, y * fw, 2, 0.5) - 0.5) * 2.0;
                let wy = y + bend * (fbm(warp_y, x * fw, y * fw, 2, 0.5) - 0.5) * 2.0;
                let along = |seed: u64, (dx, dy): (f64, f64)| {
                    let (u, v) = (wx * dx + wy * dy, -wx * dy + wy * dx);
                    let r = ridged(spread(fbm(seed, u * f / ALONG, v * f, 3, gain)));
                    r * r
                };
                let w = spread(fbm(sway, x * fr, y * fr, 2, 0.5));
                let w = w * w * (3.0 - 2.0 * w);
                let lines = w * along(a, DIRECTIONS[first]) + (1.0 - w) * along(b, DIRECTIONS[second]);
                let clump = spread(fbm(lumps, x * f * 1.2, y * f * 1.2, 3, gain));
                (1.0 - CLUMPS) * lines + CLUMPS * clump * clump
            }
        };
        // Mountain country and plains: a slow mask gathers the ranges.
        let fm = 1.0 / (MOUNTAIN_COUNTRY * scale);
        let country = spread(fbm(mask, x * fm, y * fm, 2, 0.5));
        score * (0.4 + 0.6 * country) * (0.75 + 0.25 * e)
    })
}

/// How big the regions of mountain country and of plain are, in the noise's
/// units.
const MOUNTAIN_COUNTRY: f64 = 70.0;

/// Lay the mountains and hills on the land: their shares are of `land`, the
/// land the map will have when the rivers and lakes have taken theirs.
pub fn relief(g: &mut Gen, cfg: &Config, land_hexes: usize) {
    let score = ridge_score(g, cfg);
    let t = g.terrain.clone();
    let water = |h: Hex| *t.get(h) == TileType::Water;
    let land: Vec<Hex> = t.hexes().filter(|h| !water(*h)).collect();
    let quota = |share: f64| (share / 100.0 * land_hexes as f64).round() as usize;
    let (mountains, hills) = (quota(cfg.mix[1]), quota(cfg.mix[2]));
    // The mountains: the best scores, away from the sea.
    let inland: Vec<(usize, f64)> =
        land.iter().filter(|h| !t.neighbours(**h).any(water)).map(|h| (t.index(*h), *score.get(*h))).collect();
    for i in top(&inland, mountains) {
        g.terrain.cells_mut()[i] = TileType::Mountain;
    }
    // Lone bits of mountain are hills.
    let now = g.terrain.clone();
    let (piece, sizes) = pieces(g.rows(), g.cols(), |h| *now.get(h) == TileType::Mountain);
    for h in now.hexes() {
        if piece.get(h).is_some_and(|p| sizes[p] < MIN_RANGE) {
            g.terrain.set(h, TileType::Hill);
        }
    }
    let placed = g.terrain.cells().iter().filter(|t| **t == TileType::Hill).count();
    let free_seed = Rng::stream(cfg.seed, "hills").next_u64();
    let f = 1.0 / (FREE_HILLS * cfg.feature_scale.max(0.05));
    let free = Grid::from_fn(g.rows(), g.cols(), |h| {
        let (x, y) = center(h);
        fbm(free_seed, x * f, y * f, 4, 0.5)
    });
    ring(g, (&score, &free), cfg.hill_ring, hills.saturating_sub(placed));
    open_pockets(g, &score);
    g.ridge = score;
}

/// Land walled in by mountains is of no use to anyone (the owner's eye, on
/// the judged maps: `tests/mapgen/judged`): a hollow of a few hexes in a range
/// fills with mountain; a bigger pocket gets a way out, its wall cut where
/// the ridge score is lowest (a saddle), the hexes cut hills.
fn open_pockets(g: &mut Gen, score: &Grid<f64>) {
    let (rows, cols) = (g.rows(), g.cols());
    for _ in 0..64 {
        let t = g.terrain.clone();
        let mut pockets = super::stats::pockets(rows, cols, &|h| *t.get(h));
        pockets.sort_by_key(|p| std::cmp::Reverse(p.len()));
        let Some(pocket) = pockets.into_iter().next() else { return };
        if pocket.len() < HOLLOW {
            for h in pocket {
                g.terrain.set(h, TileType::Mountain);
            }
            continue;
        }
        let mut inside: Grid<bool> = Grid::new(rows, cols, false);
        for h in &pocket {
            inside.set(*h, true);
        }
        let way = cheapest_path(
            rows,
            cols,
            &pocket,
            |h| match *t.get(h) {
                TileType::Water => None,
                TileType::Mountain => Some(10 + (score.get(h) * 20.0) as u32),
                _ => Some(0),
            },
            |h| !*inside.get(h) && !matches!(*t.get(h), TileType::Water | TileType::Mountain),
        );
        match way {
            Some(way) => {
                for h in way.into_iter().filter(|h| *t.get(*h) == TileType::Mountain) {
                    g.terrain.set(h, TileType::Hill);
                }
            }
            // Walled in by the sea too: it is hills, a highland.
            None => {
                for h in pocket {
                    g.terrain.set(h, TileType::Hill);
                }
            }
        }
    }
}

/// How big the free patches of hills are, in the noise's units.
const FREE_HILLS: f64 = 20.0;

/// Mountain edges and how many of them border hills.
fn mountain_edges(t: &Grid<TileType>) -> (usize, usize) {
    let (mut all, mut hills) = (0, 0);
    for h in t.hexes().filter(|h| *t.get(*h) == TileType::Mountain) {
        for n in t.neighbours(h).filter(|n| *t.get(*n) != TileType::Mountain) {
            all += 1;
            hills += usize::from(*t.get(n) == TileType::Hill);
        }
    }
    (all, hills)
}

/// The hills: a ring round the mountains, the hexes that border the most of
/// them first, until `share` of their edges border hills (or the hills run
/// out); then the rest in free patches, where the ridge score is next best.
fn ring(g: &mut Gen, (score, free_score): (&Grid<f64>, &Grid<f64>), share: f64, mut hills: usize) {
    let t = g.terrain.clone();
    let mountain = |h: Hex| *t.get(h) == TileType::Mountain;
    let (all, mut covered) = mountain_edges(&t);
    let want = (share * all as f64).ceil() as usize;
    // By the ridge score, which is smooth along a range: what is left out
    // of the ring is left out in stretches, not every other hex.
    let mut ring: Vec<(Hex, usize)> = t
        .hexes()
        .filter(|h| *t.get(*h) == TileType::Grass)
        .map(|h| (h, t.neighbours(h).filter(|n| mountain(*n)).count()))
        .filter(|(_, n)| *n > 0)
        .collect();
    ring.sort_by(|a, b| score.get(b.0).total_cmp(score.get(a.0)).then(t.index(a.0).cmp(&t.index(b.0))));
    for (h, edges) in ring {
        if covered >= want || hills == 0 {
            break;
        }
        g.terrain.set(h, TileType::Hill);
        covered += edges;
        hills -= 1;
    }
    // A second ring, thick where the ridge score is high, with part of what
    // is left.
    let t = g.terrain.clone();
    let beside = |h: Hex, k: TileType| t.neighbours(h).any(|n| *t.get(n) == k);
    let outer: Vec<(usize, f64)> = t
        .hexes()
        .filter(|h| *t.get(*h) == TileType::Grass && beside(*h, TileType::Hill))
        .filter(|h| !beside(*h, TileType::Water) && !beside(*h, TileType::Mountain))
        .map(|h| (t.index(h), *score.get(h)))
        .collect();
    let thick = hills * 2 / 5;
    for i in top(&outer, thick) {
        g.terrain.cells_mut()[i] = TileType::Hill;
    }
    hills -= thick.min(outer.len());
    // Free patches: the best of a noise of their own, as blobs.
    let t = g.terrain.clone();
    let free: Vec<(usize, f64)> = t
        .hexes()
        .filter(|h| *t.get(*h) == TileType::Grass && !t.neighbours(*h).any(|n| *t.get(n) == TileType::Water))
        .map(|h| (t.index(h), *free_score.get(h)))
        .collect();
    for i in top(&free, hills) {
        g.terrain.cells_mut()[i] = TileType::Hill;
    }
}
