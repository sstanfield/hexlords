//! How near a map comes to what its config asks (`docs/mapgen.md`, the soft
//! metrics): each measure with its target and how far from it is fine, a
//! mark for each, and a score (lower is better: the sum of each miss over its
//! tolerance, squared, at most 4 each).

use std::fmt::Write as _;

use crate::state::TileType;

use super::config::{Amount, Config, Plan, Roads};
use super::stats::Stats;

/// The target map's same-neighbour scores: how much each terrain keeps to
/// itself.
pub const SAME: [(TileType, f64); 6] = [
    (TileType::Grass, 0.84),
    (TileType::Forest, 0.79),
    (TileType::Hill, 0.60),
    (TileType::Mountain, 0.80),
    (TileType::Swamp, 0.82),
    (TileType::Water, 0.87),
];

/// A measure, what it should be (a value, or from `target` up to `upto`),
/// and how far from it is fine.
#[derive(Clone, Debug, PartialEq)]
pub struct Metric {
    pub name: String,
    pub target: f64,
    pub value: f64,
    pub tolerance: f64,
    pub upto: Option<f64>,
}

impl Metric {
    fn new(name: &str, target: f64, value: f64, tolerance: f64) -> Metric {
        Metric { name: name.to_string(), target, value, tolerance, upto: None }
    }

    fn range(name: &str, (lo, hi): (f64, f64), value: f64, tolerance: f64) -> Metric {
        Metric { name: name.to_string(), target: lo, value, tolerance, upto: Some(hi) }
    }

    /// How far it is from what it should be.
    fn miss(&self) -> f64 {
        match self.upto {
            Some(hi) if self.value > hi => self.value - hi,
            Some(_) => (self.target - self.value).max(0.0),
            None => (self.value - self.target).abs(),
        }
    }

    pub fn ok(&self) -> bool {
        self.miss() <= self.tolerance + 1e-9
    }

    /// Its share of the score: the miss over the tolerance, squared, at most 4.
    pub fn cost(&self) -> f64 {
        let e = self.miss() / self.tolerance.max(1e-9);
        (e * e).min(4.0)
    }
}

/// The terrain's measures: the water share, each land terrain's share, the
/// same-neighbour scores, and the mountains' edges on hills.
pub fn terrain(cfg: &Config, s: &Stats) -> Vec<Metric> {
    let mut out = vec![Metric::new("water share (%)", cfg.water * 100.0, s.water_share() * 100.0, 1.0)];
    // Grass with the places (cities, temples, ruins): the mix leaves room for
    // them, and they stand on what would be grass.
    let places = 100.0 * s.places as f64 / s.land.max(1) as f64;
    for (i, t) in super::stats::LAND.iter().enumerate() {
        let (target, value, name) = if *t == TileType::Grass {
            (
                100.0 - cfg.mix[1..].iter().sum::<f64>(),
                s.land_share(*t) + places,
                "grass and places (% of land)".to_string(),
            )
        } else {
            (cfg.mix[i], s.land_share(*t), format!("{} (% of land)", super::stats::terrain_word(*t)))
        };
        out.push(Metric::new(&name, target, value, 1.0));
    }
    for (t, same) in SAME {
        let Some(shape) = s.shape_of(t).filter(|sh| sh.hexes > 0) else { continue };
        out.push(Metric::new(&format!("{} same-neighbour", super::stats::terrain_word(t)), same, shape.same, 0.08));
    }
    out.push(Metric::new("land walled in by mountains", 0.0, s.pockets.iter().sum::<usize>() as f64, 5.0));
    if s.shape_of(TileType::Mountain).is_some_and(|m| m.hexes > 0) {
        out.push(Metric::new("mountain edges on hill", cfg.hill_ring, s.mountains_on_hills(), 0.08));
    }
    out
}

/// The cities' and the roads' measures: the nearest city (as near as an
/// even spread over the land puts them, about 0.9 x the square root of the
/// density: a perfect packing would be 1.07; the owner wanted no land
/// wasted, so ours are farther apart than the target map's 8), cities by
/// the water,
/// cities on foot on a road, road hexes for each city, roads in clean lines,
/// bridges for each city, ports among the cities by the water.
pub fn cities_and_roads(cfg: &Config, plan: &Plan, s: &Stats) -> Vec<Metric> {
    let c = &s.cities;
    let r = &s.roads;
    if c.count == 0 {
        return Vec::new();
    }
    let n = c.count as f64;
    let mut out = vec![
        Metric::new("nearest city, median (hexes)", 0.9 * plan.density.sqrt(), c.nearest.median, 1.5),
        Metric::new("cities by the water (share)", cfg.coastal_cities, c.by_water as f64 / n, 0.08),
    ];
    let foot = c.reach[0].max(1) as f64;
    let on_road = (r.cities_on_road as f64 / foot).min(1.0);
    match cfg.roads {
        Roads::None => {}
        Roads::Sparse => {
            out.push(Metric::range("cities on foot on a road (share)", (0.55, 0.7), on_road, 0.05));
            out.push(Metric::range("road hexes for each city", (4.0, 11.0), r.hexes as f64 / n, 1.5));
        }
        Roads::Linked | Roads::Dense => {
            out.push(Metric::range("cities on foot on a road (share)", (1.0, 1.0), on_road, 0.02));
            let per = if cfg.roads == Roads::Linked { (9.0, 15.0) } else { (11.0, 19.0) };
            out.push(Metric::range("road hexes for each city", per, r.hexes as f64 / n, 1.5));
        }
    }
    if r.hexes > 0 {
        out.push(Metric::range("road hexes in clean lines (share)", (0.8, 1.0), r.lines as f64 / r.hexes as f64, 0.05));
    }
    let rivers = !matches!(cfg.rivers, Amount::Count(0)) && !matches!(cfg.rivers, Amount::Rate(x) if x <= 0.0);
    if cfg.bridges && cfg.roads != Roads::None && rivers {
        out.push(Metric::range("bridges for each city", (1.0 / 8.0, 1.0 / 5.0), r.bridges.len() as f64 / n, 0.04));
    }
    if cfg.ports && c.by_water > 0 {
        out.push(Metric::range(
            "ports among cities by the water",
            (0.5, 0.9),
            c.ship_builders as f64 / c.by_water as f64,
            0.1,
        ));
    }
    out
}

/// Every measure: the terrain's, then the cities' and the roads'.
pub fn all(cfg: &Config, plan: &Plan, s: &Stats) -> Vec<Metric> {
    let mut out = terrain(cfg, s);
    out.extend(cities_and_roads(cfg, plan, s));
    out
}

/// The sum of the measures' costs.
pub fn score(metrics: &[Metric]) -> f64 {
    metrics.iter().map(Metric::cost).sum()
}

/// The measures in a table: target, value, and a mark.
pub fn table(metrics: &[Metric]) -> String {
    let mut out = String::new();
    for m in metrics {
        let mark = if m.ok() { "ok" } else { "MISS" };
        let target = match m.upto {
            Some(hi) if hi > m.target => format!("{:.2}..{:.2}", m.target, hi),
            _ => format!("{:.2}", m.target),
        };
        let _ =
            writeln!(out, "  {:<36} target {:>11}  value {:>7.2}  (±{})  {mark}", m.name, target, m.value, m.tolerance);
    }
    let _ = writeln!(out, "  score {:.2}", score(metrics));
    out
}
