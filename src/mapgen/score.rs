//! How near a map comes to what its config asks (`docs/mapgen.md`, the soft
//! metrics): each measure with its target and how far from it is fine, a
//! mark for each, and a score (lower is better: the sum of each miss over its
//! tolerance, squared, at most 4 each).

use std::fmt::Write as _;

use crate::state::TileType;

use super::config::Config;
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

/// A measure, what it should be, and how far from it is fine.
#[derive(Clone, Debug, PartialEq)]
pub struct Metric {
    pub name: String,
    pub target: f64,
    pub value: f64,
    pub tolerance: f64,
}

impl Metric {
    pub fn ok(&self) -> bool {
        (self.value - self.target).abs() <= self.tolerance + 1e-9
    }

    /// Its share of the score: the miss over the tolerance, squared, at most 4.
    pub fn cost(&self) -> f64 {
        let e = (self.value - self.target) / self.tolerance.max(1e-9);
        (e * e).min(4.0)
    }
}

/// The terrain's measures: the water share, each land terrain's share, the
/// same-neighbour scores, and the mountains' edges on hills.
pub fn terrain(cfg: &Config, s: &Stats) -> Vec<Metric> {
    let mut out = vec![Metric {
        name: "water share (%)".to_string(),
        target: cfg.water * 100.0,
        value: s.water_share() * 100.0,
        tolerance: 1.0,
    }];
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
        out.push(Metric { name, target, value, tolerance: 1.0 });
    }
    for (t, same) in SAME {
        let Some(shape) = s.shape_of(t).filter(|sh| sh.hexes > 0) else { continue };
        out.push(Metric {
            name: format!("{} same-neighbour", super::stats::terrain_word(t)),
            target: same,
            value: shape.same,
            tolerance: 0.08,
        });
    }
    if s.shape_of(TileType::Mountain).is_some_and(|m| m.hexes > 0) {
        out.push(Metric {
            name: "mountain edges on hill".to_string(),
            target: cfg.hill_ring,
            value: s.mountains_on_hills(),
            tolerance: 0.08,
        });
    }
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
        let _ = writeln!(
            out,
            "  {:<28} target {:>7.2}  value {:>7.2}  (±{})  {mark}",
            m.name, m.target, m.value, m.tolerance
        );
    }
    let _ = writeln!(out, "  score {:.2}", score(metrics));
    out
}
