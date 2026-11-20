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
//! hills), [`rivers`] (and lakes), [`biomes`] (forest, swamp, desert,
//! grass), [`cleanup`]. Cities, sites and roads are to come (the plan's
//! milestones G4 to G6).
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
pub mod cities;
pub mod cleanup;
pub mod config;
pub mod grid;
pub mod land;
pub mod names;
pub mod noise;
pub mod preview;
pub mod reach;
pub mod relief;
pub mod rivers;
pub mod rng;
pub mod roads;
pub mod score;
pub mod sites;
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
    /// Each river, from its source to its mouth (where bridges go across).
    pub rivers: Vec<Vec<crate::state::Hex>>,
    /// The rivers' and the lakes' water (the sea's rebalance leaves it be).
    pub fresh: Grid<bool>,
    /// The rivers' water alone (crossed by bridges, where lakes are not).
    pub river: Grid<bool>,
    /// The water ships sail to the sea on (the sea, and rivers into it).
    pub navigable: Grid<bool>,
    /// Hexes the last pass over the mix leaves as they are (the ways cut
    /// into walled-off land: `cleanup::join_land`).
    pub keep: Grid<bool>,
    /// The cities (`cities`), and how fair their sharing out came to be.
    pub cities: Vec<cities::City>,
    pub fairness: Vec<cities::Fair>,
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
            rivers: Vec::new(),
            fresh: Grid::new(rows, cols, false),
            river: Grid::new(rows, cols, false),
            navigable: Grid::new(rows, cols, false),
            keep: Grid::new(rows, cols, false),
            cities: Vec::new(),
            fairness: Vec::new(),
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
    /// How many seeds were tried before this map (its config has its own).
    pub attempts: u32,
}

/// Make a map: `units` is the units template's text (`template` names it, for
/// what is said of it). A map whose cities will not share out fairly is
/// made again from another seed (the seed and the try, scrambled: not the
/// next seed, which a batch makes too), at most `attempts` times; the map's
/// config says the seed that made it (which makes it again at the first
/// try), and how many tries it took. If none was fair, the fairest is given,
/// and says what is unfair.
pub fn generate(config: &Config, units: &str, template: &str) -> Result<Generated, String> {
    let plan = config.plan()?;
    let kinds = parse_units(units).map_err(|e| format!("{template}: {e}"))?;
    config.check_units(&kinds, template)?;
    let mut best: Option<(usize, Generated)> = None;
    for attempt in 0..config.attempts.max(1) {
        let mut cfg = config.clone();
        if attempt > 0 {
            cfg.seed = rng::mix(config.seed ^ rng::mix(u64::from(attempt)));
        }
        let mut made = make(&cfg, &plan, (&kinds, units))?;
        made.attempts = attempt + 1;
        let unfair = made.state.fairness.iter().filter(|f| !f.ok()).count();
        if best.as_ref().is_none_or(|(u, _)| unfair < *u) {
            best = Some((unfair, made));
        }
        if unfair == 0 {
            break;
        }
    }
    let (_, mut made) = best.ok_or("no map was made")?;
    made.attempts = made.attempts.max(1);
    Ok(made)
}

/// One map, from `cfg`'s seed.
fn make(config: &Config, plan: &Plan, (kinds, units): (&[crate::state::UnitKind], &str)) -> Result<Generated, String> {
    let plan = plan.clone();
    let hexes = usize::from(plan.rows) * usize::from(plan.cols);
    let water = (config.water.clamp(0.0, 0.95) * hexes as f64).round() as usize;
    // The rivers and lakes take water of their own: the sea is made smaller
    // by what they took, and the land made again (at most three times),
    // until the water is near its share; the sea's shore then makes it so.
    let mut sea = water;
    let mut g = Gen::new(plan.rows, plan.cols);
    for _ in 0..3 {
        g = Gen::new(plan.rows, plan.cols);
        land::land(&mut g, config, sea);
        relief::relief(&mut g, config, hexes - water);
        rivers::rivers(&mut g, config);
        let have = g.terrain.cells().iter().filter(|t| **t == TileType::Water).count();
        if have.abs_diff(water) * 100 <= hexes {
            break;
        }
        sea = (sea + water).saturating_sub(have);
    }
    land::rebalance_sea(&mut g, water);
    biomes::biomes(&mut g, config);
    cleanup::cleanup(&mut g, config);
    cleanup::join_land(&mut g);
    cities::cities(&mut g, config, &plan, kinds)?;
    sites::sites(&mut g, config, &plan);
    roads::roads(&mut g, config, &plan);
    // The mix again, round the room made for the cities.
    cleanup::quotas(&mut g, config);
    sites::judge(&mut g, config);
    cities::worth(&mut g, config, kinds);
    Ok(Generated { config: config.clone(), plan, state: g, units: units.to_string(), attempts: 1 })
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
        let mut out = "# The cities of a generated map (docs/map-format.md).\n".to_string();
        for c in &self.state.cities {
            out.push_str(&format!("\ncity {} {}\n  name     {}\n", c.hex.row, c.hex.col, c.name));
            if c.side != crate::state::Side::Grey {
                out.push_str(&format!("  side     {}\n", crate::state::scenario::side_name(c.side)));
            }
            out.push_str(&format!("  walls    {}\n  income   {}\n", c.walls, c.income));
            match c.trains {
                2 => out.push_str("  trains   veteran\n"),
                3 => out.push_str("  trains   elite\n"),
                _ => {}
            }
            for (id, turns) in &c.produce {
                out.push_str(&format!("  produce  {id} {turns}\n"));
            }
            for (id, n) in &c.garrison {
                out.push_str(&format!("  garrison {id} {n}\n"));
            }
        }
        out
    }

    /// What the preview marks on each city: at sea, in the air, a port.
    pub fn city_marks(&self) -> Vec<preview::CityMark> {
        let g = &self.state;
        self.state
            .cities
            .iter()
            .map(|c| preview::CityMark {
                sea: c.reach == reach::ReachClass::Sea,
                air: c.reach == reach::ReachClass::Air,
                port: c.produce.iter().any(|(id, _)| {
                    self.kinds().iter().any(|k| k.id == *id && reach::movement_class(k) == reach::MoveClass::Naval)
                }) && g.terrain.neighbours(c.hex).any(|n| *g.terrain.get(n) == TileType::Water),
            })
            .collect()
    }

    /// The units template's kinds.
    pub fn kinds(&self) -> Vec<crate::state::UnitKind> {
        parse_units(&self.units).unwrap_or_default()
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
