//! Land and water (`docs/mapgen.md`, stage 1): an elevation from noise and
//! the layout's shape; the lowest hexes are the sea, as many as the water
//! share asks, exactly; specks of land become water and specks of sea land;
//! and, for cities only ships reach, islands out in the open sea.

use crate::state::{Hex, TileType};

use super::Gen;
use super::config::{Config, Edge, Layout};
use super::grid::{Grid, pieces, steps_from, top};
use super::noise::{center, fbm};
use super::rng::Rng;

/// How big the land's own shapes are, in the noise's units (a hex is about
/// 1.6 of them), at `feature_scale` 1.
const LAND_FEATURE: f64 = 36.0;

/// The layout's shape at a point: higher where land is meant.
fn shape(cfg: &Config, (nx, ny): (f64, f64), seeds: &Seeds) -> f64 {
    // nx, ny: the point across the map, 0 to 1.
    let edge = |e: Edge| match e {
        Edge::North => ny,
        Edge::South => 1.0 - ny,
        Edge::West => nx,
        Edge::East => 1.0 - nx,
    };
    match cfg.layout {
        // Falling toward each sea edge over a quarter of the map.
        Layout::Coast => {
            let d = cfg.sea_edges.iter().map(|e| edge(*e)).fold(1.0f64, f64::min);
            0.55 * (d / 0.3).min(1.0)
        }
        // A rounded square, falling to every edge.
        Layout::Continent => {
            let (x, y) = ((nx - 0.5) * 2.0, (ny - 0.5) * 2.0);
            let d = (x * x * x * x + y * y * y * y).sqrt().sqrt();
            0.9 * (1.0 - d)
        }
        // The edges a little higher, so the water lies inland.
        Layout::Inland => {
            let d = [nx, ny, 1.0 - nx, 1.0 - ny].into_iter().fold(1.0f64, f64::min);
            0.3 * (1.0 - (d / 0.2).min(1.0))
        }
        // Two masses with a channel of sea between them, and a saddle of
        // land across it: the land bridge.
        Layout::Twin => {
            let wide = cfg.aspect < 1.0;
            let (a, b) = if wide { ((0.26, 0.5), (0.74, 0.5)) } else { ((0.5, 0.26), (0.5, 0.74)) };
            let blob = |(cx, cy): (f64, f64)| {
                let (dx, dy) = (nx - cx, ny - cy);
                1.0 - (dx * dx + dy * dy).sqrt() / 0.36
            };
            let (across, along) =
                if wide { ((nx - 0.5).abs(), (ny - 0.5).abs()) } else { ((ny - 0.5).abs(), (nx - 0.5).abs()) };
            let channel = 0.8 * (1.0 - across / 0.1).max(0.0);
            let saddle = 0.9 * (1.0 - along / 0.07).max(0.0);
            0.9 * blob(a).max(blob(b)).max(-1.0) - channel * (1.0 - saddle).max(0.0)
        }
        // A home island for each side, and smaller ones, in a falloff.
        Layout::Archipelago => {
            let mut best: f64 = -1.0;
            for (cx, cy, r) in &seeds.islands {
                let (dx, dy) = (nx - cx, ny - cy);
                best = best.max(1.0 - (dx * dx + dy * dy).sqrt() / r);
            }
            let d = [nx, ny, 1.0 - nx, 1.0 - ny].into_iter().fold(1.0f64, f64::min);
            0.9 * best - 0.5 * (1.0 - (d / 0.1).min(1.0))
        }
    }
}

/// What the layout drew by chance before the noise: the islands' middles
/// and sizes, across the map.
struct Seeds {
    islands: Vec<(f64, f64, f64)>,
}

fn seeds(cfg: &Config, rng: &mut Rng) -> Seeds {
    let mut islands = Vec::new();
    if cfg.layout == Layout::Archipelago {
        // The home islands round the middle, as far apart as the sides are
        // many; the same size, near enough.
        let n = usize::from(cfg.sides);
        let ring: [(f64, f64); 8] = [
            (0.25, 0.25),
            (0.75, 0.75),
            (0.75, 0.25),
            (0.25, 0.75),
            (0.5, 0.18),
            (0.5, 0.82),
            (0.18, 0.5),
            (0.82, 0.5),
        ];
        for (x, y) in ring.iter().take(n) {
            let j = (rng.unit() - 0.5) * 0.06;
            islands.push((x + j, y - j, 0.17 + rng.unit() * 0.017));
        }
        for _ in 0..n * 2 {
            islands.push((0.08 + rng.unit() * 0.84, 0.08 + rng.unit() * 0.84, 0.04 + rng.unit() * 0.05));
        }
    }
    Seeds { islands }
}

/// Make the elevation and the sea: `sea` hexes of water.
pub fn land(g: &mut Gen, cfg: &Config, sea: usize) {
    let mut rng = Rng::stream(cfg.seed, "land");
    let seeds = seeds(cfg, &mut rng);
    let noise_seed = rng.next_u64();
    let bay_seed = rng.next_u64();
    let (rows, cols) = (g.rows(), g.cols());
    let (far_x, far_y) = center(Hex::new(rows - 1, cols - 1));
    let f = 1.0 / (LAND_FEATURE * cfg.feature_scale.max(0.05));
    let gain = 0.4 + 0.2 * cfg.roughness;
    let e: Grid<f64> = Grid::from_fn(rows, cols, |h| {
        let (x, y) = center(h);
        let n = fbm(noise_seed, x * f, y * f, 5, gain);
        // Bays: the coast's line moved about by a slower noise.
        let bay = (fbm(bay_seed, x * f * 0.6, y * f * 0.6, 3, 0.5) - 0.5) * 0.5;
        let at = ((x / far_x.max(1.0) + bay).clamp(0.0, 1.0), (y / far_y.max(1.0) - bay).clamp(0.0, 1.0));
        n * (1.0 - cfg.roughness / 2.0) + shape(cfg, at, &seeds)
    });
    let e = normalized(&e);
    // The lowest are the sea.
    let order: Vec<(usize, f64)> = e.cells().iter().enumerate().map(|(i, v)| (i, -v)).collect();
    let mut terrain = Grid::new(rows, cols, TileType::Grass);
    for i in top(&order, sea) {
        terrain.cells_mut()[i] = TileType::Water;
    }
    g.elevation = e;
    g.terrain = terrain;
    tidy(g, cfg.min_island as usize);
    if cfg.sea_cities {
        islands(g, cfg, &mut rng, sea);
    }
}

/// A grid scaled to 0 to 1.
pub fn normalized(g: &Grid<f64>) -> Grid<f64> {
    let lo = g.cells().iter().copied().fold(f64::INFINITY, f64::min);
    let hi = g.cells().iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = if hi > lo { hi - lo } else { 1.0 };
    let mut out = g.clone();
    for v in out.cells_mut() {
        *v = (*v - lo) / span;
    }
    out
}

/// Specks: land pieces smaller than `min_island` become water, unless they
/// are kept; sea pieces under 3 hexes that touch no edge become land. Then
/// the water share is put back as it was: the lowest coast or the highest
/// shore gives way.
fn tidy(g: &mut Gen, min_island: usize) {
    let before = water_count(g);
    let (rows, cols) = (g.rows(), g.cols());
    let t = g.terrain.clone();
    let (land, sizes) = pieces(rows, cols, |h| *t.get(h) != TileType::Water);
    let biggest = sizes.iter().copied().max().unwrap_or(0);
    let (water, wsizes) = pieces(rows, cols, |h| *t.get(h) == TileType::Water);
    let mut touches = vec![false; wsizes.len()];
    for h in t.hexes() {
        if let (Some(p), true) = (*water.get(h), t.on_edge(h)) {
            touches[p] = true;
        }
    }
    for h in t.hexes() {
        if let Some(p) = *land.get(h) {
            if sizes[p] < min_island && sizes[p] < biggest {
                g.terrain.set(h, TileType::Water);
            }
        }
        if let Some(p) = *water.get(h) {
            if wsizes[p] < 3 && !touches[p] {
                g.terrain.set(h, TileType::Grass);
            }
        }
    }
    rebalance(g, before);
}

fn water_count(g: &Gen) -> usize {
    g.terrain.cells().iter().filter(|t| **t == TileType::Water).count()
}

/// Back to `want` water hexes: the lowest land by the water becomes water,
/// or the highest water by the land becomes land.
pub fn rebalance(g: &mut Gen, want: usize) {
    let have = water_count(g);
    let t = g.terrain.clone();
    let coast = |h: Hex, wet: bool| {
        (*t.get(h) == TileType::Water) == wet && t.neighbours(h).any(|n| (*t.get(n) == TileType::Water) != wet)
    };
    if have < want {
        // (Not the islands made for cities at sea.)
        let shore: Vec<(usize, f64)> = t
            .hexes()
            .filter(|h| coast(*h, false) && !*g.island.get(*h))
            .map(|h| (t.index(h), -g.elevation.get(h)))
            .collect();
        for i in top(&shore, want - have) {
            g.terrain.cells_mut()[i] = TileType::Water;
        }
    } else if have > want {
        let shallows: Vec<(usize, f64)> =
            t.hexes().filter(|h| coast(*h, true)).map(|h| (t.index(h), *g.elevation.get(h))).collect();
        for i in top(&shallows, have - want) {
            g.terrain.cells_mut()[i] = TileType::Grass;
        }
    }
}

/// Islands out at sea, for cities only ships reach: one for every two or
/// three of them, 6 to 60 hexes each, grown as blobs in open water at least
/// 3 hexes from other land; the water they take is given back by the lowest
/// coast.
fn islands(g: &mut Gen, cfg: &Config, rng: &mut Rng, sea: usize) {
    let (rows, cols) = (g.rows(), g.cols());
    let cities = f64::from(cfg.cities.unwrap_or(u32::from(cfg.sides) * cfg.cities_per_side));
    let wanted = cfg.sea_city_count.of(cities).max(1);
    let count = wanted.div_ceil(2) as usize;
    for _ in 0..count {
        let t = g.terrain.clone();
        let land_hexes = t.hexes().filter(|h| *t.get(*h) != TileType::Water);
        let from_land = steps_from(rows, cols, land_hexes, |_| true);
        let open: Vec<Hex> =
            t.hexes().filter(|h| from_land.get(*h).is_some_and(|d| d >= 5) && t.edge_distance(*h) >= 3).collect();
        let Some(&start) = rng.pick(&open) else { break };
        let size = rng.range(6, 60) as usize;
        let mut grown = vec![start];
        let mut i = 0;
        while grown.len() < size && i < grown.len() {
            let at = grown[i];
            for n in t.neighbours(at) {
                let room = from_land.get(n).is_some_and(|d| d >= 3);
                if grown.len() < size && room && !grown.contains(&n) && rng.below(3) > 0 {
                    grown.push(n);
                }
            }
            i += 1;
        }
        for h in grown {
            g.terrain.set(h, TileType::Grass);
            g.island.set(h, true);
        }
    }
    rebalance(g, sea);
}
