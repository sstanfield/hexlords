//! Roads, bridges and passes (`docs/mapgen.md`, stage 8): links from each
//! city on foot to its nearest, each found by a road search that bridges
//! rivers (never the sea or a lake, never longer than `max_bridge`) and
//! passes ranges (never longer than two hexes); the cheapest tree of them
//! joining the cities, and some loops (`roads`); drawn so that roads merge;
//! tidied (no corners cut twice, no roads side by side, no stubs); and every
//! city and site meant to be reached on foot made so (`repair`): a road to
//! it, or a ford.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::state::{Hex, HexSide, Side, TileType};

use super::Gen;
use super::cities::travel;
use super::config::{Config, Plan, Roads};
use super::grid::{Grid, cheapest_path};
use super::noise::center;
use super::reach::ReachClass;
use super::rng::Rng;

/// What a step costs a road to be laid on, in halves of a grass step: the
/// ground; a road already there; a bridge over a river, a pass over a
/// mountain, a hex of each.
fn ground(t: TileType) -> Option<u32> {
    match t {
        TileType::Grass | TileType::Temple | TileType::Ruin => Some(2),
        TileType::City => Some(1),
        TileType::Desert => Some(3),
        TileType::Forest => Some(4),
        TileType::Hill => Some(6),
        TileType::Swamp => Some(8),
        _ => None,
    }
}
const ON_ROAD: u32 = 1;
/// What a new road pays for a hex beside another road or a city (but the
/// hex it comes from, and the city it goes to): in the game every two road
/// hexes that touch are joined, so roads side by side, or leaving a city
/// by neighbouring sides, draw tangles. A new road joins an old one or
/// keeps a hex from it.
const TOUCH: u32 = 8;
const BRIDGE: u32 = 12;
const PASS: u32 = 10;
/// The longest pass; and the widest range a road may cross, its sides then
/// cut down to hills (a saddle) so that the pass over it is no longer.
const PASS_LONG: u32 = 2;
const PASS_SEARCH: u32 = 5;
/// How many nearest cities a city's links go to; a capital's.
const LINKS: usize = 3;
const CAPITAL_LINKS: usize = 4;
/// And how many nearest as the crow flies (a city across a range is far by
/// travel, near by a pass).
const NEAR_LINKS: usize = 2;
/// `sparse`: the share of cities on a road aimed at.
const SPARSE_SHARE: f64 = 0.6;

/// Where a road may go and what it costs.
struct Ways<'a> {
    g: &'a Gen,
    max_bridge: u32,
    bridges: bool,
    /// No road on a sea city's island, nor into an air city's ring.
    banned: Grid<bool>,
}

/// A link between two cities: their places in the list of cities on foot,
/// what its road costs on the bare land (without a pass if it can be), the
/// travel between them, whether its road goes over a pass, and what a pass
/// would save it.
#[derive(Clone, Debug)]
struct Link {
    a: usize,
    b: usize,
    cost: u32,
    travel: u32,
    pass: bool,
    saving: u32,
    /// How many ranges its road over passes crosses.
    crossings: usize,
}

/// What a pass must save a road to be worth it: a turn's march on grass.
const PASS_SAVING: u32 = 20;

impl<'a> Ways<'a> {
    fn new(g: &'a Gen, cfg: &Config, banned: &Grid<bool>) -> Ways<'a> {
        Ways { g, max_bridge: cfg.max_bridge.max(1), bridges: cfg.bridges, banned: banned.clone() }
    }

    /// The cheapest road from `from` to `to`, its cost and its hexes (the
    /// two ends included), over the roads in `road` (which cost little, so
    /// roads merge); bridges and passes as allowed (`passes`).
    fn path(&self, (from, to): (Hex, Hex), road: &Grid<bool>, passes: bool) -> Option<(u32, Vec<Hex>)> {
        let t = &self.g.terrain;
        let (rows, cols) = (t.rows(), t.cols());
        // A state: a hex, and how many water (1..=max_bridge) or mountain
        // (then max_bridge + 1 ..) hexes in a row lead to it.
        let water_top = self.max_bridge;
        let kinds = (water_top + PASS_SEARCH + 1) as usize;
        let id = |h: Hex, run: u32| t.index(h) * kinds + run as usize;
        let mut best: Vec<u32> = vec![u32::MAX; t.len() * kinds];
        let mut back: Vec<usize> = vec![usize::MAX; t.len() * kinds];
        let mut heap = BinaryHeap::new();
        best[id(from, 0)] = 0;
        // Of ways that cost the same, the one nearer the straight line
        // between the ends: roads run straight, not in the grid's zigzags.
        let ((ax, ay), (bx, by)) = (center(from), center(to));
        let (dx, dy) = (bx - ax, by - ay);
        let length = (dx * dx + dy * dy).sqrt().max(1e-9);
        let off_line = |h: Hex| {
            let (x, y) = center(h);
            (((x - ax) * dy - (y - ay) * dx).abs() / length * 16.0) as u32
        };
        heap.push(Reverse((from.distance(to) * ON_ROAD, 0u32, 0u32, id(from, 0))));
        while let Some(Reverse((_, _, cost, state))) = heap.pop() {
            if cost > best[state] {
                continue;
            }
            let (hex, run) = (t.hex(state / kinds), (state % kinds) as u32);
            if hex == to && run == 0 {
                let mut hexes = vec![hex];
                let mut s = state;
                while back[s] != usize::MAX {
                    s = back[s];
                    hexes.push(t.hex(s / kinds));
                }
                hexes.reverse();
                return Some((cost, hexes));
            }
            let (in_water, in_mountain) = (run >= 1 && run <= water_top, run > water_top);
            for side in HexSide::ALL {
                let (r, c) = hex.toward(side);
                let Some(next) = Hex::on_map(r, c, rows, cols) else { continue };
                if *self.banned.get(next) && next != to {
                    continue;
                }
                let on_road = *road.get(next);
                let step = match *t.get(next) {
                    TileType::Water => {
                        if !self.bridges || !*self.g.river.get(next) || in_mountain {
                            continue;
                        }
                        let n = if in_water { run + 1 } else { 1 };
                        if n > water_top {
                            continue;
                        }
                        (if on_road { ON_ROAD } else { BRIDGE }, n)
                    }
                    TileType::Mountain => {
                        if !(passes || on_road) || in_water {
                            continue;
                        }
                        let n = if in_mountain { run - water_top + 1 } else { 1 };
                        if n > PASS_SEARCH {
                            continue;
                        }
                        (if on_road { ON_ROAD } else { PASS }, water_top + n)
                    }
                    k => {
                        let Some(g) = ground(k) else { continue };
                        if on_road {
                            (ON_ROAD.min(g), 0)
                        } else {
                            let joins = |n: Hex| *road.get(n) || *t.get(n) == TileType::City;
                            let touches = t.neighbours(next).any(|n| n != hex && n != to && joins(n));
                            (g + if touches && next != to { TOUCH } else { 0 }, 0)
                        }
                    }
                };
                let (price, next_run) = step;
                let s = id(next, next_run);
                let c = cost + price;
                if c < best[s] {
                    best[s] = c;
                    back[s] = state;
                    heap.push(Reverse((c + next.distance(to) * ON_ROAD, off_line(next), c, s)));
                }
            }
        }
        None
    }
}

/// Lay the roads (as `roads` asks), then make every city and site meant to
/// be reached on foot reachable.
pub fn roads(g: &mut Gen, cfg: &Config, plan: &Plan) {
    let (rows, cols) = (g.rows(), g.cols());
    let air: Vec<Hex> = g.cities.iter().filter(|c| c.reach == ReachClass::Air).map(|c| c.hex).collect();
    let banned = Grid::from_fn(rows, cols, |h| *g.island.get(h) || air.iter().any(|a| a.distance(h) <= 2));
    let foot: Vec<(Hex, bool)> =
        g.cities.iter().filter(|c| c.reach == ReachClass::Foot).map(|c| (c.hex, c.side != Side::Grey)).collect();
    let mut road: Grid<bool> = Grid::new(rows, cols, false);
    if cfg.roads != Roads::None && foot.len() > 1 {
        let ways = Ways::new(g, cfg, &banned);
        let links = links(&ways, &foot);
        let chosen = network(&links, cfg, plan, foot.len());
        // (Drawn after others, a road may find another way over the
        // mountains than it was chosen for: the passes are held to
        // `passes` here too, a road that would go over drawn round.)
        let budget = cfg.passes.of(f64::from(plan.cities)) as usize;
        let mut crossed = 0;
        for link in chosen {
            let ends = (foot[link.a].0, foot[link.b].0);
            let mut way = ways.path(ends, &road, link.pass);
            if let Some((_, hexes)) = way.as_ref().filter(|_| link.pass) {
                let new = new_passes(&g.terrain, &road, hexes);
                if crossed + new > budget {
                    way = ways.path(ends, &road, false);
                } else {
                    crossed += new;
                }
            }
            if let Some((_, hexes)) = way {
                draw(&mut road, &g.terrain, &hexes);
            }
        }
        tidy(&mut road, &g.terrain);
    }
    // Every city and site meant to be reached on foot, reached: each fix
    // made before the next is looked for.
    let targets = targets(g);
    for _ in 0..targets.len() * 2 + 1 {
        let fix = repair(&Ways::new(g, cfg, &banned), cfg, &road, &targets);
        match fix {
            Fix::Done | Fix::Stuck => break,
            Fix::Road(hexes) => draw(&mut road, &g.terrain, &hexes),
            Fix::Ford(hexes) => {
                for h in hexes {
                    g.terrain.set(h, TileType::Grass);
                    g.river.set(h, false);
                    g.fresh.set(h, false);
                }
            }
        }
    }
    tidy(&mut road, &g.terrain);
    gates(&mut road, &g.terrain);
    saddles(g, &road);
    g.road = road;
}

/// A pass longer than `PASS_LONG`: its mountains but the middle one or two
/// become hills, a saddle the road climbs to its pass.
fn saddles(g: &mut Gen, road: &Grid<bool>) {
    let (rows, cols) = (g.rows(), g.cols());
    let t = g.terrain.clone();
    let pass = |h: Hex| *road.get(h) && *t.get(h) == TileType::Mountain;
    let (piece, sizes) = super::grid::pieces(rows, cols, pass);
    for (id, size) in sizes.iter().enumerate() {
        if *size as u32 <= PASS_LONG {
            continue;
        }
        let run: Vec<Hex> = t.hexes().filter(|h| *piece.get(*h) == Some(id)).collect();
        // Steps along it from its ends (where it meets land).
        let ends = run
            .iter()
            .copied()
            .filter(|h| t.neighbours(*h).any(|n| *road.get(n) && !pass(n) || *t.get(n) == TileType::City));
        let along = super::grid::steps_from(rows, cols, ends, |h| *piece.get(h) == Some(id));
        let top = run.iter().filter_map(|h| *along.get(*h)).max().unwrap_or(0);
        for h in run {
            if along.get(h).is_some_and(|d| d < top) {
                g.terrain.set(h, TileType::Hill);
            }
        }
    }
}

/// What the reach repair does next.
enum Fix {
    /// Everything meant to be reached on foot is.
    Done,
    /// A road to lay (bridges and passes as needed).
    Road(Vec<Hex>),
    /// River hexes to make land.
    Ford(Vec<Hex>),
    /// Something cut off that nothing can reach.
    Stuck,
}

/// What must be reached on foot: the cities meant to be, and the temples
/// and ruins off the islands.
fn targets(g: &Gen) -> Vec<Hex> {
    let t = &g.terrain;
    let mut out: Vec<Hex> = g.cities.iter().filter(|c| c.reach == ReachClass::Foot).map(|c| c.hex).collect();
    out.extend(t.hexes().filter(|h| matches!(*t.get(*h), TileType::Temple | TileType::Ruin) && !*g.island.get(*h)));
    out
}

/// The candidate links: each city on foot to its nearest by travel (a
/// capital to more), the road each would take on the bare land; a link
/// whose road runs through a third city is left out (the two through it
/// do).
fn links(ways: &Ways, foot: &[(Hex, bool)]) -> Vec<Link> {
    let g = ways.g;
    let t = &g.terrain;
    let bare = Grid::new(t.rows(), t.cols(), false);
    let mut pairs: Vec<(usize, usize, u32)> = Vec::new();
    for (i, (hex, capital)) in foot.iter().enumerate() {
        let (cost, _) = travel(g, &[*hex]);
        let mut near: Vec<(u32, usize)> =
            (0..foot.len()).filter(|j| *j != i).filter_map(|j| cost.get(foot[j].0).map(|c| (c, j))).collect();
        near.sort_unstable();
        let mut crow: Vec<(u32, usize)> =
            (0..foot.len()).filter(|j| *j != i).map(|j| (hex.distance(foot[j].0), j)).collect();
        crow.sort_unstable();
        let by_travel = near.iter().copied().take(if *capital { CAPITAL_LINKS } else { LINKS });
        let by_crow = crow.into_iter().take(NEAR_LINKS).map(|(_, j)| (cost.get(foot[j].0).unwrap_or(u32::MAX), j));
        for (c, j) in by_travel.chain(by_crow) {
            let (a, b) = (i.min(j), i.max(j));
            if !pairs.iter().any(|p| p.0 == a && p.1 == b) {
                pairs.push((a, b, c));
            }
        }
    }
    let city = |h: Hex| *t.get(h) == TileType::City;
    let mut out = Vec::new();
    let over_pass = |hexes: &[Hex]| hexes.iter().any(|h| *t.get(*h) == TileType::Mountain);
    for (a, b, travel) in pairs {
        let ends = (foot[a].0, foot[b].0);
        let around = ways.path(ends, &bare, false);
        let over = ways.path(ends, &bare, true).filter(|(_, hexes)| over_pass(hexes));
        let (cost, hexes, pass) = match (&around, &over) {
            (Some((c, h)), _) => (*c, h.clone(), false),
            (None, Some((c, h))) => (*c, h.clone(), true),
            (None, None) => continue,
        };
        if hexes[1..hexes.len() - 1].iter().any(|h| city(*h)) {
            continue;
        }
        let saving = match (&around, &over) {
            (Some((c, _)), Some((p, _))) => c.saturating_sub(*p),
            (None, Some(_)) => u32::MAX,
            _ => 0,
        };
        let mountain = |h: &Hex| *t.get(*h) == TileType::Mountain;
        let crossings =
            over.as_ref().map_or(0, |(_, h)| h.windows(2).filter(|w| mountain(&w[1]) && !mountain(&w[0])).count());
        out.push(Link { a, b, cost, travel, pass, saving, crossings });
    }
    out.sort_by_key(|l| (l.cost, l.a, l.b));
    out
}

/// Which links become roads: the cheapest tree joining the cities (Kruskal),
/// no more passes than `passes` allows; then, `linked`, the cheapest
/// `road_loops` share of the rest; `dense`, two and a half times as many;
/// `sparse`, the tree thinned, its longest links first (one of the three
/// longest, by lot), until about three cities in five are on a road.
fn network(links: &[Link], cfg: &Config, plan: &Plan, cities: usize) -> Vec<Link> {
    let mut parent: Vec<usize> = (0..cities).collect();
    fn find(p: &mut [usize], mut x: usize) -> usize {
        while p[x] != x {
            p[x] = p[p[x]];
            x = p[x];
        }
        x
    }
    let budget = cfg.passes.of(f64::from(plan.cities)) as usize;
    let mut passes = 0;
    let (mut tree, mut rest): (Vec<Link>, Vec<Link>) = (Vec::new(), Vec::new());
    for l in links {
        if l.pass && passes + l.crossings > budget {
            continue;
        }
        let (ra, rb) = (find(&mut parent, l.a), find(&mut parent, l.b));
        if ra != rb {
            parent[ra] = rb;
            passes += if l.pass { l.crossings } else { 0 };
            tree.push(l.clone());
        } else {
            rest.push(l.clone());
        }
    }
    let loops = |share: f64, rest: &[Link], passes: usize| {
        let want = (share * rest.len() as f64).round() as usize;
        let mut used = passes;
        let mut out = Vec::new();
        for l in rest {
            if out.len() >= want {
                break;
            }
            if l.pass {
                if used + l.crossings > budget {
                    continue;
                }
                used += l.crossings;
            }
            out.push(l.clone());
        }
        out
    };
    let mut chosen: Vec<Link> = match cfg.roads {
        Roads::None => Vec::new(),
        Roads::Linked => {
            let extra = loops(cfg.road_loops, &rest, passes);
            tree.into_iter().chain(extra).collect()
        }
        Roads::Dense => {
            let extra = loops((cfg.road_loops * 2.5).min(1.0), &rest, passes);
            tree.into_iter().chain(extra).collect()
        }
        Roads::Sparse => {
            let mut rng = Rng::stream(cfg.seed, "sparse roads");
            let mut kept: Vec<Link> = tree;
            kept.sort_by_key(|l| (Reverse(l.travel), l.a, l.b));
            let on_road = |kept: &[Link]| {
                let mut on = vec![false; cities];
                for l in kept {
                    on[l.a] = true;
                    on[l.b] = true;
                }
                on.iter().filter(|o| **o).count()
            };
            while !kept.is_empty() && on_road(&kept) as f64 > SPARSE_SHARE * cities as f64 + 0.5 {
                let i = rng.below(kept.len().min(3) as u64) as usize;
                let mut without = kept.clone();
                without.remove(i);
                if (on_road(&without) as f64) < (SPARSE_SHARE - 0.05) * cities as f64 {
                    break;
                }
                kept = without;
            }
            kept
        }
    };
    // Passes where they save most (a turn at least), as many as are left:
    // a road chosen goes over the pass; one not chosen is added.
    if cfg.roads != Roads::Sparse {
        let mut used: usize = chosen.iter().filter(|l| l.pass).map(|l| l.crossings).sum();
        let mut by_saving: Vec<&Link> = links.iter().filter(|l| !l.pass && l.saving >= PASS_SAVING).collect();
        by_saving.sort_by_key(|l| (Reverse(l.saving), l.a, l.b));
        for l in by_saving {
            if used + l.crossings > budget {
                continue;
            }
            match chosen.iter_mut().find(|c| c.a == l.a && c.b == l.b) {
                Some(c) => c.pass = true,
                None => chosen.push(Link { pass: true, ..l.clone() }),
            }
            used += l.crossings;
        }
    }
    chosen
}

/// How many ranges a road along `hexes` crosses where no road is yet.
fn new_passes(t: &Grid<TileType>, road: &Grid<bool>, hexes: &[Hex]) -> usize {
    let fresh = |h: &Hex| *t.get(*h) == TileType::Mountain && !*road.get(*h);
    hexes.windows(2).filter(|w| fresh(&w[1]) && !fresh(&w[0])).count()
}

/// A road along `hexes`: every hex of it a road but the cities at its ends
/// (a city joins the roads beside it by itself).
fn draw(road: &mut Grid<bool>, t: &Grid<TileType>, hexes: &[Hex]) {
    for h in hexes {
        if *t.get(*h) != TileType::City {
            road.set(*h, true);
        }
    }
}

/// Two roads at a city's gate by neighbouring sides draw a triangle (in the
/// game every two road hexes that touch are joined): one of them goes, its
/// road carried by the other, if the roads beyond it touch that; or moves a
/// side round, if the land there takes a road and the roads beyond touch it
/// too; else it stays.
fn gates(road: &mut Grid<bool>, t: &Grid<TileType>) {
    let (rows, cols) = (t.rows(), t.cols());
    let at = |c: Hex, i: usize| {
        let (r, col) = c.toward(HexSide::ALL[i % 6]);
        Hex::on_map(r, col, rows, cols)
    };
    let land_road = |road: &Grid<bool>, h: Hex| *road.get(h) && ground(*t.get(h)).is_some();
    let crossing =
        |road: &Grid<bool>, h: Hex| *road.get(h) && matches!(*t.get(h), TileType::Water | TileType::Mountain);
    let cities: Vec<Hex> = t.hexes().filter(|h| *t.get(*h) == TileType::City).collect();
    for c in cities {
        for _ in 0..6 {
            let pair = (0..6).find(|i| {
                at(c, *i).is_some_and(|h| land_road(road, h)) && at(c, i + 1).is_some_and(|h| land_road(road, h))
            });
            let Some(i) = pair else { break };
            let mut fixed = false;
            // Each of the two in turn: (the one that goes, the one beside it,
            // the side it may move to).
            for (go, other, to) in [(i, i + 1, i + 5), (i + 1, i, i + 2)] {
                let (Some(g), Some(o)) = (at(c, go), at(c, other)) else { continue };
                if t.neighbours(g).any(|n| crossing(road, n)) {
                    continue;
                }
                let beyond: Vec<Hex> = t.neighbours(g).filter(|n| *n != c && *n != o && joins(road, t, *n)).collect();
                if beyond.iter().all(|b| b.is_adjacent(o)) {
                    road.set(g, false);
                    fixed = true;
                    break;
                }
                let Some(x) = at(c, to) else { continue };
                let free = !*road.get(x)
                    && ground(*t.get(x)).is_some()
                    && *t.get(x) != TileType::City
                    && at(c, to + 5).is_none_or(|n| !joins(road, t, n) || n == g)
                    && at(c, to + 1).is_none_or(|n| !joins(road, t, n) || n == g);
                if free && beyond.iter().all(|b| b.is_adjacent(x)) {
                    road.set(g, false);
                    road.set(x, true);
                    fixed = true;
                    break;
                }
            }
            if !fixed {
                break;
            }
        }
    }
}

/// True if a hex joins the roads beside it: a road, or a city.
fn joins(road: &Grid<bool>, t: &Grid<TileType>, h: Hex) -> bool {
    *road.get(h) || *t.get(h) == TileType::City
}

/// Tidy the roads: a road hex on land whose road neighbours stay joined
/// without it goes (a corner cut, two roads side by side made one), and so
/// does a stub that ends nowhere; until nothing changes.
fn tidy(road: &mut Grid<bool>, t: &Grid<TileType>) {
    let (rows, cols) = (t.rows(), t.cols());
    for _ in 0..64 {
        let mut changed = false;
        for h in t.hexes() {
            if !*road.get(h) || matches!(*t.get(h), TileType::Water | TileType::Mountain | TileType::City) {
                continue;
            }
            let around: Vec<bool> = HexSide::ALL
                .iter()
                .map(|s| {
                    let (r, c) = h.toward(*s);
                    Hex::on_map(r, c, rows, cols).is_some_and(|n| joins(road, t, n))
                })
                .collect();
            let count = around.iter().filter(|a| **a).count();
            let runs = (0..6).filter(|i| around[*i] && !around[(i + 1) % 6]).count();
            // The land end of a bridge or a pass stays.
            let crossing = |n: Hex| *road.get(n) && matches!(*t.get(n), TileType::Water | TileType::Mountain);
            if t.neighbours(h).any(crossing) {
                continue;
            }
            if count <= 1 || (count >= 2 && runs == 1) {
                road.set(h, false);
                changed = true;
            }
        }
        if !changed {
            return;
        }
    }
}

/// Where a walker gets from `starts`, as the game lets it: onto any land
/// but a mountain; onto a bridge or a pass only along the road.
pub fn walked(g: &Gen, road: &Grid<bool>, starts: &[Hex]) -> Grid<bool> {
    let t = &g.terrain;
    let mut seen = Grid::new(t.rows(), t.cols(), false);
    let mut stack: Vec<Hex> = starts.to_vec();
    for s in starts {
        seen.set(*s, true);
    }
    while let Some(h) = stack.pop() {
        for n in t.neighbours(h) {
            if *seen.get(n) {
                continue;
            }
            let ok = match *t.get(n) {
                TileType::Water | TileType::Mountain => *road.get(n) && joins(road, t, h),
                _ => true,
            };
            if ok {
                seen.set(n, true);
                stack.push(n);
            }
        }
    }
    seen
}

/// The next fix for what is meant to be reached on foot and is not, from
/// the capitals: a road from it to one of the nearest cities reached
/// (bridges as needed, a pass only if nothing else gets there), or, where
/// there is none, a ford: the
/// fewest river hexes between it and land reached, made land.
fn repair(ways: &Ways, cfg: &Config, road: &Grid<bool>, targets: &[Hex]) -> Fix {
    let g = ways.g;
    let t = &g.terrain;
    let capitals: Vec<Hex> = g.cities.iter().filter(|c| c.side != Side::Grey).map(|c| c.hex).collect();
    let reached = walked(g, road, &capitals);
    let reached = |h: Hex| *reached.get(h);
    let Some(lost) = targets.iter().copied().find(|h| !reached(*h)) else { return Fix::Done };
    if cfg.roads != Roads::None && cfg.bridges {
        let mut near: Vec<Hex> =
            g.cities.iter().filter(|c| c.reach == ReachClass::Foot && reached(c.hex)).map(|c| c.hex).collect();
        near.sort_by_key(|h| (h.distance(lost), t.index(*h)));
        // Round the ranges if it can be, over a pass if not.
        for passes in [false, true] {
            if let Some((_, hexes)) = near.iter().take(6).find_map(|c| ways.path((lost, *c), road, passes)) {
                return Fix::Road(hexes);
            }
        }
    }
    let ford = cheapest_path(
        t.rows(),
        t.cols(),
        &[lost],
        |h| match *t.get(h) {
            TileType::Water if *g.river.get(h) => Some(1),
            TileType::Water | TileType::Mountain => None,
            _ => Some(0),
        },
        |h| reached(h) && !matches!(*t.get(h), TileType::Water | TileType::Mountain),
    );
    match ford {
        Some(hexes) => Fix::Ford(hexes.into_iter().filter(|h| *t.get(*h) == TileType::Water).collect()),
        None => Fix::Stuck,
    }
}
