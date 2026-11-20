//! The random map generator (`mapgen`): from a seed and a few knobs, a whole
//! map directory in the game's format (`docs/map-format.md`), with a report
//! of what it measures and pictures of it (`docs/mapgen.md`).
//!
//! It is pure: no files, no clock, no state of its own beyond what it is
//! given, so the same seed and knobs make the same map on every machine, and
//! the game can call it for a new random map one day. Its stages each draw on
//! their own stream of chance ([`rng`]), so changing what comes later leaves
//! what came before as it was.
//!
//! The stages, in order: [`land`] (land and water), [`relief`] (mountains and
//! hills), [`biomes`] (forest, swamp, desert, grass), [`cleanup`]. Rivers,
//! cities, sites and roads are to come (the plan's milestones G3 to G6).
//!
//! - [`config`]: the knobs, the presets, the size.
//! - [`grid`]: the hex grid's helpers: walks, shortest ways, pieces, the
//!   quantile pick.
//! - [`noise`], [`rng`]: the same on every machine.
//! - [`reach`]: movement classes, and who gets where.
//! - [`stats`]: what any map measures (`mapgen stats`).
//! - [`check`]: what `mapgen check` says beyond what the reader refuses.
//! - [`preview`]: pictures of maps, stages, and contact sheets.
//! - [`score`]: how near a map comes to its targets.
//!
//! Not a port: the C++ had no maps but one.

pub mod biomes;
pub mod check;
pub mod cleanup;
pub mod config;
pub mod grid;
pub mod land;
pub mod noise;
pub mod preview;
pub mod reach;
pub mod relief;
pub mod rng;
pub mod score;
pub mod stats;

use crate::state::scenario::{self, parse_units};
use crate::state::{Map, TileType};

use config::{Config, Plan};
use grid::Grid;

/// What the stages share as they make a map: its terrain and roads, and the
/// grids they work from (kept, to be drawn: `mapgen gen --layers`).
#[derive(Clone, Debug)]
pub struct Gen {
    pub terrain: Grid<TileType>,
    pub road: Grid<bool>,
    /// 0 (the deepest sea) to 1 (the highest land).
    pub elevation: Grid<f64>,
    pub moisture: Grid<f64>,
    /// Where mountains are wanted, the land's.
    pub ridge: Grid<f64>,
    /// The land made for cities only ships reach.
    pub island: Grid<bool>,
}

impl Gen {
    fn new(rows: u16, cols: u16) -> Gen {
        Gen {
            terrain: Grid::new(rows, cols, TileType::Grass),
            road: Grid::new(rows, cols, false),
            elevation: Grid::new(rows, cols, 0.0),
            moisture: Grid::new(rows, cols, 0.0),
            ridge: Grid::new(rows, cols, 0.0),
            island: Grid::new(rows, cols, false),
        }
    }

    pub fn rows(&self) -> u16 {
        self.terrain.rows()
    }

    pub fn cols(&self) -> u16 {
        self.terrain.cols()
    }
}

/// A map the generator made: what it was made with, its terrain, and the
/// units it is played with.
#[derive(Clone, Debug)]
pub struct Generated {
    pub config: Config,
    pub plan: Plan,
    pub state: Gen,
    /// The units template's text, copied into the map.
    pub units: String,
}

/// Make a map: `units` is the units template's text (`template` names it, for
/// what is said of it).
pub fn generate(config: &Config, units: &str, template: &str) -> Result<Generated, String> {
    let plan = config.plan()?;
    let kinds = parse_units(units).map_err(|e| format!("{template}: {e}"))?;
    config.check_units(&kinds, template)?;
    let mut g = Gen::new(plan.rows, plan.cols);
    let hexes = usize::from(plan.rows) * usize::from(plan.cols);
    let sea = (config.water.clamp(0.0, 0.95) * hexes as f64).round() as usize;
    land::land(&mut g, config, sea);
    relief::relief(&mut g, config);
    biomes::biomes(&mut g, config);
    cleanup::cleanup(&mut g, config);
    Ok(Generated { config: config.clone(), plan, state: g, units: units.to_string() })
}

impl Generated {
    /// `terrain.map`: the rows, the columns, a letter for each hex (a capital
    /// for a road).
    pub fn terrain_text(&self) -> String {
        let g = &self.state;
        let mut out = format!("{}\n{}\n", g.rows(), g.cols());
        for h in g.terrain.hexes() {
            let c = g.terrain.get(h).to_char();
            out.push(if *g.road.get(h) { c.to_ascii_uppercase() } else { c });
            if h.col + 1 == g.cols() {
                out.push('\n');
            }
        }
        out
    }

    /// `cities.txt`.
    pub fn cities_text(&self) -> String {
        "# The cities of a generated map (docs/map-format.md): none yet.\n".to_string()
    }

    /// `mapgen.txt`: the config, seed, size and cities and all, that makes
    /// this map again.
    pub fn config_text(&self) -> String {
        format!(
            "# mapgen's config for this map: `mapgen gen --config mapgen.txt` makes it again.\n{}",
            self.config.to_text(Some(&self.plan))
        )
    }

    /// The map as the game reads it.
    pub fn map(&self) -> Result<Map, String> {
        scenario::parse(&self.terrain_text(), Some(&self.units), Some(&self.cities_text()))
            .map(|(map, _)| map)
            .map_err(|e| e.to_string())
    }
}

/// A terrain file of the smallest map the reader takes (20 x 20), with a
/// drawing in its top left corner and `fill` around it: for tests.
#[cfg(test)]
pub(crate) fn padded(drawing: &[&str], fill: char) -> String {
    let rows: Vec<String> = (0..20)
        .map(|r| {
            let line = drawing.get(r).copied().unwrap_or("");
            format!("{line}{}", fill.to_string().repeat(20 - line.chars().count()))
        })
        .collect();
    format!("20\n20\n{}\n", rows.join("\n"))
}
