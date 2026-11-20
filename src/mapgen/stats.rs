//! What a map measures: its water and land, the mix of its terrain and the
//! shape of it (how much each terrain keeps to itself, its patches, what it
//! borders), its cities (spacing, by travel too, by the water, walls, reach),
//! its sites, its roads, bridges and passes. `mapgen stats` prints these for
//! any map; the generator measures what it makes with the same code, so its
//! targets and its ruler are one.

use std::fmt::Write as _;

use crate::state::{Hex, Map, Side, TileType, step_cost};

use super::grid::{Grid, cheapest_from, median, pieces, steps_from};
use super::reach::{MoveClass, ReachClass, by_water, capitals, city_reach, movement_class, reference_foot};

/// The land terrains, in the order the measures list them.
pub const LAND: [TileType; 6] =
    [TileType::Grass, TileType::Mountain, TileType::Hill, TileType::Forest, TileType::Swamp, TileType::Desert];

/// The terrains whose shape is measured: the land's, and water.
pub const SHAPED: [TileType; 7] = [
    TileType::Grass,
    TileType::Forest,
    TileType::Hill,
    TileType::Mountain,
    TileType::Swamp,
    TileType::Desert,
    TileType::Water,
];

/// The least, the middle, the mean and the most of some numbers.
#[derive(Copy, Clone, Debug, Default, PartialEq)]
pub struct Summary {
    pub min: u32,
    pub median: f64,
    pub mean: f64,
    pub max: u32,
    pub count: usize,
}

impl Summary {
    pub fn of(values: &[u32]) -> Summary {
        if values.is_empty() {
            return Summary::default();
        }
        let sum: u64 = values.iter().map(|v| u64::from(*v)).sum();
        Summary {
            min: *values.iter().min().unwrap_or(&0),
            median: median(values),
            mean: sum as f64 / values.len() as f64,
            max: *values.iter().max().unwrap_or(&0),
            count: values.len(),
        }
    }
}

/// How one terrain lies: the average share of its hexes' neighbours that are
/// the same (and what a random scatter would give: its share of the map),
/// its patches, their middle size and the biggest.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Shape {
    pub terrain: Option<TileType>,
    pub hexes: usize,
    pub same: f64,
    pub random: f64,
    pub patches: usize,
    pub median_patch: f64,
    pub largest: Vec<usize>,
    /// Of its edges with other terrains, each other terrain's share.
    pub borders: Vec<(TileType, f64)>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CityStats {
    pub count: usize,
    pub capitals: usize,
    /// The nearest other city, in hexes; the second nearest's median.
    pub nearest: Summary,
    pub second_median: f64,
    /// The nearest other city by travel cost of the reference unit on foot.
    pub travel: Summary,
    pub by_water: usize,
    /// The terrain of the six hexes around the cities.
    pub ring: Vec<(TileType, f64)>,
    pub edge_min: u32,
    /// Each capital's nearest other capital, in hexes, the nearest first.
    pub capital_gaps: Vec<u32>,
    /// How many cities have each level of walls.
    pub walls: Vec<(u32, usize)>,
    /// How many are reached on foot, by sea, by air.
    pub reach: [usize; 3],
    pub ship_builders: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SiteStats {
    pub temples: usize,
    pub ruins: usize,
    pub to_city: Summary,
    pub to_site: Summary,
    pub ring: Vec<(TileType, f64)>,
    pub on_road: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RoadStats {
    pub hexes: usize,
    pub under: Vec<(TileType, f64)>,
    pub cities_on_road: usize,
    /// The cities on each road network, the most first.
    pub networks: Vec<usize>,
    /// Road hexes with exactly two road neighbours (a city counts), and with
    /// one: dead ends.
    pub lines: usize,
    pub dead_ends: usize,
    /// Each bridge's and each pass's length in hexes, the longest first.
    pub bridges: Vec<usize>,
    pub passes: Vec<usize>,
}

/// Everything a map measures.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stats {
    pub rows: u16,
    pub cols: u16,
    pub water: usize,
    /// The biggest piece of water, and the others (lakes), the biggest first.
    pub sea: usize,
    pub lakes: Vec<usize>,
    /// Water 3 hexes or more from land; water no more than 5 wide some way
    /// across (rivers, straits).
    pub open_water: usize,
    pub narrow_water: usize,
    pub edge: usize,
    pub edge_water: usize,
    pub land: usize,
    pub coastal_land: usize,
    pub land_pieces: Vec<usize>,
    /// Land walled in by mountains (`pockets`): each pocket's size, the
    /// biggest first.
    pub pockets: Vec<usize>,
    /// How many land hexes each land terrain has (`LAND`), and the places
    /// (cities, temples, ruins).
    pub mix: Vec<(TileType, usize)>,
    pub places: usize,
    pub shapes: Vec<Shape>,
    pub cities: CityStats,
    pub sites: SiteStats,
    pub roads: RoadStats,
}

fn kind(map: &Map, hex: Hex) -> TileType {
    map.tile(hex).map_or(TileType::Water, |t| t.kind)
}

fn is_place(t: TileType) -> bool {
    matches!(t, TileType::City | TileType::Temple | TileType::Ruin)
}

/// Shares of a list of terrains, the most first.
fn shares(counts: &[(TileType, usize)]) -> Vec<(TileType, f64)> {
    let total: usize = counts.iter().map(|(_, n)| n).sum();
    let mut out: Vec<(TileType, f64)> =
        counts.iter().filter(|(_, n)| *n > 0).map(|(t, n)| (*t, *n as f64 / total.max(1) as f64)).collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.index().cmp(&b.0.index())));
    out
}

fn count_into(counts: &mut Vec<(TileType, usize)>, t: TileType) {
    match counts.iter_mut().find(|(k, _)| *k == t) {
        Some((_, n)) => *n += 1,
        None => counts.push((t, 1)),
    }
}

/// The terrain of the hexes around `places`.
fn ring_of(map: &Map, places: &[Hex]) -> Vec<(TileType, f64)> {
    let mut counts = Vec::new();
    for hex in places {
        for n in map.neighbours(*hex) {
            count_into(&mut counts, kind(map, n));
        }
    }
    shares(&counts)
}

impl Stats {
    pub fn of(map: &Map) -> Stats {
        let (rows, cols) = (map.rows(), map.cols());
        let mut s = Stats { rows, cols, ..Stats::default() };
        let water = |h: Hex| kind(map, h) == TileType::Water;
        let all = Grid::new(rows, cols, ());

        // Water and land.
        let (_, mut water_pieces) = pieces(rows, cols, water);
        water_pieces.sort_unstable_by(|a, b| b.cmp(a));
        s.water = water_pieces.iter().sum();
        s.sea = water_pieces.first().copied().unwrap_or(0);
        s.lakes = water_pieces.iter().skip(1).copied().collect();
        let land_hexes = all.hexes().filter(|h| !water(*h));
        let to_land = steps_from(rows, cols, land_hexes, |_| true);
        s.open_water = all.hexes().filter(|h| water(*h) && to_land.get(*h).is_some_and(|d| d >= 3)).count();
        s.narrow_water = all.hexes().filter(|h| water(*h) && narrow(map, *h)).count();
        s.edge = all.hexes().filter(|h| all.on_edge(*h)).count();
        s.edge_water = all.hexes().filter(|h| all.on_edge(*h) && water(*h)).count();
        s.land = all.len() - s.water;
        s.coastal_land = all.hexes().filter(|h| !water(*h) && map.neighbours(*h).any(water)).count();
        let (_, mut land_pieces) = pieces(rows, cols, |h| !water(h));
        land_pieces.sort_unstable_by(|a, b| b.cmp(a));
        s.land_pieces = land_pieces;
        let mut walled: Vec<usize> = pockets(rows, cols, &|h| kind(map, h)).iter().map(Vec::len).collect();
        walled.sort_unstable_by(|a, b| b.cmp(a));
        s.pockets = walled;

        // The mix and the shape of the terrain.
        for t in LAND {
            s.mix.push((t, all.hexes().filter(|h| kind(map, *h) == t).count()));
        }
        s.places = all.hexes().filter(|h| is_place(kind(map, *h))).count();
        for t in SHAPED {
            s.shapes.push(shape(map, t));
        }

        s.cities = cities(map);
        s.sites = sites(map);
        s.roads = roads(map);
        s
    }

    /// A land terrain's share of the land, in percent.
    pub fn land_share(&self, t: TileType) -> f64 {
        let n = self.mix.iter().find(|(k, _)| *k == t).map_or(0, |(_, n)| *n);
        100.0 * n as f64 / self.land.max(1) as f64
    }

    /// Water's share of the map.
    pub fn water_share(&self) -> f64 {
        self.water as f64 / (usize::from(self.rows) * usize::from(self.cols)).max(1) as f64
    }

    /// The shape of a terrain.
    pub fn shape_of(&self, t: TileType) -> Option<&Shape> {
        self.shapes.iter().find(|s| s.terrain == Some(t))
    }

    /// The share of mountain edges that border hills.
    pub fn mountains_on_hills(&self) -> f64 {
        let mountain = self.shape_of(TileType::Mountain);
        mountain.and_then(|m| m.borders.iter().find(|(t, _)| *t == TileType::Hill)).map_or(0.0, |(_, s)| *s)
    }

    /// All of it, in lines of text.
    pub fn report(&self) -> String {
        let mut out = String::new();
        let hexes = usize::from(self.rows) * usize::from(self.cols);
        let pct = |n: usize, of: usize| 100.0 * n as f64 / of.max(1) as f64;
        let _ = writeln!(out, "size {} x {} = {hexes} hexes", self.rows, self.cols);
        let _ = writeln!(
            out,
            "water {:.1}% ({} hexes), land {:.1}%",
            pct(self.water, hexes),
            self.water,
            pct(self.land, hexes)
        );
        let lakes: Vec<String> = self.lakes.iter().take(8).map(|n| n.to_string()).collect();
        let _ = writeln!(out, "  sea {} hexes; {} lakes: {}", self.sea, self.lakes.len(), lakes.join(", "));
        let _ = writeln!(
            out,
            "  open water {} ({:.0}% of water), narrow water {} ({:.0}% of water)",
            self.open_water,
            pct(self.open_water, self.water),
            self.narrow_water,
            pct(self.narrow_water, self.water)
        );
        let _ = writeln!(out, "  map edge that is water {:.0}%", pct(self.edge_water, self.edge));
        let _ = writeln!(out, "  coastal land {:.1}% of land", pct(self.coastal_land, self.land));
        let pieces: Vec<String> = self.land_pieces.iter().take(8).map(|n| n.to_string()).collect();
        let _ = writeln!(out, "  land pieces {}: {}", self.land_pieces.len(), pieces.join(" / "));
        let walled: Vec<String> = self.pockets.iter().take(8).map(|n| n.to_string()).collect();
        let _ = writeln!(
            out,
            "  walled in by mountains: {} hexes in {} pockets: {}",
            self.pockets.iter().sum::<usize>(),
            self.pockets.len(),
            walled.join(", ")
        );
        let mix: Vec<String> =
            LAND.iter().map(|t| format!("{} {:.1}%", terrain_word(*t), self.land_share(*t))).collect();
        let _ = writeln!(out, "land mix: {}, places {:.1}%", mix.join(", "), pct(self.places, self.land));
        let _ = writeln!(out, "shape (same-neighbour (random), patches, median, largest; borders):");
        for sh in self.shapes.iter().filter(|sh| sh.hexes > 0) {
            let largest: Vec<String> = sh.largest.iter().map(|n| n.to_string()).collect();
            let borders: Vec<String> =
                sh.borders.iter().take(4).map(|(t, f)| format!("{} {:.0}%", terrain_word(*t), f * 100.0)).collect();
            let _ = writeln!(
                out,
                "  {:<8} {:.2} ({:.2}), {} patches, median {}, largest {}; {}",
                sh.terrain.map_or("?", terrain_word),
                sh.same,
                sh.random,
                sh.patches,
                sh.median_patch,
                largest.join(", "),
                borders.join(", ")
            );
        }
        let c = &self.cities;
        let _ = writeln!(out, "cities {} ({} capitals)", c.count, c.capitals);
        if c.count > 0 {
            let _ = writeln!(
                out,
                "  hexes per city {:.0}, of land {:.0}",
                hexes as f64 / c.count as f64,
                self.land as f64 / c.count as f64
            );
            let _ = writeln!(
                out,
                "  nearest city: min {}, median {}, mean {:.1}, max {}; second nearest median {}",
                c.nearest.min, c.nearest.median, c.nearest.mean, c.nearest.max, c.second_median
            );
            let _ = writeln!(
                out,
                "  nearest by travel (10 moves on foot): median {}, mean {:.1}, max {}",
                c.travel.median, c.travel.mean, c.travel.max
            );
            let _ = writeln!(out, "  by the water {} ({:.0}%)", c.by_water, pct(c.by_water, c.count));
            let _ = writeln!(out, "  around them: {}", words(&c.ring));
            let _ = writeln!(out, "  nearest to an edge {}", c.edge_min);
            let gaps: Vec<String> = c.capital_gaps.iter().map(|g| g.to_string()).collect();
            let _ = writeln!(out, "  capitals' nearest capital: {}", gaps.join(", "));
            let walls: Vec<String> = c.walls.iter().map(|(w, n)| format!("{w}: {n}")).collect();
            let _ = writeln!(out, "  walls {}", walls.join(", "));
            let _ = writeln!(out, "  reach: foot {}, sea {}, air {}", c.reach[0], c.reach[1], c.reach[2]);
            let _ = writeln!(out, "  build ships {}", c.ship_builders);
        }
        let si = &self.sites;
        let _ = writeln!(out, "sites {} ({} temples, {} ruins)", si.temples + si.ruins, si.temples, si.ruins);
        if si.temples + si.ruins > 0 {
            let _ = writeln!(
                out,
                "  to the nearest city: min {}, median {}, max {}; to the nearest site: min {}, median {}",
                si.to_city.min, si.to_city.median, si.to_city.max, si.to_site.min, si.to_site.median
            );
            let _ = writeln!(out, "  around them: {}; on a road {}", words(&si.ring), si.on_road);
        }
        let r = &self.roads;
        let _ = writeln!(
            out,
            "roads {} hexes ({:.1} per city), under them: {}",
            r.hexes,
            r.hexes as f64 / c.count.max(1) as f64,
            words(&r.under)
        );
        let networks: Vec<String> = r.networks.iter().map(|n| n.to_string()).collect();
        let _ = writeln!(
            out,
            "  cities on a road {} ({:.0}%); {} networks, cities on each: {}",
            r.cities_on_road,
            pct(r.cities_on_road, c.count),
            r.networks.len(),
            networks.join(", ")
        );
        let _ =
            writeln!(out, "  {:.0}% in lines (two road neighbours), {} dead ends", pct(r.lines, r.hexes), r.dead_ends);
        let lengths = |v: &[usize]| v.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(", ");
        let _ = writeln!(out, "  bridges {}: {}", r.bridges.len(), lengths(&r.bridges));
        let _ = writeln!(out, "  passes {}: {}", r.passes.len(), lengths(&r.passes));
        out
    }
}

/// A terrain's word, as the stats say it.
pub fn terrain_word(t: TileType) -> &'static str {
    match t {
        TileType::Grass => "grass",
        TileType::Forest => "forest",
        TileType::Swamp => "swamp",
        TileType::Desert => "desert",
        TileType::Hill => "hill",
        TileType::Mountain => "mountain",
        TileType::Water => "water",
        TileType::Temple => "temple",
        TileType::Ruin => "ruin",
        TileType::City => "city",
    }
}

fn words(shares: &[(TileType, f64)]) -> String {
    shares.iter().map(|(t, f)| format!("{} {:.0}%", terrain_word(*t), f * 100.0)).collect::<Vec<_>>().join(", ")
}

/// Land walled in by mountains: each piece of the map that mountains alone
/// cut off from the rest (water does not: a river is bridged, the sea
/// sailed), the land of it. A lake in a ring of mountains is one too.
pub fn pockets(rows: u16, cols: u16, kind: &dyn Fn(Hex) -> TileType) -> Vec<Vec<Hex>> {
    let (piece, sizes) = pieces(rows, cols, |h| kind(h) != TileType::Mountain);
    let Some(biggest) = sizes.iter().enumerate().max_by_key(|(i, n)| (**n, std::cmp::Reverse(*i))).map(|(i, _)| i)
    else {
        return Vec::new();
    };
    let all = Grid::new(rows, cols, ());
    let mut hexes: Vec<Vec<Hex>> = vec![Vec::new(); sizes.len()];
    for h in all.hexes() {
        if let Some(p) = *piece.get(h) {
            if kind(h) != TileType::Water {
                hexes[p].push(h);
            }
        }
    }
    hexes.into_iter().enumerate().filter(|(p, h)| *p != biggest && !h.is_empty()).map(|(_, h)| h).collect()
}

/// True if water at `hex` is no more than 5 wide some way across: along one
/// of the three lines through it, land within 2 steps on both sides.
fn narrow(map: &Map, hex: Hex) -> bool {
    let land_within = |side: crate::state::HexSide| {
        let mut at = hex;
        for _ in 0..3 {
            let (r, c) = at.toward(side);
            let Some(next) = Hex::on_map(r, c, map.rows(), map.cols()) else { return false };
            if kind(map, next) != TileType::Water {
                return true;
            }
            at = next;
        }
        false
    };
    (0..3).any(|i| land_within(crate::state::HexSide::ALL[i]) && land_within(crate::state::HexSide::ALL[i + 3]))
}

fn shape(map: &Map, t: TileType) -> Shape {
    let (rows, cols) = (map.rows(), map.cols());
    let is = |h: Hex| kind(map, h) == t;
    let all = Grid::new(rows, cols, ());
    let hexes: Vec<Hex> = all.hexes().filter(|h| is(*h)).collect();
    let mut sh = Shape { terrain: Some(t), hexes: hexes.len(), ..Shape::default() };
    if hexes.is_empty() {
        return sh;
    }
    let mut same = 0.0;
    let mut borders = Vec::new();
    for hex in &hexes {
        let around: Vec<Hex> = map.neighbours(*hex).collect();
        let alike = around.iter().filter(|h| is(**h)).count();
        same += alike as f64 / around.len().max(1) as f64;
        for n in around.iter().filter(|h| !is(**h)) {
            count_into(&mut borders, kind(map, *n));
        }
    }
    sh.same = same / hexes.len() as f64;
    sh.random = hexes.len() as f64 / all.len() as f64;
    let (_, mut sizes) = pieces(rows, cols, is);
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    sh.patches = sizes.len();
    let small: Vec<u32> = sizes.iter().map(|n| *n as u32).collect();
    sh.median_patch = median(&small);
    sh.largest = sizes.iter().take(4).copied().collect();
    sh.borders = shares(&borders);
    sh
}

fn cities(map: &Map) -> CityStats {
    let list: Vec<Hex> = map.cities().iter().map(|c| c.pos).collect();
    let mut c = CityStats { count: list.len(), ..CityStats::default() };
    if list.is_empty() {
        return c;
    }
    c.capitals = map.cities().iter().filter(|x| x.side != Side::Grey).count();
    let all = Grid::new(map.rows(), map.cols(), ());
    let mut nearest = Vec::new();
    let mut second = Vec::new();
    for (i, a) in list.iter().enumerate() {
        let mut d: Vec<u32> = list.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, b)| a.distance(*b)).collect();
        d.sort_unstable();
        nearest.extend(d.first());
        second.extend(d.get(1));
    }
    c.nearest = Summary::of(&nearest);
    c.second_median = median(&second);
    // By travel: from each city, the cheapest way to another.
    let foot = reference_foot();
    let mut travel = Vec::new();
    for a in &list {
        let (cost, _) = cheapest_from(map.rows(), map.cols(), &[(*a, 0)], |from, to| {
            step_cost(map, &foot, from, to).map(u32::from)
        });
        let best = list.iter().filter(|b| *b != a).filter_map(|b| *cost.get(*b)).min();
        travel.extend(best);
    }
    c.travel = Summary::of(&travel);
    c.by_water = list.iter().filter(|h| by_water(map, **h)).count();
    c.ring = ring_of(map, &list);
    c.edge_min = list.iter().map(|h| u32::from(all.edge_distance(*h))).min().unwrap_or(0);
    let caps: Vec<Hex> = capitals(map).iter().map(|x| x.pos).collect();
    if caps.len() > 1 {
        let mut gaps: Vec<u32> = caps
            .iter()
            .enumerate()
            .filter_map(|(i, a)| caps.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, b)| a.distance(*b)).min())
            .collect();
        gaps.sort_unstable();
        c.capital_gaps = gaps;
    }
    for city in map.cities().iter() {
        match c.walls.iter_mut().find(|(w, _)| *w == city.walls) {
            Some((_, n)) => *n += 1,
            None => c.walls.push((city.walls, 1)),
        }
    }
    c.walls.sort_unstable();
    for r in city_reach(map) {
        c.reach[match r {
            ReachClass::Foot => 0,
            ReachClass::Sea => 1,
            ReachClass::Air => 2,
        }] += 1;
    }
    let units = map.units();
    let naval = |name: &str| {
        units.kind_named(name).and_then(|k| units.kind(k)).is_some_and(|k| movement_class(k) == MoveClass::Naval)
    };
    c.ship_builders = map.cities().iter().filter(|x| x.units.iter().any(|p| naval(&p.basename))).count();
    c
}

fn sites(map: &Map) -> SiteStats {
    let all = Grid::new(map.rows(), map.cols(), ());
    let list: Vec<Hex> = all.hexes().filter(|h| matches!(kind(map, *h), TileType::Temple | TileType::Ruin)).collect();
    let mut s = SiteStats {
        temples: list.iter().filter(|h| kind(map, **h) == TileType::Temple).count(),
        ruins: list.iter().filter(|h| kind(map, **h) == TileType::Ruin).count(),
        ..SiteStats::default()
    };
    let cities: Vec<Hex> = map.cities().iter().map(|c| c.pos).collect();
    let to_city: Vec<u32> = list.iter().filter_map(|a| cities.iter().map(|b| a.distance(*b)).min()).collect();
    let to_site: Vec<u32> = list
        .iter()
        .enumerate()
        .filter_map(|(i, a)| list.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, b)| a.distance(*b)).min())
        .collect();
    s.to_city = Summary::of(&to_city);
    s.to_site = Summary::of(&to_site);
    s.ring = ring_of(map, &list);
    s.on_road = list.iter().filter(|h| map.tile(**h).is_some_and(|t| t.road)).count();
    s
}

fn roads(map: &Map) -> RoadStats {
    let (rows, cols) = (map.rows(), map.cols());
    let all = Grid::new(rows, cols, ());
    let road = |h: Hex| map.tile(h).is_some_and(|t| t.road);
    let city = |h: Hex| kind(map, h) == TileType::City;
    let list: Vec<Hex> = all.hexes().filter(|h| road(*h)).collect();
    let mut r = RoadStats { hexes: list.len(), ..RoadStats::default() };
    let mut under = Vec::new();
    for h in &list {
        count_into(&mut under, kind(map, *h));
    }
    r.under = shares(&under);
    let on_road: Vec<Hex> = map.cities().iter().map(|c| c.pos).filter(|c| map.neighbours(*c).any(road)).collect();
    r.cities_on_road = on_road.len();
    // A network: road hexes and the cities on them, joined.
    let (piece, sizes) = pieces(rows, cols, |h| road(h) || (city(h) && map.neighbours(h).any(road)));
    let mut networks = vec![0usize; sizes.len()];
    for c in &on_road {
        if let Some(p) = *piece.get(*c) {
            networks[p] += 1;
        }
    }
    networks.sort_unstable_by(|a, b| b.cmp(a));
    r.networks = networks;
    for h in &list {
        let n = map.neighbours(*h).filter(|x| road(*x) || city(*x)).count();
        r.lines += usize::from(n == 2);
        r.dead_ends += usize::from(n == 1);
    }
    let runs = |t: TileType| {
        let (_, mut sizes) = pieces(rows, cols, |h| road(h) && kind(map, h) == t);
        sizes.sort_unstable_by(|a, b| b.cmp(a));
        sizes
    };
    r.bridges = runs(TileType::Water);
    r.passes = runs(TileType::Mountain);
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::scenario;

    /// 20 x 20: water in the west three columns, a mountain range down
    /// column 7 with hills on both sides, forest in its last four rows east
    /// of it, grass elsewhere; a road along row 2 from a city at (2, 3) over a
    /// pass to a city at (2, 9).
    fn map() -> Map {
        let east = "g".repeat(10);
        let mut rows: Vec<String> = (0..20).map(|_| format!("wwwggghmhg{east}")).collect();
        for row in rows.iter_mut().skip(16) {
            *row = format!("wwwggghmff{east}");
        }
        rows[2] = format!("wwwcGGHMHc{east}");
        let terrain = format!("20\n20\n{}\n", rows.join("\n"));
        let units = "unit a\nmoves 10\nenter grass 2\nunit boat\nmoves 10\ntravels sea\nenter water 1, city 1\n";
        let cities = "city 2 3\nname West\nside red\nwalls 2\nproduce boat 3\ncity 2 9\nname East\nwalls 3\n";
        scenario::parse(&terrain, Some(units), Some(cities)).unwrap().0
    }

    #[test]
    fn the_water_and_the_land_are_counted() {
        let s = Stats::of(&map());
        assert_eq!((s.water, s.sea, s.land, s.lakes.len()), (60, 60, 340, 0));
        assert_eq!(s.edge_water, 3 + 3 + 20 - 2, "the top and bottom rows' three, and the west edge");
        assert_eq!(s.coastal_land, 20, "column 3 touches the water");
        assert_eq!(s.land_pieces, [340]);
        // Beyond the water is the map's edge, not land: none of it is narrow;
        // its west column is 3 from land, open water.
        assert_eq!((s.narrow_water, s.open_water), (0, 20));
        assert!((s.water_share() - 0.15).abs() < 1e-9);
    }

    #[test]
    fn the_mix_and_the_shape_of_the_terrain() {
        let s = Stats::of(&map());
        // Mountains: column 7, the pass too; hills on both sides.
        assert_eq!(s.mix.iter().find(|(t, _)| *t == TileType::Mountain).map(|(_, n)| *n), Some(20));
        assert_eq!(s.places, 2);
        let m = s.shape_of(TileType::Mountain).unwrap();
        assert_eq!((m.patches, m.largest[0]), (1, 20));
        assert!(m.same > 0.3 && m.same < 0.4, "{}", m.same);
        assert!(s.mountains_on_hills() > 0.7, "{}", s.mountains_on_hills());
        assert_eq!(s.shape_of(TileType::Forest).unwrap().patches, 1);
    }

    #[test]
    fn the_cities_sites_and_roads_are_measured() {
        let s = Stats::of(&map());
        let c = &s.cities;
        assert_eq!((c.count, c.capitals, c.nearest.min, c.by_water, c.edge_min), (2, 1, 6, 1, 2));
        assert_eq!(c.walls, [(2, 1), (3, 1)]);
        // East is over the pass, on foot.
        assert_eq!(c.reach, [2, 0, 0]);
        assert_eq!(c.ship_builders, 1);
        assert_eq!(c.travel.min, 6, "five road steps and a city");
        let r = &s.roads;
        assert_eq!((r.hexes, r.cities_on_road, r.networks.clone()), (5, 2, vec![2]));
        assert_eq!((r.lines, r.dead_ends), (5, 0));
        assert_eq!((r.bridges.len(), r.passes.clone()), (0, vec![1]));
    }
}
