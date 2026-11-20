//! Rivers and lakes (`docs/mapgen.md`, stage 3): the land is flooded from
//! the water up (a priority flood over all but the mountains), which gives
//! every hex the neighbour it drains to; a river from a source by the
//! mountains, or from the map's land edge, follows that down to the sea or
//! another river, so it never crosses a range, and widens toward its mouth.
//! Then lakes, mostly small, in the land's deepest hollows. (The sea gives
//! back what they took: `generate`.)

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::state::{Hex, TileType};

use super::Gen;
use super::config::Config;
use super::grid::{Grid, steps_from};
use super::noise::{center, fbm};
use super::rng::Rng;

/// Rivers shorter than this are not rivers.
const MIN_RIVER: usize = 15;
/// How far a source is from the sea, and from another source, at the least.
const FROM_SEA: u32 = 20;
const APART: u32 = 15;
/// The sizes a lake is grown to.
const LAKE: (u64, u64) = (3, 100);

/// Where each land hex drains to, and how high the water would stand there
/// (the elevation, its hollows filled).
struct Drainage {
    to: Grid<Option<Hex>>,
    level: Grid<f64>,
}

/// How much a river wanders (a noise on the ground it flows over), and how
/// much the land rises toward the map's edges (so rivers leave them).
const MEANDER: f64 = 0.12;
const MEANDER_SIZE: f64 = 9.0;
const EDGE_RISE: f64 = 0.08;
/// How much higher the ground right at the foot of a range is, so rivers
/// keep a hex off it (and the range keeps its ring of hills).
const FOOT_RISE: f64 = 0.06;

/// Flood the land from the water up: each land hex but the mountains is
/// reached from a lower neighbour it drains to; a hollow fills to its rim.
/// The ground it floods is the elevation, wandering a little (`MEANDER`),
/// higher by the map's edges.
fn drainage(g: &Gen, seed: u64) -> Drainage {
    let (rows, cols) = (g.rows(), g.cols());
    let t = &g.terrain;
    let f = 1.0 / MEANDER_SIZE;
    let wander = Grid::from_fn(rows, cols, |h| {
        let (x, y) = center(h);
        fbm(seed, x * f, y * f, 3, 0.5)
    });
    let mut level = Grid::from_fn(rows, cols, |h| {
        let edge = (1.0 - f64::from(t.edge_distance(h)) / 4.0).max(0.0);
        let foot = if t.neighbours(h).any(|n| *t.get(n) == TileType::Mountain) { FOOT_RISE } else { 0.0 };
        g.elevation.get(h) + MEANDER * wander.get(h) + EDGE_RISE * edge + foot
    });
    let mut to: Grid<Option<Hex>> = Grid::new(rows, cols, None);
    let mut done: Grid<bool> = Grid::new(rows, cols, false);
    let mut heap = BinaryHeap::new();
    for h in t.hexes().filter(|h| *t.get(*h) == TileType::Water) {
        done.set(h, true);
        heap.push(Reverse((Level(*level.get(h)), t.index(h))));
    }
    while let Some(Reverse((Level(at_level), i))) = heap.pop() {
        let hex = t.hex(i);
        for n in t.neighbours(hex) {
            if *done.get(n) || *t.get(n) == TileType::Mountain {
                continue;
            }
            done.set(n, true);
            // A hollow fills: the water stands a hair above where it came
            // from, a hair that wanders too, so the way across a flat does.
            let l = level.get(n).max(at_level + 1e-4 * (0.1 + wander.get(n)));
            level.set(n, l);
            to.set(n, Some(hex));
            heap.push(Reverse((Level(l), t.index(n))));
        }
    }
    Drainage { to, level }
}

/// An f64 to order a heap by (its total order).
#[derive(Copy, Clone, PartialEq)]
struct Level(f64);
impl Eq for Level {}
impl PartialOrd for Level {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Level {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

pub fn rivers(g: &mut Gen, cfg: &Config) {
    let mut rng = Rng::stream(cfg.seed, "rivers");
    let hexes = g.terrain.len() as f64;
    let meander = rng.next_u64();
    let flow = drainage(g, meander);
    let (rows, cols) = (g.rows(), g.cols());
    let t = g.terrain.clone();
    let water = |h: Hex| *t.get(h) == TileType::Water;
    let to_sea = steps_from(rows, cols, t.hexes().filter(|h| water(*h)), |_| true);
    // Sources: by the mountains (or on high hills), and at the land's edge of
    // the map (a river from beyond it, which cuts the land in two on its way
    // to the sea), far from the water, the highest first; the two kinds by
    // turns.
    let source = |h: &Hex| {
        matches!(*t.get(*h), TileType::Grass | TileType::Hill)
            && to_sea.get(*h).is_some_and(|d| d >= FROM_SEA)
            && flow.to.get(*h).is_some()
    };
    let ranked = |list: Vec<Hex>, rng: &mut Rng| {
        let mut list: Vec<(Hex, f64)> = list.into_iter().map(|h| (h, *g.elevation.get(h) + rng.unit() * 0.1)).collect();
        list.sort_by(|a, b| b.1.total_cmp(&a.1).then(t.index(a.0).cmp(&t.index(b.0))));
        list
    };
    let hills = ranked(
        t.hexes()
            .filter(source)
            .filter(|h| t.neighbours(*h).any(|n| *t.get(n) == TileType::Mountain) || *g.elevation.get(*h) > 0.8)
            .collect(),
        &mut rng,
    );
    let edge = ranked(t.hexes().filter(|h| t.on_edge(*h) && source(h)).collect(), &mut rng);
    let mut sources: Vec<(Hex, f64)> = Vec::new();
    for i in 0..hills.len().max(edge.len()) {
        sources.extend(edge.get(i));
        sources.extend(hills.get(i));
    }
    let wanted = cfg.rivers.of(hexes) as usize;
    let mut starts: Vec<Hex> = Vec::new();
    let mut river: Grid<bool> = Grid::new(rows, cols, false);
    for (source, _) in sources {
        if g.rivers.len() >= wanted {
            break;
        }
        if starts.iter().any(|s| s.distance(source) < APART) || *river.get(source) {
            continue;
        }
        // Down the drainage to water, or to another river: joining it as
        // soon as it comes alongside (rivers do not run side by side).
        let mut way = vec![source];
        let mut at = source;
        while let Some(next) = *flow.to.get(at) {
            if water(next) || *river.get(next) {
                way.push(next);
                break;
            }
            way.push(next);
            if let Some(other) = t.neighbours(next).find(|n| *river.get(*n) && !way.contains(n)) {
                way.push(other);
                break;
            }
            at = next;
        }
        if way.len() < MIN_RIVER {
            continue;
        }
        starts.push(source);
        for h in &way {
            river.set(*h, true);
        }
        g.rivers.push(way);
    }
    // Carve them, wider toward the mouth.
    let (narrow, wide) = (cfg.river_width.0.max(1), cfg.river_width.1.max(cfg.river_width.0.max(1)));
    for way in g.rivers.clone() {
        let n = way.len();
        for (i, h) in way.iter().enumerate() {
            // (Its mouth too: the sea's rebalance leaves it be.)
            g.fresh.set(*h, true);
            if water(*h) {
                continue;
            }
            let width = match 3 * i / n.max(1) {
                0 => 1,
                1 => 2,
                _ => 3,
            }
            .clamp(narrow, wide);
            g.terrain.set(*h, TileType::Water);
            g.fresh.set(*h, true);
            // Wider: the lowest land beside it, and the next.
            let mut beside: Vec<Hex> =
                t.neighbours(*h).filter(|b| matches!(*g.terrain.get(*b), TileType::Grass | TileType::Hill)).collect();
            beside.sort_by(|a, b| flow.level.get(*a).total_cmp(flow.level.get(*b)).then(t.index(*a).cmp(&t.index(*b))));
            for b in beside.into_iter().take(width as usize - 1) {
                g.terrain.set(b, TileType::Water);
                g.fresh.set(b, true);
            }
        }
    }
    // Then the lakes, in the hollows the rivers left.
    lakes(g, cfg.lakes.of(hexes), &mut rng);
}

/// Lakes in the land's deepest hollows (where the flood filled most), or,
/// with none deep enough, its lowest land away from the sea: grown round
/// about their middle to 3 to 100 hexes.
fn lakes(g: &mut Gen, count: u32, rng: &mut Rng) {
    if count == 0 {
        return;
    }
    let flow = drainage(g, rng.next_u64());
    let (rows, cols) = (g.rows(), g.cols());
    let t = g.terrain.clone();
    let water = |h: Hex| *t.get(h) == TileType::Water;
    let to_water = steps_from(rows, cols, t.hexes().filter(|h| water(*h)), |_| true);
    let mut spots: Vec<(Hex, f64)> = t
        .hexes()
        .filter(|h| *t.get(*h) == TileType::Grass && to_water.get(*h).is_some_and(|d| d >= 6))
        .map(|h| (h, (flow.level.get(h) - g.elevation.get(h)) * 10.0 - g.elevation.get(h) + rng.unit() * 0.05))
        .collect();
    spots.sort_by(|a, b| b.1.total_cmp(&a.1).then(t.index(a.0).cmp(&t.index(b.0))));
    let mut middles: Vec<Hex> = Vec::new();
    for (spot, _) in spots {
        if middles.len() as u32 >= count {
            break;
        }
        if middles.iter().any(|m| m.distance(spot) < 12) {
            continue;
        }
        middles.push(spot);
        // Mostly small, now and then big.
        let u = rng.unit();
        let size = LAKE.0 as usize + ((LAKE.1 - LAKE.0) as f64 * u * u * u) as usize;
        // Grown over the lowest land beside it, never onto a mountain.
        let mut lake = vec![spot];
        let mut heap = BinaryHeap::new();
        for n in t.neighbours(spot) {
            heap.push(Reverse((Level(*g.elevation.get(n)), t.index(n))));
        }
        while lake.len() < size {
            let Some(Reverse((_, i))) = heap.pop() else { break };
            let h = t.hex(i);
            if lake.contains(&h) || !matches!(*t.get(h), TileType::Grass | TileType::Hill) {
                continue;
            }
            // Lakes keep off the sea and off other lakes.
            if to_water.get(h).is_some_and(|d| d < 3) {
                continue;
            }
            lake.push(h);
            for n in t.neighbours(h) {
                heap.push(Reverse((Level(*g.elevation.get(n)), t.index(n))));
            }
        }
        for h in lake {
            g.terrain.set(h, TileType::Water);
            g.fresh.set(h, true);
        }
    }
}
