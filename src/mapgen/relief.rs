//! Mountains and hills (`docs/mapgen.md`, stage 2): a ridge score on the
//! land, from ridged noise drawn out along a few directions that blend from
//! region to region (ranges run long and bend; `massifs`: plain noise, round
//! lumps); its best hexes are the mountains, never on the coast, thick
//! masses thinned to their spines (no mountain deeper in a range than
//! `DEPTH`), as many as the mix asks; around them a ring of hills as much as
//! `hill_ring` asks, and the rest of the hills in free patches.

use crate::state::{Hex, HexSide, TileType};

use super::Gen;
use super::config::{Config, RangeStyle};
use super::grid::{Grid, cheapest_path, pieces, steps_from, top};
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

/// How deep in a range a mountain may lie: no farther than this from a hex
/// that is not mountain, so a range is at most about twice it thick. The
/// owner found thick masses of mountain odd to look at and wasted land.
pub const DEPTH: u32 = 3;

/// The hexes of the map within `r` of `c`.
fn within(t: &Grid<TileType>, c: Hex, r: u32) -> impl Iterator<Item = Hex> + use<> {
    let (rows, cols) = (t.rows(), t.cols());
    let ri = r as i32;
    (-ri..=ri).flat_map(move |dr| {
        (-ri..=ri).filter_map(move |dc| {
            Hex::on_map(i32::from(c.row) + dr, i32::from(c.col) + dc, rows, cols).filter(|h| h.distance(c) <= r)
        })
    })
}

/// True if a mountain at `h` can go without breaking its range or walling
/// anything in: its mountain neighbours are one run round it, of two to five
/// (one is the tip of a range, which keeps its length). Past the map's edge
/// counts as mountain: a hex left there would be walled in by the edge.
fn peelable(t: &Grid<TileType>, h: Hex) -> bool {
    let around: Vec<bool> = HexSide::ALL
        .iter()
        .map(|s| {
            let (r, c) = h.toward(*s);
            Hex::on_map(r, c, t.rows(), t.cols()).is_none_or(|n| *t.get(n) == TileType::Mountain)
        })
        .collect();
    let count = around.iter().filter(|m| **m).count();
    let runs = (0..6).filter(|i| around[*i] && !around[(i + 1) % 6]).count();
    runs == 1 && (2..=5).contains(&count)
}

/// True if `h` may become mountain and leave every mountain within `DEPTH`
/// of a hex that is not (the map's edge does not count as one), and wall
/// nothing in (the hexes round it that are not mountain are one run).
fn thin_enough(t: &Grid<TileType>, h: Hex) -> bool {
    let mountain = |x: Hex| x == h || *t.get(x) == TileType::Mountain;
    let open: Vec<bool> = HexSide::ALL
        .iter()
        .map(|s| {
            let (r, c) = h.toward(*s);
            Hex::on_map(r, c, t.rows(), t.cols()).is_some_and(|n| !mountain(n))
        })
        .collect();
    let runs = (0..6).filter(|i| open[*i] && !open[(i + 1) % 6]).count();
    runs == 1 && within(t, h, DEPTH).filter(|x| mountain(*x)).all(|x| within(t, x, DEPTH).any(|y| !mountain(y)))
}

/// The mountains brought to `want`, a hex at a time: over, the weakest at a
/// range's edge become hills (each only if `peelable`); under, the best
/// hexes beside a range, inland, that keep it thin and wall nothing in (on
/// the whole map: `stats::pockets`) become mountain.
fn settle(g: &mut Gen, score: &Grid<f64>, want: usize) {
    let (rows, cols) = (g.rows(), g.cols());
    let walled =
        |t: &Grid<TileType>| -> usize { super::stats::pockets(rows, cols, &|h| *t.get(h)).iter().map(Vec::len).sum() };
    for _ in 0..want.max(16) {
        let t = g.terrain.clone();
        let have = t.cells().iter().filter(|k| **k == TileType::Mountain).count();
        match have.cmp(&want) {
            std::cmp::Ordering::Equal => return,
            std::cmp::Ordering::Greater => {
                let weakest = t
                    .hexes()
                    .filter(|h| *t.get(*h) == TileType::Mountain && peelable(&t, *h))
                    .min_by(|a, b| score.get(*a).total_cmp(score.get(*b)).then(t.index(*a).cmp(&t.index(*b))));
                let Some(h) = weakest else { return };
                g.terrain.set(h, TileType::Hill);
            }
            std::cmp::Ordering::Less => {
                let mut best: Vec<Hex> = t
                    .hexes()
                    .filter(|h| !matches!(*t.get(*h), TileType::Mountain | TileType::Water))
                    .filter(|h| t.neighbours(*h).any(|n| *t.get(n) == TileType::Mountain))
                    .filter(|h| !t.neighbours(*h).any(|n| *t.get(n) == TileType::Water))
                    .filter(|h| thin_enough(&t, *h))
                    .collect();
                best.sort_by(|a, b| score.get(*b).total_cmp(score.get(*a)).then(t.index(*a).cmp(&t.index(*b))));
                let before = walled(&t);
                let mut added = false;
                for h in best {
                    g.terrain.set(h, TileType::Mountain);
                    if walled(&g.terrain) <= before {
                        added = true;
                        break;
                    }
                    g.terrain.set(h, *t.get(h));
                }
                if !added {
                    return;
                }
            }
        }
    }
}

/// Thick masses of mountain thinned to their spines: while any mountain is
/// deeper than `DEPTH`, the mountains at the edge near it go, the weakest
/// first, from both sides (each only if `peelable`), back to what they were.
fn thin(g: &mut Gen, before: &Grid<TileType>, score: &Grid<f64>) {
    let (rows, cols) = (g.rows(), g.cols());
    for _ in 0..32 {
        let now = g.terrain.clone();
        let mountain = |h: Hex| *now.get(h) == TileType::Mountain;
        let depth = steps_from(rows, cols, now.hexes().filter(|h| !mountain(*h)), |_| true);
        let deep = |h: Hex| mountain(h) && depth.get(h).is_none_or(|d| d > DEPTH);
        let mut edge: Vec<(f64, Hex)> = now
            .hexes()
            .filter(|h| mountain(*h) && *depth.get(*h) == Some(1) && within(&now, *h, DEPTH).any(deep))
            .map(|h| (*score.get(h), h))
            .collect();
        edge.sort_by(|a, b| a.0.total_cmp(&b.0).then(now.index(a.1).cmp(&now.index(b.1))));
        let mut peeled = false;
        for (_, h) in edge {
            if peelable(&g.terrain, h) {
                g.terrain.set(h, *before.get(h));
                peeled = true;
            }
        }
        if !peeled {
            return;
        }
    }
}

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
    // The mountains: the best scores, away from the sea, thinned; as many
    // laid before thinning as leave the mix's share after it.
    let mut inland: Vec<(usize, f64)> =
        land.iter().filter(|h| !t.neighbours(**h).any(water)).map(|h| (t.index(*h), *score.get(*h))).collect();
    inland.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    let lay = |g: &mut Gen, n: usize| {
        g.terrain = t.clone();
        for (i, _) in inland.iter().take(n) {
            g.terrain.cells_mut()[*i] = TileType::Mountain;
        }
        thin(g, &t, &score);
        // Lone bits of mountain are hills.
        let now = g.terrain.clone();
        let (piece, sizes) = pieces(g.rows(), g.cols(), |h| *now.get(h) == TileType::Mountain);
        for h in now.hexes() {
            if piece.get(h).is_some_and(|p| sizes[p] < MIN_RANGE) {
                g.terrain.set(h, TileType::Hill);
            }
        }
        g.terrain.cells().iter().filter(|k| **k == TileType::Mountain).count()
    };
    let (mut n, mut best) = ((mountains * 3 / 2).min(inland.len()), (usize::MAX, 0));
    for _ in 0..8 {
        let got = lay(g, n);
        if got.abs_diff(mountains) < best.0 {
            best = (got.abs_diff(mountains), n);
        }
        if got.abs_diff(mountains) * 400 <= land_hexes.max(1) {
            break;
        }
        n = ((n as f64 * mountains as f64 / got.max(1) as f64).round() as usize).clamp(mountains, inland.len());
    }
    lay(g, best.1);
    let placed = g.terrain.cells().iter().filter(|t| **t == TileType::Hill).count();
    let free_seed = Rng::stream(cfg.seed, "hills").next_u64();
    let f = 1.0 / (FREE_HILLS * cfg.feature_scale.max(0.05));
    let free = Grid::from_fn(g.rows(), g.cols(), |h| {
        let (x, y) = center(h);
        fbm(free_seed, x * f, y * f, 4, 0.5)
    });
    ring(g, (&score, &free), cfg.hill_ring, hills.saturating_sub(placed));
    open_pockets(g, &score);
    settle(g, &score, mountains);
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
