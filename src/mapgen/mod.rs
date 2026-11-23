//! The random map generator (`mapgen`): from a seed and a few knobs, a whole
//! map directory in the game's format (`docs/map-format.md`), with a report
//! of what it measures and pictures of it (`docs/mapgen.md`).
//!
//! It is pure: no files, no clock, no state of its own beyond what it is
//! given, so the same seed and knobs make the same map on every machine, and
//! the new game screen calls it for a new random map. Its stages each draw on
//! their own stream of chance ([`rng`]), so changing what comes later leaves
//! what came before as it was.
//!
//! The stages, in order: [`land`] (land and water), [`relief`] (mountains and
//! hills), [`rivers`] (and lakes), [`biomes`] (forest, swamp, desert,
//! grass), [`cleanup`], [`cities`], [`sites`] (temples), [`roads`].
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
pub mod validate;

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
    /// Travel goes by sea too (`archipelago`): a hex of sea costs little, a
    /// turn to embark (`cities::travel`).
    pub by_sea: bool,
    /// The main sea: the biggest water a ship sails on (`cities`). Travel by
    /// sea and the cities at sea keep to it, so that a port on the
    /// capitals' land opens on it.
    pub main_sea: Grid<bool>,
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
            by_sea: false,
            main_sea: Grid::new(rows, cols, false),
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
    /// What it fails of the hard checks (`validate::hard`): nothing, unless
    /// no try passed (then the map that failed least is given).
    pub failures: Vec<String>,
    /// Its soft score (`score::all`): lower is nearer what was asked.
    pub score: f64,
}

/// Make a map: `units` is the units template's text (`template` names it, for
/// what is said of it). A map that fails a hard check (`validate::hard`:
/// unfair, a city out of reach, ...) is made again from another seed (the
/// seed and the try, scrambled: not the next seed, which a batch makes too),
/// at most `attempts` times; `accept best N` makes N maps that pass and
/// keeps the one with the lowest soft score. The map's config says the seed
/// that made it (which makes it again at the first try), and how many tries
/// it took. If none passed, the one that failed least is given, and says
/// what it fails (`Generated::failures`).
pub fn generate(config: &Config, units: &str, template: &str) -> Result<Generated, String> {
    let plan = config.plan()?;
    let kinds = parse_units(units).map_err(|e| format!("{template}: {e}"))?;
    config.check_units(&kinds, template)?;
    let wanted = match config.accept {
        config::Accept::First => 1,
        config::Accept::Best(n) => n.max(1) as usize,
    };
    let mut passed: Vec<Generated> = Vec::new();
    let mut failed: Option<Generated> = None;
    for attempt in 0..config.attempts.max(1) {
        let mut cfg = config.clone();
        if attempt > 0 {
            cfg.seed = rng::mix(config.seed ^ rng::mix(u64::from(attempt)));
        }
        let mut made = make(&cfg, &plan, (&kinds, units))?;
        made.attempts = attempt + 1;
        made.failures = validate::hard(&made);
        made.score =
            made.map().map_or(f64::MAX, |map| score::score(&score::all(&made.config, &plan, &stats::Stats::of(&map))));
        if made.failures.is_empty() {
            passed.push(made);
            if passed.len() >= wanted {
                break;
            }
        } else if failed.as_ref().is_none_or(|f| made.failures.len() < f.failures.len()) {
            failed = Some(made);
        }
    }
    let tries = passed.last().map_or(config.attempts.max(1), |m| m.attempts);
    let best = passed.into_iter().min_by(|a, b| a.score.total_cmp(&b.score)).or(failed);
    let mut made = best.ok_or("no map was made")?;
    made.attempts = made.attempts.max(1).max(if wanted > 1 { tries } else { 0 });
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
    Ok(Generated {
        config: config.clone(),
        plan,
        state: g,
        units: units.to_string(),
        attempts: 1,
        failures: Vec::new(),
        score: 0.0,
    })
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

    /// Each hex's capital: the one that gets to it first (`cities::travel`),
    /// as the fairness measures count; by the capitals' order.
    pub fn cells(&self) -> Grid<Option<usize>> {
        let capitals: Vec<crate::state::Hex> =
            self.state.cities.iter().filter(|c| c.side != crate::state::Side::Grey).map(|c| c.hex).collect();
        cities::travel(&self.state, &capitals).1
    }

    /// Each side's share, a line each: its capital, the neutral cities in its
    /// cell and within 3 turns, its land, the way to its nearest enemy and
    /// to its nearest temple (turns), the ports, temples and ruins in its
    /// cell. (What the fairness measures compare, side by side.)
    pub fn sides_table(&self) -> String {
        use crate::state::{Side, TileType};
        let g = &self.state;
        let t = &g.terrain;
        let capitals: Vec<&cities::City> = g.cities.iter().filter(|c| c.side != Side::Grey).collect();
        let grids: Vec<Grid<Option<u32>>> = capitals.iter().map(|c| cities::travel(g, &[c.hex]).0).collect();
        let owner = |h: crate::state::Hex| {
            (0..grids.len()).filter_map(|i| grids[i].get(h).map(|c| (c, i))).min().map(|(_, i)| i)
        };
        let turns = |c: Option<u32>| c.map_or("-".to_string(), |c| format!("{:.1}", f64::from(c) / 10.0));
        let mut out = format!(
            "  {:<8} {:<14} {:>7} {:>9} {:>6} {:>8} {:>8} {:>6} {:>8} {:>6}\n",
            "side", "capital", "cities", "in 3 t.", "land", "enemy t.", "temple t.", "ships", "temples", "ruins"
        );
        for (i, cap) in capitals.iter().enumerate() {
            let neutral = g.cities.iter().filter(|c| c.side == Side::Grey);
            let mine = neutral.clone().filter(|c| owner(c.hex) == Some(i)).count();
            let near = neutral.filter(|c| grids[i].get(c.hex).is_some_and(|d| d <= 30)).count();
            let land = t
                .hexes()
                .filter(|h| !matches!(*t.get(*h), TileType::Water | TileType::Mountain) && owner(*h) == Some(i))
                .count();
            let enemy = capitals.iter().filter(|c| c.hex != cap.hex).filter_map(|c| *grids[i].get(c.hex)).min();
            let temple = t.hexes().filter(|h| *t.get(*h) == TileType::Temple).filter_map(|h| *grids[i].get(h)).min();
            let ships = |c: &&cities::City| {
                c.produce.iter().any(|(id, _)| {
                    self.kinds().iter().any(|k| k.id == *id && reach::movement_class(k) == reach::MoveClass::Naval)
                })
            };
            let ports = g.cities.iter().filter(|c| owner(c.hex) == Some(i)).filter(ships).count();
            let of = |k: TileType| t.hexes().filter(|h| *t.get(*h) == k && owner(*h) == Some(i)).count();
            out.push_str(&format!(
                "  {:<8} {:<14} {:>7} {:>9} {:>6} {:>8} {:>8} {:>6} {:>8} {:>6}\n",
                crate::state::scenario::side_name(cap.side),
                cap.name,
                mine,
                near,
                land,
                turns(enemy),
                turns(temple),
                ports,
                of(TileType::Temple),
                of(TileType::Ruin)
            ));
        }
        out
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
