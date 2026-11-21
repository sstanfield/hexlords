//! Temples and ruins (`docs/mapgen.md`, stage 7): shared out among the
//! capitals' cells as evenly as they go, each kind on its own (a temple
//! blesses the units that stop on it: the owner, "temples are a definite
//! boon"); each side's first temple a few turns from its capital, the rest
//! spread over its land, in wilder country by liking; none near a city or
//! another site, none where no walker gets.

use crate::state::{Hex, Side, TileType};

use super::Gen;
use super::cities::{Fair, travel, walk};
use super::config::{Amount, Config, Fairness, Plan};
use super::grid::{Grid, steps_from};
use super::reach::ReachClass;
use super::rng::Rng;

/// A turn of the reference unit, and where each side's first temple goes:
/// between these many turns from its capital.
const TURN: u32 = 10;
const NEAR: (u32, u32) = (20, 35);
/// How much nearer its own capital a site is than any other's: well inside
/// its cell, so a change of a hex or two does not move it to another.
const INSIDE: u32 = 8;

/// The fairness of the sites, by level: temples in a cell (most less
/// fewest), ruins likewise, the way from each capital to its nearest temple
/// (longest less shortest, in turns).
fn limits(level: Fairness) -> Option<[f64; 3]> {
    match level {
        Fairness::Off => None,
        Fairness::Loose => Some([2.0, 2.0, 3.0]),
        Fairness::Normal => Some([1.0, 1.0, 2.0]),
        Fairness::Strict => Some([0.0, 0.0, 1.0]),
    }
}

/// How many of a kind of site: as asked; a rate, with fairness on, rounded
/// to a whole number for each side (one each at least), so every side can
/// have as many.
pub fn count(amount: Amount, cfg: &Config, plan: &Plan) -> usize {
    let n = amount.of(f64::from(plan.cities)) as usize;
    let sides = usize::from(cfg.sides).max(1);
    match amount {
        Amount::Rate(_) if cfg.fairness != Fairness::Off && n > 0 => {
            ((n as f64 / sides as f64).round() as usize).max(1) * sides
        }
        _ => n,
    }
}

/// How many temples: `temples` if it is given, else `temples_per_side` for
/// each side (0.5: one shared by two neighbours).
pub fn temple_count(cfg: &Config, plan: &Plan) -> usize {
    match cfg.temples {
        Some(amount) => count(amount, cfg, plan),
        None => (cfg.temples_per_side * f64::from(cfg.sides)).round() as usize,
    }
}

/// `n` temples (fewer than the sides) of the places `open`, shared between
/// the capitals (`grids`: their travel): where the capital worst served is
/// nearest one, the capitals' ways to their nearest temples most alike, and
/// all of them shortest; `gap` apart. The nearest first, then each moved where that is better.
fn shared(open: &[Hex], grids: &[Grid<Option<u32>>], n: usize, gap: u32) -> Vec<Hex> {
    let far = u32::MAX / 4;
    let cost = |h: Hex, i: usize| grids[i].get(h).unwrap_or(far);
    // The most travel from a capital to its nearest temple, how far the
    // others fall short of it, and all of it (so a temple that cannot make
    // the most less goes where it serves most).
    let judge = |set: &[Hex]| {
        let d: Vec<u32> = (0..grids.len()).map(|i| set.iter().map(|h| cost(*h, i)).min().unwrap_or(far)).collect();
        let (most, least) = (*d.iter().max().unwrap_or(&far), *d.iter().min().unwrap_or(&far));
        (most, most - least, d.iter().map(|x| u64::from(*x)).sum::<u64>())
    };
    let apart = |set: &[Hex], h: Hex, skip: Option<usize>| {
        set.iter().enumerate().all(|(i, s)| Some(i) == skip || s.distance(h) >= gap)
    };
    let mut set: Vec<Hex> = Vec::new();
    for _ in 0..n {
        let best = open.iter().copied().filter(|h| apart(&set, *h, None)).min_by_key(|h| {
            let mut s = set.clone();
            s.push(*h);
            (judge(&s), h.row, h.col)
        });
        match best {
            Some(h) => set.push(h),
            None => break,
        }
    }
    for _ in 0..3 {
        let mut moved = false;
        for i in 0..set.len() {
            let now = judge(&set);
            let better = open.iter().copied().filter(|h| apart(&set, *h, Some(i))).min_by_key(|h| {
                let mut s = set.clone();
                s[i] = *h;
                (judge(&s), h.row, h.col)
            });
            if let Some(h) = better {
                let mut s = set.clone();
                s[i] = h;
                if judge(&s) < now {
                    set = s;
                    moved = true;
                }
            }
        }
        if !moved {
            break;
        }
    }
    set
}

/// How much a place is liked for a site: a little more in wilder country
/// (forest, hills, the water's edge, swamp round it) than on the open grass;
/// the target map's sites have some of each round them.
fn liking(t: &Grid<TileType>, h: Hex) -> f64 {
    let wild = t
        .neighbours(h)
        .filter(|n| matches!(*t.get(*n), TileType::Forest | TileType::Hill | TileType::Water | TileType::Swamp))
        .count();
    let base = match *t.get(h) {
        TileType::Grass => 1.0,
        TileType::Forest => 0.8,
        TileType::Hill => 0.8,
        TileType::Desert => 0.6,
        _ => 0.5,
    };
    base * (0.8 + 0.4 * wild as f64 / 6.0)
}

/// Lay the temples and the ruins, and say how fairly they are shared out
/// (added to `g.fairness`).
pub fn sites(g: &mut Gen, cfg: &Config, plan: &Plan) {
    let (rows, cols) = (g.rows(), g.cols());
    let mut rng = Rng::stream(cfg.seed, "sites");
    let capitals: Vec<Hex> = g.cities.iter().filter(|c| c.side != Side::Grey).map(|c| c.hex).collect();
    if capitals.is_empty() {
        return;
    }
    let sides = capitals.len();
    let grids: Vec<Grid<Option<u32>>> = capitals.iter().map(|c| travel(g, &[*c]).0).collect();
    let owner = |h: Hex| (0..sides).filter_map(|i| grids[i].get(h).map(|c| (c, i))).min().map(|(_, i)| i);
    let inside = |h: Hex| {
        let mut costs: Vec<u32> = grids.iter().filter_map(|grid| *grid.get(h)).collect();
        costs.sort_unstable();
        costs.len() < 2 || costs[1] - costs[0] >= INSIDE
    };
    let air: Vec<Hex> = g.cities.iter().filter(|c| c.reach == ReachClass::Air).map(|c| c.hex).collect();
    let cities: Vec<Hex> = g.cities.iter().map(|c| c.hex).collect();
    // (On a map of islands, the capitals' islands: a site is walked to.)
    let walked = walk(g, &capitals);
    let near_city = steps_from(rows, cols, cities.iter().copied(), |_| true);
    // Where a site may be: land a walker gets to from the capitals, not a
    // city, off the edge, not on an island, not in an air city's ring, far
    // enough from the cities.
    let may = |t: &Grid<TileType>, h: Hex| {
        matches!(*t.get(h), TileType::Grass | TileType::Forest | TileType::Hill | TileType::Desert | TileType::Swamp)
            && owner(h).is_some()
            && walked.get(h).is_some()
            && t.edge_distance(h) >= 1
            && !*g.island.get(h)
            && air.iter().all(|a| a.distance(h) > 2)
            && near_city.get(h).is_some_and(|d| d >= cfg.site_city_gap)
    };
    let fits = |t: &Grid<TileType>, h: Hex| may(t, h) && inside(h);
    // A little chance in where sites go.
    let jitter: Vec<f64> = (0..g.terrain.len()).map(|_| rng.unit()).collect();
    let mut placed: Vec<Hex> = Vec::new();
    for (kind, n) in [(TileType::Temple, temple_count(cfg, plan)), (TileType::Ruin, count(cfg.ruins, cfg, plan))] {
        // Fewer temples than sides: each shared by neighbours.
        if kind == TileType::Temple && n > 0 && n < sides {
            let t = g.terrain.clone();
            let open: Vec<Hex> =
                t.hexes().filter(|h| may(&t, *h) && placed.iter().all(|s| s.distance(*h) >= cfg.site_gap)).collect();
            for h in shared(&open, &grids, n, cfg.site_gap) {
                g.terrain.set(h, kind);
                placed.push(h);
            }
            continue;
        }
        // So many to each cell; what does not share out evenly, to cells
        // by lot.
        let mut want = vec![n / sides; sides];
        let mut order: Vec<usize> = (0..sides).collect();
        for i in (1..sides).rev() {
            order.swap(i, rng.below(i as u64 + 1) as usize);
        }
        for i in order.into_iter().take(n % sides) {
            want[i] += 1;
        }
        for (cell, wanted) in want.iter().enumerate() {
            for k in 0..*wanted {
                let t = g.terrain.clone();
                let apart = |h: Hex| placed.iter().all(|s| s.distance(h) >= cfg.site_gap);
                let mine = |h: Hex| owner(h) == Some(cell) && fits(&t, h) && apart(h);
                // The first temple near home; the others where the sites
                // and cities are farthest, by liking.
                let first_temple = kind == TileType::Temple && k == 0;
                let score = |h: Hex| {
                    let room = placed
                        .iter()
                        .chain(&cities)
                        .map(|s| s.distance(h))
                        .min()
                        .unwrap_or(u32::MAX)
                        .min(2 * cfg.site_gap);
                    let base = if first_temple { 10.0 } else { f64::from(room) };
                    base * liking(&t, h) * (0.7 + 0.6 * jitter[t.index(h)])
                };
                let home = |h: Hex| grids[cell].get(h).is_some_and(|c| (NEAR.0..=NEAR.1).contains(&c));
                let pick = |near: bool| {
                    t.hexes()
                        .filter(|h| mine(*h) && (!near || home(*h)))
                        .max_by(|a, b| score(*a).total_cmp(&score(*b)).then(t.index(*b).cmp(&t.index(*a))))
                };
                let spot = if first_temple { pick(true).or_else(|| pick(false)) } else { pick(false) };
                if let Some(h) = spot {
                    g.terrain.set(h, kind);
                    placed.push(h);
                }
            }
        }
    }
}

/// How fairly the sites are shared out, on the map as it ends (added to
/// `g.fairness`).
pub fn judge(g: &mut Gen, cfg: &Config) {
    let Some(limit) = limits(cfg.fairness) else { return };
    let capitals: Vec<Hex> = g.cities.iter().filter(|c| c.side != Side::Grey).map(|c| c.hex).collect();
    let grids: Vec<Grid<Option<u32>>> = capitals.iter().map(|c| travel(g, &[*c]).0).collect();
    if !grids.is_empty() {
        let fair = fairness(g, &grids, limit);
        g.fairness.extend(fair);
    }
}

/// How fairly the sites are shared out: temples, then ruins, in each cell
/// (most less fewest), and the way from each capital to its nearest temple
/// (longest less shortest, in turns).
fn fairness(g: &Gen, grids: &[Grid<Option<u32>>], limit: [f64; 3]) -> Vec<Fair> {
    let sides = grids.len();
    let t = &g.terrain;
    let owner = |h: Hex| (0..sides).filter_map(|i| grids[i].get(h).map(|c| (c, i))).min().map(|(_, i)| i);
    let spread = |kind: TileType| {
        let mut n = vec![0usize; sides];
        for h in t.hexes().filter(|h| *t.get(*h) == kind) {
            if let Some(o) = owner(h) {
                n[o] += 1;
            }
        }
        (*n.iter().max().unwrap_or(&0) - *n.iter().min().unwrap_or(&0)) as f64
    };
    let temples: Vec<Hex> = t.hexes().filter(|h| *t.get(*h) == TileType::Temple).collect();
    let mut out = vec![Fair {
        name: "ruins in a cell (most less fewest)".to_string(),
        value: spread(TileType::Ruin),
        limit: limit[1],
    }];
    // (Fewer temples than sides stand between cells, shared: none is a
    // cell's own.)
    if temples.len() >= sides {
        out.insert(
            0,
            Fair {
                name: "temples in a cell (most less fewest)".to_string(),
                value: spread(TileType::Temple),
                limit: limit[0],
            },
        );
    }
    if !temples.is_empty() {
        let nearest: Vec<f64> = grids
            .iter()
            .map(|grid| {
                let c = temples.iter().filter_map(|h| *grid.get(*h)).min().unwrap_or(10 * TURN);
                f64::from(c) / f64::from(TURN)
            })
            .collect();
        let (most, least) =
            (nearest.iter().copied().fold(0.0, f64::max), nearest.iter().copied().fold(f64::MAX, f64::min));
        out.push(Fair {
            name: "way to the nearest temple (longest less shortest, turns)".to_string(),
            value: most - least,
            limit: limit[2],
        });
    }
    out
}
