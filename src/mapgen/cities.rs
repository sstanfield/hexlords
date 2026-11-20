//! Cities (`docs/mapgen.md`, stage 6): the capitals, as far apart by travel
//! as the land allows and with as fair a share of it each; the neutral
//! cities spread over the land (a Poisson disc, then relaxed toward the
//! middles of their cells), as many as asked; cities on islands only ships
//! reach, and in mountain rings only flyers reach, when their checkboxes are
//! on; a fairness pass that moves cities from the side with the most to the
//! side with the fewest; room around each city; and each city's record:
//! its name, walls, taxes, what it builds, who stands in it.
//!
//! Travel is the reference unit's on foot (`reach::reference_foot`), with a
//! river crossed as if bridged (the roads see to that).

use crate::state::{Hex, Side, TileType, UnitKind};

use super::Gen;
use super::config::{Config, Fairness, Plan};
use super::grid::{Grid, cheapest_from, pieces, steps_from};
use super::names::{NAMES, Syllables};
use super::noise::center;
use super::reach::{MoveClass, ReachClass, movement_class};
use super::rng::Rng;

/// A city the generator made, and its record.
#[derive(Clone, Debug, PartialEq)]
pub struct City {
    pub hex: Hex,
    /// Its holder at the start: a capital's side, else nobody (`Grey`).
    pub side: Side,
    /// How it is meant to be reached.
    pub reach: ReachClass,
    pub name: String,
    pub walls: u32,
    pub income: u32,
    /// What it builds (a unit's id, turns), and who stands in it (an id,
    /// how many).
    pub produce: Vec<(String, u32)>,
    pub garrison: Vec<(String, u32)>,
}

/// A measure of how fair the sharing out is, and the most it may be.
#[derive(Clone, Debug, PartialEq)]
pub struct Fair {
    pub name: String,
    pub value: f64,
    pub limit: f64,
}

impl Fair {
    pub fn ok(&self) -> bool {
        self.value <= self.limit + 1e-9
    }
}

/// A turn of the reference unit, and what crossing a river costs it (as on
/// a bridge, before there is one).
const TURN: u32 = 10;
const RIVER: u32 = 6;
/// The fairness pass's most moves, and how many capital tries it has.
const MOVES: usize = 50;
/// How many sets of capitals are tried on one terrain (a terrain that will
/// not share out fairly is made anew: `generate`).
const CAPITAL_TRIES: u32 = 6;
/// How many places a capital may be at are weighed, and how many swaps the
/// search for the best of them makes.
const SAMPLES: usize = 140;
const SWAPS: usize = 400;
const STARTS: usize = 4;
/// The most rounds of moving each capital within its own land.
const SWEEPS: usize = 12;
/// What a degree of a capital's widest view short of a half turn costs a
/// set of capitals (`hemmed`), against the travel from the land to them.
const HEMMED: f64 = 4.0;

fn walkable(t: TileType) -> bool {
    !matches!(t, TileType::Water | TileType::Mountain)
}

/// What a step onto a hex costs the reference unit, before roads: a river
/// as if bridged; a lake, the sea and a mountain are walls.
fn price(g: &Gen, h: Hex) -> Option<u32> {
    match *g.terrain.get(h) {
        TileType::Grass | TileType::Temple | TileType::Ruin => Some(2),
        TileType::Forest => Some(4),
        TileType::Hill | TileType::Swamp => Some(5),
        TileType::Desert => Some(3),
        TileType::City => Some(1),
        TileType::Water if *g.river.get(h) => Some(RIVER),
        _ => None,
    }
}

/// Travel costs from the nearest of `sources`, and which it is.
pub fn travel(g: &Gen, sources: &[Hex]) -> (Grid<Option<u32>>, Grid<Option<usize>>) {
    let s: Vec<(Hex, u32)> = sources.iter().map(|h| (*h, 0)).collect();
    cheapest_from(g.rows(), g.cols(), &s, |_, to| price(g, to))
}

/// The levels of the fairness table, by `Fairness`: neutral cities in a
/// cell, within 3 turns of a capital, land in a cell (a ratio), the way to
/// the nearest enemy capital (a ratio), ports in a cell, sea and air cities
/// nearest one capital.
fn limits(level: Fairness) -> Option<[f64; 6]> {
    match level {
        Fairness::Off => None,
        Fairness::Loose => Some([3.0, 2.0, 1.5, 1.5, 3.0, 2.0]),
        Fairness::Normal => Some([2.0, 1.0, 1.3, 1.3, 2.0, 1.0]),
        Fairness::Strict => Some([1.0, 1.0, 1.15, 1.15, 1.0, 1.0]),
    }
}

/// Where the cities go and what they are.
struct Places {
    capitals: Vec<Hex>,
    neutrals: Vec<Hex>,
    sea: Vec<Hex>,
}

impl Places {
    fn all(&self) -> Vec<Hex> {
        self.capitals.iter().chain(&self.neutrals).chain(&self.sea).copied().collect()
    }
}

/// Make the cities: placed, made fair (as far as the tries allow), with
/// room around them, and recorded.
pub fn cities(g: &mut Gen, cfg: &Config, plan: &Plan, kinds: &[UnitKind]) -> Result<(), String> {
    let sides = usize::from(cfg.sides);
    let total = plan.cities as usize;
    let has = |class: MoveClass| kinds.iter().any(|k| movement_class(k) == class);
    let sea_count =
        if cfg.sea_cities { (cfg.sea_city_count.of(total as f64) as usize).clamp(1, total / 6 + 1) } else { 0 };
    let air_count =
        if cfg.fly_cities { cfg.fly_city_count.map_or((total / 40).clamp(1, 3), |n| n as usize) } else { 0 };
    let ports = cfg.ports && has(MoveClass::Naval);
    g.navigable = navigable(g);
    let mut best: Option<(usize, Places, Vec<Fair>)> = None;
    for attempt in 0..CAPITAL_TRIES {
        let mut rng = Rng::stream(cfg.seed, &format!("cities {attempt}"));
        let Some(capitals) = pick_capitals(g, cfg, plan, &mut rng) else { continue };
        let mut places = Places { capitals, neutrals: Vec::new(), sea: Vec::new() };
        places.sea = sea_places(g, cfg, plan, &places.capitals, sea_count, &mut rng);
        let foot = total.saturating_sub(sides + places.sea.len());
        places.neutrals = place_neutrals(g, cfg, plan, &places, foot, &mut rng);
        relax(g, cfg, plan, &mut places);
        let fair = make_fair(g, cfg, plan, &mut places, ports);
        // The holes the fairness pass leaves, filled as far as fairness allows.
        let fair = fill_holes(g, cfg, plan, &mut places, (ports, fair));
        let misses = fair.iter().filter(|f| !f.ok()).count();
        let whole = places.all().len() == total;
        let score = misses + if whole { 0 } else { 10 };
        if best.as_ref().is_none_or(|(s, _, _)| score < *s) {
            best = Some((score, places, fair));
        }
        if score == 0 {
            break;
        }
    }
    let (_, places, fair) = best.ok_or("there is no room for the capitals: the map is too small or too broken")?;
    let mut rng = Rng::stream(cfg.seed, "city records");
    let air = air_places(g, &places, air_count);
    room(g, &places.all(), &air);
    g.fairness = fair;
    g.cities = records(g, cfg, (&places, &air), kinds, &mut rng);
    Ok(())
}

/// The capitals: of the places a capital may be (grass on the main land,
/// well in from the edges, off the water, open around, with land enough
/// within 3 turns), those as far apart by travel as can be, with as fair a
/// share of the land and as like a way to their nearest enemy: farthest
/// points first, then swaps that make it better.
fn pick_capitals(g: &Gen, cfg: &Config, plan: &Plan, rng: &mut Rng) -> Option<Vec<Hex>> {
    let (rows, cols) = (g.rows(), g.cols());
    let t = &g.terrain;
    let sides = usize::from(cfg.sides);
    let [_, _, land_limit, enemy_limit, _, _] = limits(cfg.fairness).unwrap_or([0.0, 0.0, 1.3, 1.3, 0.0, 0.0]);
    let to_water = steps_from(rows, cols, t.hexes().filter(|h| *t.get(*h) == TileType::Water), |_| true);
    // The main land: travel's pieces (a river crossed), the big ones.
    let (piece, sizes) =
        pieces(rows, cols, |h| walkable(*t.get(h)) || (*t.get(h) == TileType::Water && *g.river.get(h)));
    let biggest = sizes.iter().copied().max().unwrap_or(0);
    let big = |h: Hex| piece.get(h).is_some_and(|p| sizes[p] * 3 >= biggest);
    let margin = (cols / 10).max(8).min(rows / 5).min(cols / 5);
    let open = |h: Hex| t.neighbours(h).filter(|n| matches!(*t.get(*n), TileType::Grass | TileType::Forest)).count();
    let fits = |h: Hex| {
        *t.get(h) == TileType::Grass
            && t.edge_distance(h) >= margin
            && to_water.get(h).is_some_and(|d| d >= 3)
            && open(h) >= 4
            && big(h)
            && !*g.island.get(h)
    };
    let mut candidates: Vec<Hex> = t.hexes().filter(|h| fits(*h)).collect();
    // A sample, spread out.
    let mut order: Vec<(u64, Hex)> = candidates.drain(..).map(|h| (rng.next_u64(), h)).collect();
    order.sort();
    let mut samples: Vec<Hex> = Vec::new();
    for (_, h) in order {
        if samples.len() >= SAMPLES {
            break;
        }
        if samples.iter().all(|s| s.distance(h) >= 4) {
            samples.push(h);
        }
    }
    // Their travel costs, and the land each has within 3 turns.
    let mut grids: Vec<Vec<u32>> = Vec::new();
    let mut kept: Vec<Hex> = Vec::new();
    let room = (2.0 * plan.density) as usize;
    for s in samples {
        let (cost, _) = travel(g, &[s]);
        let near = cost.cells().iter().filter(|c| c.is_some_and(|c| c <= 3 * TURN)).count();
        if near < room.min(t.len() / (sides * 3)) {
            continue;
        }
        grids.push(cost.cells().iter().map(|c| c.unwrap_or(u32::MAX)).collect());
        kept.push(s);
    }
    if kept.len() < sides {
        return None;
    }
    // The land the shares are counted on: every third land hex, for speed.
    let land: Vec<usize> =
        t.hexes().filter(|h| walkable(*t.get(*h)) && big(*h)).map(|h| t.index(h)).step_by(3).collect();
    let between = |a: usize, b: usize| -> f64 {
        let c = grids[a][t.index(kept[b])];
        if c == u32::MAX { f64::from(kept[a].distance(kept[b]) * 4 + 20) } else { f64::from(c) }
    };
    // The owner of a hex (an index) among a set: the capital that gets there
    // first, if any does.
    let owner = |set: &[usize], idx: usize| -> Option<(usize, u32)> {
        set.iter()
            .enumerate()
            .map(|(k, s)| (k, grids[*s][idx]))
            .filter(|(_, c)| *c != u32::MAX)
            .min_by_key(|(k, c)| (*c, *k))
    };
    // A set's worth: each capital in the middle of its own land (the least
    // travel from the land to its capital, on the whole); within what
    // fairness allows (land, the way to the nearest enemy), beyond it
    // weighing most; and none hemmed in by the others.
    let score = |set: &[usize]| -> f64 {
        let mut nearest = vec![f64::MAX; set.len()];
        for (i, a) in set.iter().enumerate() {
            for (j, b) in set.iter().enumerate() {
                if i != j {
                    nearest[i] = nearest[i].min(between(*a, *b));
                }
            }
        }
        let closest = nearest.iter().copied().fold(f64::MAX, f64::min);
        let farthest = nearest.iter().copied().fold(0.0, f64::max);
        let mut cells = vec![0usize; set.len()];
        let (mut total, mut counted) = (0u64, 0u64);
        for idx in &land {
            if let Some((k, c)) = owner(set, *idx) {
                cells[k] += 1;
                total += u64::from(c);
                counted += 1;
            }
        }
        let mean = total as f64 / counted.max(1) as f64;
        let (most, least) = (*cells.iter().max().unwrap_or(&1), (*cells.iter().min().unwrap_or(&1)).max(1));
        let (land, enemy) = (most as f64 / least as f64, farthest / closest.max(1.0));
        let over = |r: f64, limit: f64| 400.0 * (r - limit * 0.95).max(0.0) + 20.0 * (r - 1.0);
        let hexes: Vec<Hex> = set.iter().map(|i| kept[*i]).collect();
        -mean - over(land, land_limit) - over(enemy, enemy_limit) - HEMMED * hemmed(&hexes)
    };
    // From a few starts: farthest points from a first by chance, then swaps
    // that make it better; the best of them.
    let mut best: Option<(f64, Vec<usize>)> = None;
    for _ in 0..STARTS {
        let mut set = vec![rng.below(kept.len() as u64) as usize];
        while set.len() < sides {
            let next = (0..kept.len()).filter(|c| !set.contains(c)).max_by(|a, b| {
                let d = |c: usize| set.iter().map(|s| between(*s, c)).fold(f64::MAX, f64::min);
                d(*a).total_cmp(&d(*b)).then(b.cmp(a))
            })?;
            set.push(next);
        }
        let mut now = score(&set);
        // Each capital to the best place in its own land, while that helps.
        for _ in 0..SWEEPS {
            let mut moved = false;
            for i in 0..sides {
                let mine: Vec<usize> = (0..kept.len())
                    .filter(|c| !set.contains(c) && owner(&set, t.index(kept[*c])).is_some_and(|(k, _)| k == i))
                    .collect();
                for c in mine {
                    let mut other = set.clone();
                    other[i] = c;
                    let s = score(&other);
                    if s > now {
                        (set, now, moved) = (other, s, true);
                    }
                }
            }
            if !moved {
                break;
            }
        }
        for _ in 0..SWAPS / STARTS {
            let (i, c) = (rng.below(sides as u64) as usize, rng.below(kept.len() as u64) as usize);
            if set.contains(&c) {
                continue;
            }
            let mut other = set.clone();
            other[i] = c;
            let s = score(&other);
            if s > now {
                (set, now) = (other, s);
            }
        }
        if best.as_ref().is_none_or(|(b, _)| now > *b) {
            best = Some((now, set));
        }
    }
    best.map(|(_, set)| set.into_iter().map(|i| kept[i]).collect())
}

/// How hemmed in capitals are: for each, the widest angle with no other
/// capital in it, as seen from it; the degrees by which it falls short of a
/// half turn, summed. (A capital with enemies all round has no side of the
/// map to itself.)
pub fn hemmed(capitals: &[Hex]) -> f64 {
    let mut short = 0.0;
    for (i, a) in capitals.iter().enumerate() {
        let (x, y) = center(*a);
        let mut angles: Vec<f64> = capitals
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, b)| {
                let (u, v) = center(*b);
                (v - y).atan2(u - x).to_degrees()
            })
            .collect();
        if angles.len() < 2 {
            continue;
        }
        angles.sort_by(f64::total_cmp);
        let mut widest = 360.0 - (angles[angles.len() - 1] - angles[0]);
        for w in angles.windows(2) {
            widest = f64::max(widest, w[1] - w[0]);
        }
        short += (180.0 - widest).max(0.0);
    }
    short
}

/// Where a neutral city may stand: on the land a capital can get to, not a
/// swamp or a mountain, open around (3 neighbours a walker can stand on),
/// off the map's edge.
fn site(g: &Gen, cfg: &Config, reached: &Grid<Option<u32>>, h: Hex) -> bool {
    let t = &g.terrain;
    matches!(*t.get(h), TileType::Grass | TileType::Forest | TileType::Hill | TileType::Desert)
        && t.neighbours(h).filter(|n| walkable(*t.get(*n))).count() >= 3
        && u32::from(t.edge_distance(h)) >= cfg.edge_margin
        && reached.get(h).is_some()
        && !*g.island.get(h)
}

/// How much a place is liked for a city: grass most, desert least; by the
/// water more, while the cities by it are fewer than asked.
fn liking(g: &Gen, h: Hex, coast_wanted: bool) -> f64 {
    let base = match *g.terrain.get(h) {
        TileType::Grass => 1.0,
        TileType::Forest => 0.6,
        TileType::Hill => 0.5,
        _ => 0.3,
    };
    let by_water = g.terrain.neighbours(h).any(|n| *g.terrain.get(n) == TileType::Water);
    base + if by_water && coast_wanted { 0.5 } else { 0.0 }
}

/// The neutral cities: a Poisson disc from the capitals over all the land
/// (as far apart as halfway between the least gap and the gap aimed at),
/// thinned where they crowd most, one at a time, to as many as asked (or
/// more in the emptiest places, if it made too few); then evened out.
fn place_neutrals(g: &Gen, cfg: &Config, plan: &Plan, places: &Places, count: usize, rng: &mut Rng) -> Vec<Hex> {
    let (rows, cols) = (g.rows(), g.cols());
    let (reached, _) = travel(g, &places.capitals);
    let gap = cfg.min_gap(plan);
    let r = (((f64::from(gap) + cfg.gap_target(plan)) / 2.0).round() as u32).max(gap);
    let mut cities: Vec<Hex> = places.capitals.iter().chain(&places.sea).copied().collect();
    let mut neutrals: Vec<Hex> = Vec::new();
    let mut active: Vec<Hex> = places.capitals.clone();
    let mut coastal = 0usize;
    while !active.is_empty() && neutrals.len() < count * 20 {
        let a = rng.below(active.len() as u64) as usize;
        let from = active[a];
        let mut found = false;
        for _ in 0..30 {
            let dr = rng.below(u64::from(4 * r + 1)) as i32 - 2 * r as i32;
            let dc = rng.below(u64::from(4 * r + 1)) as i32 - 2 * r as i32;
            let Some(h) = Hex::on_map(i32::from(from.row) + dr, i32::from(from.col) + dc, rows, cols) else { continue };
            let d = from.distance(h);
            if d < r || d > 2 * r || !site(g, cfg, &reached, h) {
                continue;
            }
            let coast_wanted = (coastal as f64) < cfg.coastal_cities * (neutrals.len() + 1) as f64;
            if rng.unit() > liking(g, h, coast_wanted) || cities.iter().any(|c| c.distance(h) < r) {
                continue;
            }
            coastal += usize::from(g.terrain.neighbours(h).any(|n| *g.terrain.get(n) == TileType::Water));
            cities.push(h);
            neutrals.push(h);
            active.push(h);
            found = true;
            break;
        }
        if !found {
            active.swap_remove(a);
        }
    }
    // As many as asked: more in the emptiest places, or fewer where crowded.
    while neutrals.len() < count {
        let empty = steps_from(rows, cols, cities.iter().copied(), |_| true);
        let spot = g
            .terrain
            .hexes()
            .filter(|h| site(g, cfg, &reached, *h))
            .filter_map(|h| empty.get(h).map(|d| (d, h)))
            .max_by(|a, b| a.0.cmp(&b.0).then(g.terrain.index(b.1).cmp(&g.terrain.index(a.1))));
        match spot {
            Some((d, h)) if d >= gap => {
                cities.push(h);
                neutrals.push(h);
            }
            _ => break,
        }
    }
    // Thinned: the city nearest another goes, one at a time (its nearest
    // kept up to date only for those it was nearest to).
    let others = |i: usize, neutrals: &[Hex]| {
        let h = neutrals[i];
        let fixed = places.capitals.iter().chain(&places.sea).map(|c| c.distance(h));
        let free = neutrals.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, c)| c.distance(h));
        fixed.chain(free).min().unwrap_or(u32::MAX)
    };
    let mut nearest: Vec<u32> = (0..neutrals.len()).map(|i| others(i, &neutrals)).collect();
    while neutrals.len() > count {
        let Some(i) = (0..neutrals.len()).min_by_key(|i| (nearest[*i], *i)) else { break };
        let gone = neutrals.remove(i);
        nearest.remove(i);
        for j in 0..neutrals.len() {
            if neutrals[j].distance(gone) == nearest[j] {
                nearest[j] = others(j, &neutrals);
            }
        }
    }
    let mut places = Places { capitals: places.capitals.clone(), neutrals, sea: places.sea.clone() };
    even_out(g, cfg, plan, &mut places, &|_: &Places| true);
    places.neutrals
}

/// No land left empty: while it brings the land nearer its cities on the
/// whole (the squares of the distances, so the farthest land counts most),
/// and `allowed` says yes, the most crowded city moves to the emptiest place
/// (or, if that is not allowed, the next most crowded).
fn even_out(g: &Gen, cfg: &Config, plan: &Plan, places: &mut Places, allowed: &dyn Fn(&Places) -> bool) {
    let (rows, cols) = (g.rows(), g.cols());
    let (reached, _) = travel(g, &places.capitals);
    let gap = cfg.min_gap(plan);
    let land: Vec<Hex> =
        g.terrain.hexes().filter(|h| reached.get(*h).is_some() && walkable(*g.terrain.get(*h))).collect();
    let emptiness = |all: &[Hex]| {
        let empty = steps_from(rows, cols, all.iter().copied(), |_| true);
        let sum: u64 = land.iter().filter_map(|h| empty.get(*h).map(|d| u64::from(d * d))).sum();
        (sum, empty)
    };
    for _ in 0..places.neutrals.len() {
        let (now, empty) = emptiness(&places.all());
        let Some((d, to)) = g
            .terrain
            .hexes()
            .filter(|h| site(g, cfg, &reached, *h))
            .filter_map(|h| empty.get(h).map(|d| (d, h)))
            .max_by(|a, b| a.0.cmp(&b.0).then(g.terrain.index(b.1).cmp(&g.terrain.index(a.1))))
        else {
            break;
        };
        if d < gap {
            break;
        }
        // The cities from the most crowded on, the first that may move there.
        let all = places.all();
        let mut order: Vec<(u32, usize)> = (0..places.neutrals.len())
            .map(|i| {
                let h = places.neutrals[i];
                (all.iter().filter(|c| **c != h).map(|c| c.distance(h)).min().unwrap_or(u32::MAX), i)
            })
            .collect();
        order.sort_unstable();
        let mut moved = false;
        for (_, i) in order.into_iter().take(6) {
            let mut next = Places {
                capitals: places.capitals.clone(),
                neutrals: places.neutrals.clone(),
                sea: places.sea.clone(),
            };
            next.neutrals[i] = to;
            if emptiness(&next.all()).0 < now && allowed(&next) {
                *places = next;
                moved = true;
                break;
            }
        }
        if !moved {
            break;
        }
    }
}

/// After the fairness pass: `even_out`, each move kept only if the sharing
/// out stays as fair (no measure that was within its limit goes beyond it).
fn fill_holes(g: &Gen, cfg: &Config, plan: &Plan, places: &mut Places, (ports, fair): (bool, Vec<Fair>)) -> Vec<Fair> {
    let grids: Vec<Grid<Option<u32>>> = places.capitals.iter().map(|c| travel(g, &[*c]).0).collect();
    let misses = |f: &[Fair]| f.iter().filter(|f| !f.ok()).count();
    let before = misses(&fair);
    let allowed = |p: &Places| misses(&fairness(g, cfg, p, (ports, &grids))) <= before;
    even_out(g, cfg, plan, places, &allowed);
    fairness(g, cfg, places, (ports, &grids))
}

/// A few passes of moving each neutral city a hex toward the middle of its
/// cell (the land nearer it than any other city), where it may stand and
/// keeps its distance: spacing evens out.
fn relax(g: &Gen, cfg: &Config, plan: &Plan, places: &mut Places) {
    let (rows, cols) = (g.rows(), g.cols());
    let (reached, _) = travel(g, &places.capitals);
    let gap = cfg.min_gap(plan);
    for _ in 0..3 {
        let all = places.all();
        let sources: Vec<(Hex, u32)> = all.iter().map(|h| (*h, 0)).collect();
        let (_, owner) = cheapest_from(rows, cols, &sources, |_, to| walkable(*g.terrain.get(to)).then_some(1));
        let mut sum = vec![(0.0f64, 0.0f64, 0usize); all.len()];
        for h in g.terrain.hexes() {
            if let Some(o) = *owner.get(h) {
                let (x, y) = center(h);
                sum[o] = (sum[o].0 + x, sum[o].1 + y, sum[o].2 + 1);
            }
        }
        let first = places.capitals.len();
        for i in 0..places.neutrals.len() {
            let (sx, sy, n) = sum[first + i];
            if n == 0 {
                continue;
            }
            let (mx, my) = (sx / n as f64, sy / n as f64);
            let here = places.neutrals[i];
            let far = |h: Hex| {
                let (x, y) = center(h);
                (x - mx) * (x - mx) + (y - my) * (y - my)
            };
            let others: Vec<Hex> = places.all().into_iter().filter(|c| *c != here).collect();
            let better = g
                .terrain
                .neighbours(here)
                .filter(|h| site(g, cfg, &reached, *h) && others.iter().all(|c| c.distance(*h) >= gap))
                .min_by(|a, b| far(*a).total_cmp(&far(*b)));
            if let Some(b) = better.filter(|b| far(*b) < far(here)) {
                places.neutrals[i] = b;
            }
        }
    }
}

/// Cities for islands only ships reach: on the islands made for them, the
/// first of each by the water.
fn sea_places(g: &Gen, cfg: &Config, plan: &Plan, capitals: &[Hex], count: usize, rng: &mut Rng) -> Vec<Hex> {
    if count == 0 {
        return Vec::new();
    }
    let (rows, cols) = (g.rows(), g.cols());
    let t = &g.terrain;
    let gap = cfg.min_gap(plan);
    let (piece, sizes) = pieces(rows, cols, |h| *g.island.get(h) && walkable(*t.get(h)));
    let mut order: Vec<usize> = (0..sizes.len()).collect();
    order.sort_by_key(|p| std::cmp::Reverse(sizes[*p]));
    let mut out: Vec<Hex> = Vec::new();
    for p in order {
        if out.len() >= count || sizes[p] < 6 {
            break;
        }
        let mut spots: Vec<Hex> = t
            .hexes()
            .filter(|h| *piece.get(*h) == Some(p) && t.neighbours(*h).any(|n| *t.get(n) == TileType::Water))
            .filter(|h| capitals.iter().chain(&out).all(|c| c.distance(*h) >= gap))
            .collect();
        if spots.is_empty() {
            continue;
        }
        let pick = spots.swap_remove(rng.below(spots.len() as u64) as usize);
        out.push(pick);
    }
    out
}

/// Cities only flyers reach: neutral cities 3 turns or more from every
/// capital, in or by a range, closed in by a ring of mountains two hexes
/// out with grass inside; the mountains it takes are given back elsewhere.
fn air_places(g: &mut Gen, places: &Places, count: usize) -> Vec<Hex> {
    if count == 0 {
        return Vec::new();
    }
    let (cost, _) = travel(g, &places.capitals);
    let t = g.terrain.clone();
    let ring = |c: Hex| -> Vec<Hex> { t.hexes().filter(|h| h.distance(c) == 2).collect() };
    let mut picks: Vec<(usize, Hex)> = places
        .neutrals
        .iter()
        .copied()
        .filter(|c| cost.get(*c).is_some_and(|d| d >= 3 * TURN))
        .filter(|c| t.edge_distance(*c) >= 3)
        .filter(|c| ring(*c).iter().all(|h| *t.get(*h) != TileType::Water))
        .map(|c| (t.hexes().filter(|h| h.distance(c) <= 3 && *t.get(*h) == TileType::Mountain).count(), c))
        .collect();
    picks.sort_by(|a, b| b.0.cmp(&a.0).then(t.index(a.1).cmp(&t.index(b.1))));
    let mut out = Vec::new();
    for (_, c) in picks.into_iter().take(count) {
        let mut added = 0usize;
        for h in ring(c) {
            if *g.terrain.get(h) != TileType::Mountain {
                g.terrain.set(h, TileType::Mountain);
                added += 1;
            }
        }
        for h in t.neighbours(c) {
            g.terrain.set(h, TileType::Grass);
        }
        // The mountains it took, back to hills where the ranges' edges are
        // lowest.
        let snap = g.terrain.clone();
        let mut edges: Vec<(Hex, f64)> = snap
            .hexes()
            .filter(|h| *snap.get(*h) == TileType::Mountain && h.distance(c) > 2)
            .filter(|h| snap.neighbours(*h).any(|n| *snap.get(n) != TileType::Mountain))
            .map(|h| (h, *g.ridge.get(h)))
            .collect();
        edges.sort_by(|a, b| a.1.total_cmp(&b.1).then(snap.index(a.0).cmp(&snap.index(b.0))));
        for (h, _) in edges.into_iter().take(added) {
            g.terrain.set(h, TileType::Hill);
        }
        out.push(c);
    }
    out
}

/// How fair the sharing out is: of the neutral cities in each capital's
/// cell (the land it gets to before any other), within 3 turns of each,
/// the land in each cell, the way to each one's nearest enemy, the ports in
/// each cell, and the cities at sea nearest one capital.
fn fairness(g: &Gen, cfg: &Config, places: &Places, (ports, grids): (bool, &[Grid<Option<u32>>])) -> Vec<Fair> {
    let Some(limit) = limits(cfg.fairness) else { return Vec::new() };
    let sides = places.capitals.len();
    let owner = |h: Hex| -> Option<usize> {
        (0..sides).filter_map(|i| grids[i].get(h).map(|c| (c, i))).min().map(|(_, i)| i).or_else(|| {
            // Off the land the capitals walk to (an island): the nearest.
            (0..sides).min_by_key(|i| places.capitals[*i].distance(h))
        })
    };
    let spread =
        |counts: &[usize]| (*counts.iter().max().unwrap_or(&0) as f64) - (*counts.iter().min().unwrap_or(&0) as f64);
    let mut cell = vec![0usize; sides];
    let mut near = vec![0usize; sides];
    for c in &places.neutrals {
        if let Some(o) = owner(*c) {
            cell[o] += 1;
        }
        for (i, grid) in grids.iter().enumerate() {
            near[i] += usize::from(grid.get(*c).is_some_and(|d| d <= 3 * TURN));
        }
    }
    let mut land = vec![0usize; sides];
    for h in g.terrain.hexes().filter(|h| walkable(*g.terrain.get(*h))) {
        if let Some(o) = (0..sides).filter_map(|i| grids[i].get(h).map(|c| (c, i))).min().map(|(_, i)| i) {
            land[o] += 1;
        }
    }
    let enemy: Vec<f64> = (0..sides)
        .map(|i| {
            (0..sides)
                .filter(|j| *j != i)
                .map(|j| {
                    grids[i]
                        .get(places.capitals[j])
                        .map_or(f64::from(places.capitals[i].distance(places.capitals[j]) * 4 + 20), f64::from)
                })
                .fold(f64::MAX, f64::min)
        })
        .collect();
    let ratio = |v: &[f64]| {
        let lo = v.iter().copied().fold(f64::MAX, f64::min).max(1.0);
        v.iter().copied().fold(0.0, f64::max) / lo
    };
    let mut out = vec![
        Fair { name: "neutral cities in a cell (most less fewest)".to_string(), value: spread(&cell), limit: limit[0] },
        Fair {
            name: "neutral cities within 3 turns (most less fewest)".to_string(),
            value: spread(&near),
            limit: limit[1],
        },
        Fair {
            name: "land in a cell (most over least)".to_string(),
            value: ratio(&land.iter().map(|n| *n as f64).collect::<Vec<_>>()),
            limit: limit[2],
        },
        Fair {
            name: "way to the nearest enemy (longest over shortest)".to_string(),
            value: ratio(&enemy),
            limit: limit[3],
        },
    ];
    if ports {
        let mut port = vec![0usize; sides];
        for c in places.neutrals.iter().chain(&places.capitals) {
            if is_port(g, *c) {
                if let Some(o) = owner(*c) {
                    port[o] += 1;
                }
            }
        }
        out.push(Fair {
            name: "ports in a cell (most less fewest)".to_string(),
            value: spread(&port),
            limit: limit[4],
        });
        // (A cell with no water a ship sails on cannot have one.)
        let mut can = vec![false; sides];
        for h in g.terrain.hexes().filter(|h| walkable(*g.terrain.get(*h)) && is_port(g, *h)) {
            if let Some(o) = (0..sides).filter_map(|i| grids[i].get(h).map(|c| (c, i))).min().map(|(_, i)| i) {
                can[o] = true;
            }
        }
        let short = (0..sides).filter(|i| can[*i] && port[*i] < cfg.ports_per_side as usize).count();
        out.push(Fair { name: "cells short of their ports".to_string(), value: short as f64, limit: 0.0 });
    }
    if !places.sea.is_empty() {
        let mut at_sea = vec![0usize; sides];
        for c in &places.sea {
            if let Some(o) = owner(*c) {
                at_sea[o] += 1;
            }
        }
        out.push(Fair {
            name: "cities at sea nearest one capital".to_string(),
            value: *at_sea.iter().max().unwrap_or(&0) as f64,
            limit: limit[5],
        });
    }
    out
}

/// True if a city is a port: by water a ship sails to the sea on (the sea,
/// and the rivers that run into it), not a lake shut off.
fn is_port(g: &Gen, c: Hex) -> bool {
    g.terrain.neighbours(c).any(|n| *g.navigable.get(n))
}

/// The water a ship sails on: the pieces of water at the map's edge (or the
/// biggest), rivers that run into them and all.
fn navigable(g: &Gen) -> Grid<bool> {
    let (rows, cols) = (g.rows(), g.cols());
    let t = &g.terrain;
    let (piece, sizes) = pieces(rows, cols, |h| *t.get(h) == TileType::Water);
    let mut sea = vec![false; sizes.len()];
    for h in t.hexes().filter(|h| t.on_edge(*h)) {
        if let Some(p) = *piece.get(h) {
            sea[p] = true;
        }
    }
    if !sea.contains(&true) {
        if let Some((biggest, _)) = sizes.iter().enumerate().max_by_key(|(i, n)| (**n, std::cmp::Reverse(*i))) {
            sea[biggest] = true;
        }
    }
    Grid::from_fn(rows, cols, |h| piece.get(h).is_some_and(|p| sea[p]))
}

/// The fairness pass: while the neutral cities are shared out unfairly, the
/// side with the most gives up the city it holds most narrowly, and a new
/// one is made in the emptiest place of the side with the fewest; within 3
/// turns of the capitals, likewise; a side short of ports gets a city by the
/// sea. What only the capitals can change (the land, the ways between them)
/// it leaves to the next try.
fn make_fair(g: &Gen, cfg: &Config, plan: &Plan, places: &mut Places, ports: bool) -> Vec<Fair> {
    let (rows, cols) = (g.rows(), g.cols());
    let gap = cfg.min_gap(plan);
    let sides = places.capitals.len();
    let (reached, _) = travel(g, &places.capitals);
    let grids: Vec<Grid<Option<u32>>> = places.capitals.iter().map(|c| travel(g, &[*c]).0).collect();
    for _ in 0..MOVES {
        let fair = fairness(g, cfg, places, (ports, &grids));
        if fair.iter().all(Fair::ok) {
            return fair;
        }
        let cost = |i: usize, h: Hex| grids[i].get(h).unwrap_or(u32::MAX);
        let owner = |h: Hex| (0..sides).min_by_key(|i| (cost(*i, h), *i)).unwrap_or(0);
        // The emptiest place for a city that `fits`, the city that moves
        // there (`give`) gone from where it was.
        let emptiest = |give: Option<usize>, fits: &dyn Fn(Hex) -> bool| {
            let from = give.map(|i| places.neutrals[i]);
            let empty = steps_from(rows, cols, places.all().into_iter().filter(|c| Some(*c) != from), |_| true);
            g.terrain
                .hexes()
                .filter(|h| site(g, cfg, &reached, *h) && fits(*h))
                .filter_map(|h| empty.get(h).filter(|d| *d >= gap).map(|d| (d, h)))
                .max_by(|a, b| a.0.cmp(&b.0).then(g.terrain.index(b.1).cmp(&g.terrain.index(a.1))))
                .map(|(_, h)| h)
        };
        let failing = |name: &str| fair.iter().any(|f| f.name.starts_with(name) && !f.ok());
        let (give, take): (Option<usize>, Option<Hex>) = if failing("neutral cities in a cell") {
            let mut count = vec![0usize; sides];
            for c in &places.neutrals {
                count[owner(*c)] += 1;
            }
            let rich = (0..sides).max_by_key(|i| (count[*i], std::cmp::Reverse(*i))).unwrap_or(0);
            let poor = (0..sides).min_by_key(|i| (count[*i], *i)).unwrap_or(0);
            // The rich side's city held most narrowly: nearest to a tie.
            let narrow = (0..places.neutrals.len()).filter(|i| owner(places.neutrals[*i]) == rich).min_by_key(|i| {
                let h = places.neutrals[*i];
                let second = (0..sides).filter(|s| *s != rich).map(|s| cost(s, h)).min().unwrap_or(u32::MAX);
                (second.saturating_sub(cost(rich, h)), *i)
            });
            (narrow, emptiest(narrow, &|h| owner(h) == poor))
        } else if failing("neutral cities within 3 turns") {
            let mut count = vec![0usize; sides];
            for c in &places.neutrals {
                for (i, n) in count.iter_mut().enumerate() {
                    *n += usize::from(cost(i, *c) <= 3 * TURN);
                }
            }
            let rich = (0..sides).max_by_key(|i| (count[*i], std::cmp::Reverse(*i))).unwrap_or(0);
            let poor = (0..sides).min_by_key(|i| (count[*i], *i)).unwrap_or(0);
            let farthest_near = (0..places.neutrals.len())
                .filter(|i| cost(rich, places.neutrals[*i]) <= 3 * TURN)
                .max_by_key(|i| (cost(rich, places.neutrals[*i]), std::cmp::Reverse(*i)));
            (
                farthest_near,
                emptiest(farthest_near, &|h| {
                    cost(poor, h) <= 3 * TURN && (0..sides).all(|s| s == poor || cost(s, h) > 3 * TURN)
                }),
            )
        } else if failing("ports") || failing("cells short") {
            let mut count = vec![0usize; sides];
            for c in places.neutrals.iter().chain(&places.capitals) {
                if is_port(g, *c) {
                    count[owner(*c)] += 1;
                }
            }
            let poor = (0..sides).min_by_key(|i| (count[*i], *i)).unwrap_or(0);
            // One of its inland cities goes to its coast.
            let inland = (0..places.neutrals.len())
                .filter(|i| owner(places.neutrals[*i]) == poor && !is_port(g, places.neutrals[*i]))
                .max_by_key(|i| (cost(poor, places.neutrals[*i]), std::cmp::Reverse(*i)));
            (inland, emptiest(inland, &|h| owner(h) == poor && is_port(g, h)))
        } else {
            (None, None)
        };
        match (give, take) {
            (Some(i), Some(h)) => places.neutrals[i] = h,
            _ => return fair,
        }
    }
    fairness(g, cfg, places, (ports, &grids))
}

/// Room around each city: it is a city; a mountain beside it is a hill, a
/// swamp grass, and three of its neighbours at least are grass (an air
/// city's ring stays).
fn room(g: &mut Gen, cities: &[Hex], air: &[Hex]) {
    for c in cities {
        g.terrain.set(*c, TileType::City);
        if air.contains(c) {
            continue;
        }
        let around: Vec<Hex> = g.terrain.neighbours(*c).collect();
        for n in &around {
            match *g.terrain.get(*n) {
                TileType::Mountain => g.terrain.set(*n, TileType::Hill),
                TileType::Swamp => g.terrain.set(*n, TileType::Grass),
                _ => {}
            }
        }
        let mut grass = around.iter().filter(|n| *g.terrain.get(**n) == TileType::Grass).count();
        for n in &around {
            if grass >= 3 {
                break;
            }
            if matches!(*g.terrain.get(*n), TileType::Forest | TileType::Hill | TileType::Desert) {
                g.terrain.set(*n, TileType::Grass);
                grass += 1;
            }
        }
    }
}

/// Each city's record: a name (a capital's grand), its holder, walls (a
/// capital the highest of the mix; a neutral drawn from it, one more by the
/// water or in the hills), taxes, what it builds, who stands in it.
fn records(g: &Gen, cfg: &Config, (places, air): (&Places, &[Hex]), kinds: &[UnitKind], rng: &mut Rng) -> Vec<City> {
    let syllables = Syllables::parse(NAMES);
    let class = |k: &UnitKind| movement_class(k);
    let mut foot: Vec<&UnitKind> = kinds.iter().filter(|k| class(k) == MoveClass::Foot && k.commander == 0).collect();
    foot.sort_by_key(|k| (k.price, k.strength, k.id.clone()));
    let leader = kinds.iter().find(|k| k.commander > 0 && class(k) == MoveClass::Foot);
    let ship =
        kinds.iter().filter(|k| class(k) == MoveClass::Naval).max_by_key(|k| (k.carries, std::cmp::Reverse(k.price)));
    let flyer = kinds.iter().filter(|k| class(k) == MoveClass::Flyer).min_by_key(|k| (k.price, k.id.clone()));
    let turns = |k: &UnitKind| if k.commander > 0 { 4 } else { k.price.div_ceil(4).clamp(1, 4) };
    let top_walls = cfg.walls.iter().map(|(w, _)| *w).max().unwrap_or(0);
    let draw_walls = |rng: &mut Rng| {
        let total: f64 = cfg.walls.iter().map(|(_, s)| s).sum();
        let mut x = rng.unit() * total;
        for (w, s) in &cfg.walls {
            if x < *s {
                return *w;
            }
            x -= s;
        }
        top_walls
    };
    let mut taken: Vec<String> = Vec::new();
    let mut out = Vec::new();
    let all: Vec<(Hex, Option<Side>, ReachClass)> = places
        .capitals
        .iter()
        .enumerate()
        .map(|(i, h)| (*h, Some(cfg.colours[i]), ReachClass::Foot))
        .chain(
            places
                .neutrals
                .iter()
                .map(|h| (*h, None, if air.contains(h) { ReachClass::Air } else { ReachClass::Foot })),
        )
        .chain(places.sea.iter().map(|h| (*h, None, ReachClass::Sea)))
        .collect();
    for (hex, side, reach) in all {
        let capital = side.is_some();
        let name = syllables.name(rng, capital, &taken);
        taken.push(name.clone());
        let t = &g.terrain;
        let hills = t.neighbours(hex).filter(|n| *t.get(*n) == TileType::Hill).count();
        let by_water = t.neighbours(hex).any(|n| *t.get(n) == TileType::Water);
        let walls =
            if capital { top_walls } else { (draw_walls(rng) + u32::from(by_water || hills >= 2)).min(top_walls) };
        let mut produce: Vec<(String, u32)> = Vec::new();
        let mut add = |k: Option<&&UnitKind>| {
            if let Some(k) = k {
                if !produce.iter().any(|(id, _)| *id == k.id) {
                    produce.push((k.id.clone(), turns(k)));
                }
            }
        };
        if reach == ReachClass::Air {
            add(flyer.as_ref());
        }
        if capital {
            add(foot.first());
            add(foot.get(1));
            add(foot.iter().max_by_key(|k| (k.moves, std::cmp::Reverse(k.price))));
            add(leader.as_ref());
        } else if hills >= 2 {
            let mut strong = foot.clone();
            strong.sort_by_key(|k| (std::cmp::Reverse(k.strength), k.price));
            add(strong.first());
            add(strong.get(1));
        } else {
            add(foot.first());
            if foot.len() > 1 {
                add(foot.get(1 + rng.below(foot.len() as u64 - 1) as usize));
            }
        }
        if cfg.ports && is_port(g, hex) {
            add(ship.as_ref());
        }
        if !capital && walls == top_walls && rng.below(15) == 0 {
            add(flyer.as_ref());
        }
        let mut garrison: Vec<(String, u32)> = Vec::new();
        if capital {
            if let Some(l) = leader {
                garrison.push((l.id.clone(), 1));
            }
            let soldiers: Vec<&(String, u32)> =
                produce.iter().filter(|(id, _)| foot.iter().any(|k| k.id == *id)).take(2).collect();
            for (i, (id, _)) in soldiers.into_iter().enumerate() {
                garrison.push((id.clone(), if i == 0 { 2 } else { 1 }));
            }
        } else if let Some(k) = foot.first() {
            garrison.push((k.id.clone(), (walls / 2).clamp(1, 8)));
        }
        out.push(City {
            hex,
            side: side.unwrap_or(Side::Grey),
            reach,
            name,
            walls,
            income: if capital { 20 } else { 4 + 2 * walls },
            produce,
            garrison,
        });
    }
    out
}
