//! The generator's knobs (`docs/mapgen.md`): read from `key value` lines (a
//! preset, a config file, the command line), written back whole, seed and
//! all, so that any map can be made again (`mapgen.txt`); and the size, the
//! count of cities and their density worked out from the two of them given.

use crate::state::scenario::{side_name, side_named};
use crate::state::{MAX_COLS, MAX_ROWS, MIN_COLS, MIN_ROWS, Side, UnitKind};

use super::reach::{MoveClass, movement_class};

/// The shape of the land.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Layout {
    /// The land fills the map; the sea comes in from some edges.
    Coast,
    /// One land mass, the sea on every edge.
    Continent,
    /// Hardly any sea: rivers and lakes.
    Inland,
    /// Two land masses joined by land bridges.
    Twin,
    /// Islands, big and small.
    Archipelago,
}

/// An edge of the map.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Edge {
    North,
    South,
    East,
    West,
}

/// How mountains lie: in long ranges, or in round lumps.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum RangeStyle {
    Ridges,
    Massifs,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Roads {
    None,
    Sparse,
    Linked,
    Dense,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Fairness {
    Off,
    Loose,
    Normal,
    Strict,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Symmetry {
    None,
    Mirror,
    Rotate,
}

/// The most temples for each side the knob takes.
pub const MAX_TEMPLES_PER_SIDE: f64 = 4.0;

/// The choices of temples for each side offered on the new game screen:
/// none, one shared by two neighbours, one each, ... four each.
pub const TEMPLE_CHOICES: [f64; 6] = [0.0, 0.5, 1.0, 2.0, 3.0, 4.0];

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Accept {
    First,
    Best(u32),
}

/// How many of something: a count, or a rate (of cities, or of hexes, as the
/// knob says).
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Amount {
    Count(u32),
    Rate(f64),
}

impl Amount {
    /// How many, of `base` (cities or hexes).
    pub fn of(self, base: f64) -> u32 {
        match self {
            Amount::Count(n) => n,
            Amount::Rate(r) => (r * base).round().max(0.0) as u32,
        }
    }
}

/// The size of the map: a name from the table, or rows and columns.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Size {
    Named(&'static str),
    Exact(u16, u16),
}

/// The named sizes, and their hexes (square, at normal density, they hold
/// 8, 16, 32, 48 and 80 cities).
pub const SIZES: [(&str, u32); 5] =
    [("tiny", 38 * 44), ("small", 54 * 62), ("medium", 77 * 88), ("large", 94 * 108), ("huge", 121 * 140)];

/// The densities with names: land hexes for each city.
pub const DENSITIES: [(&str, f64); 3] = [("sparse", 230.0), ("normal", 170.0), ("dense", 120.0)];

/// The aspects with names: rows for each column (square looks square on
/// the screen, the hexes being wider than they are tall).
pub const ASPECTS: [(&str, f64); 3] = [("square", 0.87), ("portrait", 1.24), ("landscape", 0.6)];

/// The sides, in the order the generator gives them out.
pub const COLOURS: [Side; 8] =
    [Side::Black, Side::White, Side::Red, Side::Green, Side::Blue, Side::Orange, Side::Yellow, Side::Purple];

/// The land terrains of `mix`, in its order.
pub const MIX: [&str; 6] = ["grass", "mountain", "hill", "forest", "swamp", "desert"];

/// Every knob.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub seed: u64,
    pub sides: u8,
    pub colours: Vec<Side>,
    pub cities: Option<u32>,
    pub cities_per_side: u32,
    pub density: Option<f64>,
    pub size: Option<Size>,
    pub aspect: f64,
    pub water: f64,
    pub layout: Layout,
    pub sea_edges: Vec<Edge>,
    /// Per hex.
    pub lakes: Amount,
    pub min_island: u32,
    /// Shares of the land, in percent, in the order of `MIX`.
    pub mix: [f64; 6],
    pub feature_scale: f64,
    pub roughness: f64,
    pub range_style: RangeStyle,
    pub hill_ring: f64,
    /// Per hex.
    pub rivers: Amount,
    pub river_width: (u32, u32),
    pub city_min_gap: Option<u32>,
    pub city_gap_target: Option<f64>,
    pub coastal_cities: f64,
    pub edge_margin: u32,
    /// The walls of the neutral cities: each level and its share, in percent.
    pub walls: Vec<(u32, f64)>,
    /// Per city.
    /// Temples for each side (0 to 4: none, shared between neighbours,
    /// one each, ...); or, if `temples` is given, that many (a count, or a
    /// rate of the cities).
    pub temples_per_side: f64,
    pub temples: Option<Amount>,
    pub ruins: Amount,
    pub site_city_gap: u32,
    pub site_gap: u32,
    pub roads: Roads,
    pub road_loops: f64,
    pub bridges: bool,
    pub max_bridge: u32,
    /// Per city.
    pub passes: Amount,
    pub sea_cities: bool,
    /// Per city.
    pub sea_city_count: Amount,
    pub fly_cities: bool,
    pub fly_city_count: Option<u32>,
    pub ports: bool,
    pub ports_per_side: u32,
    pub fairness: Fairness,
    pub symmetry: Symmetry,
    pub attempts: u32,
    pub accept: Accept,
}

/// The presets, as the files they are: `assets/mapgen/presets/<name>.txt`.
pub const PRESETS: [(&str, &str); 5] = [
    ("classic", include_str!("../../assets/mapgen/presets/classic.txt")),
    ("varied", include_str!("../../assets/mapgen/presets/varied.txt")),
    ("open", include_str!("../../assets/mapgen/presets/open.txt")),
    ("rugged", include_str!("../../assets/mapgen/presets/rugged.txt")),
    ("islands", include_str!("../../assets/mapgen/presets/islands.txt")),
];

impl Default for Config {
    /// The `classic` preset: the target map's numbers (`docs/mapgen.md`).
    fn default() -> Config {
        Config {
            seed: 1,
            sides: 4,
            colours: COLOURS[..4].to_vec(),
            cities: None,
            cities_per_side: 10,
            density: None,
            size: None,
            aspect: 0.87,
            water: 0.19,
            layout: Layout::Coast,
            sea_edges: vec![Edge::West, Edge::North],
            lakes: Amount::Rate(1.0 / 1500.0),
            min_island: 30,
            mix: [53.5, 13.0, 15.5, 14.0, 3.0, 0.0],
            feature_scale: 1.0,
            roughness: 0.5,
            range_style: RangeStyle::Ridges,
            hill_ring: 0.85,
            rivers: Amount::Rate(1.0 / 2500.0),
            river_width: (2, 3),
            city_min_gap: None,
            city_gap_target: None,
            coastal_cities: 0.25,
            edge_margin: 2,
            walls: vec![(3, 8.0), (4, 58.0), (5, 18.0), (6, 16.0)],
            temples_per_side: 1.0,
            temples: None,
            ruins: Amount::Rate(0.24),
            site_city_gap: 3,
            site_gap: 5,
            roads: Roads::Linked,
            road_loops: 0.15,
            bridges: true,
            max_bridge: 5,
            passes: Amount::Rate(0.1),
            sea_cities: false,
            sea_city_count: Amount::Rate(0.04),
            fly_cities: false,
            fly_city_count: None,
            ports: true,
            ports_per_side: 1,
            fairness: Fairness::Normal,
            symmetry: Symmetry::None,
            attempts: 30,
            accept: Accept::First,
        }
    }
}

/// What the size, the cities and their density come to.
#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub rows: u16,
    pub cols: u16,
    pub cities: u32,
    /// Land hexes for each city.
    pub density: f64,
    pub warnings: Vec<String>,
}

fn number(key: &str, value: &str) -> Result<f64, String> {
    value.parse::<f64>().ok().filter(|v| v.is_finite()).ok_or(format!("{key}: \"{value}\" is not a number"))
}

fn whole(key: &str, value: &str) -> Result<u32, String> {
    value.parse::<u32>().map_err(|_| format!("{key}: \"{value}\" is not a whole number"))
}

fn on_off(key: &str, value: &str) -> Result<bool, String> {
    match value {
        "on" | "true" | "yes" => Ok(true),
        "off" | "false" | "no" => Ok(false),
        _ => Err(format!("{key}: \"{value}\" is not on or off")),
    }
}

/// A count ("21"), a rate ("0.26"), or a rate of hexes ("1/1500").
fn amount(key: &str, value: &str) -> Result<Amount, String> {
    if let Some((a, b)) = value.split_once('/') {
        let (a, b) = (number(key, a.trim())?, number(key, b.trim())?);
        if b <= 0.0 {
            return Err(format!("{key}: \"{value}\": divided by nothing"));
        }
        return Ok(Amount::Rate(a / b));
    }
    if value.contains('.') { Ok(Amount::Rate(number(key, value)?)) } else { Ok(Amount::Count(whole(key, value)?)) }
}

fn amount_text(a: Amount, per_hex: bool) -> String {
    match a {
        Amount::Count(n) => n.to_string(),
        Amount::Rate(r) if per_hex && r > 0.0 => format!("1/{}", trim(1.0 / r)),
        Amount::Rate(r) => trim_rate(r),
    }
}

/// A number as short as it can be written.
fn trim(x: f64) -> String {
    let s = format!("{x:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    s.to_string()
}

/// A rate, always with a point, so it reads back as a rate.
fn trim_rate(x: f64) -> String {
    let s = trim(x);
    if s.contains('.') { s } else { format!("{s}.0") }
}

fn named<T: Copy>(key: &str, value: &str, names: &[(&str, T)]) -> Result<T, String> {
    names.iter().find(|(n, _)| *n == value).map(|(_, t)| *t).ok_or_else(|| {
        let all: Vec<&str> = names.iter().map(|(n, _)| *n).collect();
        format!("{key}: \"{value}\" is not one of {}", all.join(", "))
    })
}

fn name_of<T: PartialEq + Copy>(names: &[(&'static str, T)], t: T) -> &'static str {
    names.iter().find(|(_, x)| *x == t).map_or("?", |(n, _)| n)
}

const LAYOUTS: [(&str, Layout); 5] = [
    ("coast", Layout::Coast),
    ("continent", Layout::Continent),
    ("inland", Layout::Inland),
    ("twin", Layout::Twin),
    ("archipelago", Layout::Archipelago),
];
const EDGES: [(&str, Edge); 4] =
    [("north", Edge::North), ("south", Edge::South), ("east", Edge::East), ("west", Edge::West)];
const STYLES: [(&str, RangeStyle); 2] = [("ridges", RangeStyle::Ridges), ("massifs", RangeStyle::Massifs)];
const ROADS: [(&str, Roads); 4] =
    [("none", Roads::None), ("sparse", Roads::Sparse), ("linked", Roads::Linked), ("dense", Roads::Dense)];
const FAIRNESS: [(&str, Fairness); 4] =
    [("off", Fairness::Off), ("loose", Fairness::Loose), ("normal", Fairness::Normal), ("strict", Fairness::Strict)];
const SYMMETRY: [(&str, Symmetry); 3] =
    [("none", Symmetry::None), ("mirror", Symmetry::Mirror), ("rotate", Symmetry::Rotate)];

impl Config {
    /// A preset by name.
    pub fn preset(name: &str) -> Result<Config, String> {
        let text = PRESETS.iter().find(|(n, _)| *n == name).map(|(_, t)| *t).ok_or_else(|| {
            let names: Vec<&str> = PRESETS.iter().map(|(n, _)| *n).collect();
            format!("there is no preset \"{name}\" ({})", names.join(", "))
        })?;
        Config::default().with_text(text)
    }

    /// This config with what a text of `key value` lines says on top of it
    /// (`#` starts a comment).
    pub fn with_text(mut self, text: &str) -> Result<Config, String> {
        for (i, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let (key, value) = line.split_once(char::is_whitespace).map_or((line, ""), |(k, v)| (k, v.trim()));
            self.set(key, value).map_err(|e| format!("line {}: {e}", i + 1))?;
        }
        Ok(self)
    }

    /// Set a knob: its key (`-` or `_` between words) and its value.
    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        let key = key.replace('-', "_");
        let k = key.as_str();
        match k {
            "seed" => self.seed = value.parse().map_err(|_| format!("seed: \"{value}\" is not a whole number"))?,
            "sides" => {
                let n = whole(k, value)?;
                if !(2..=8).contains(&n) {
                    return Err(format!("sides: {n}: from 2 to 8"));
                }
                self.sides = n as u8;
                if self.colours.len() != usize::from(self.sides) {
                    self.colours = COLOURS[..usize::from(self.sides)].to_vec();
                }
            }
            "side_colours" | "side_colors" => {
                let sides: Vec<Side> = value
                    .split(',')
                    .map(|w| side_named(w.trim()).filter(|s| *s != Side::Grey).ok_or(format!("{k}: no side \"{w}\"")))
                    .collect::<Result<_, _>>()?;
                if sides.len() < 2 || sides.len() > 8 {
                    return Err(format!("{k}: from 2 to 8 sides"));
                }
                self.sides = sides.len() as u8;
                self.colours = sides;
            }
            "cities" => self.cities = Some(whole(k, value)?),
            "cities_per_side" => self.cities_per_side = whole(k, value)?,
            "density" => {
                self.density = Some(if let Ok(n) = value.parse::<f64>() { n } else { named(k, value, &DENSITIES)? });
            }
            "size" => {
                self.size = Some(if let Some((r, c)) = value.split_once('x') {
                    let (r, c) = (whole(k, r)?, whole(k, c)?);
                    Size::Exact(r.min(u32::from(u16::MAX)) as u16, c.min(u32::from(u16::MAX)) as u16)
                } else {
                    let names: Vec<(&str, &str)> = SIZES.iter().map(|(n, _)| (*n, *n)).collect();
                    Size::Named(named(k, value, &names)?)
                });
            }
            "aspect" => self.aspect = if let Ok(n) = value.parse::<f64>() { n } else { named(k, value, &ASPECTS)? },
            "water" => self.water = number(k, value)?,
            "layout" => self.layout = named(k, value, &LAYOUTS)?,
            "sea_edges" => {
                self.sea_edges = value.split(',').map(|w| named(k, w.trim(), &EDGES)).collect::<Result<_, _>>()?;
            }
            "lakes" => self.lakes = amount(k, value)?,
            "min_island" => self.min_island = whole(k, value)?,
            "mix" => {
                let words: Vec<&str> = value.split_whitespace().collect();
                if words.len() % 2 != 0 {
                    return Err("mix: terrain and share, in pairs".to_string());
                }
                let mut mix = [0.0; 6];
                for pair in words.chunks(2) {
                    let i = MIX.iter().position(|m| *m == pair[0]).ok_or(format!("mix: no terrain \"{}\"", pair[0]))?;
                    mix[i] = number(k, pair[1])?;
                }
                self.mix = mix;
            }
            "feature_scale" => self.feature_scale = number(k, value)?,
            "roughness" => self.roughness = number(k, value)?.clamp(0.0, 1.0),
            "range_style" => self.range_style = named(k, value, &STYLES)?,
            "hill_ring" => self.hill_ring = number(k, value)?,
            "rivers" => self.rivers = amount(k, value)?,
            "river_width" => {
                let (a, b) = value.split_once("..").unwrap_or((value, value));
                self.river_width = (whole(k, a)?, whole(k, b)?);
            }
            "city_min_gap" => self.city_min_gap = Some(whole(k, value)?),
            "city_gap_target" => self.city_gap_target = Some(number(k, value)?),
            "coastal_cities" => self.coastal_cities = number(k, value)?,
            "edge_margin" => self.edge_margin = whole(k, value)?,
            "walls" => {
                self.walls = value
                    .split_whitespace()
                    .map(|w| {
                        let (level, share) = w.split_once(':').ok_or(format!("walls: \"{w}\" is not level:share"))?;
                        let level = whole(k, level)?;
                        // (As high as walls go in the game: `state::WALLS_CAP`.)
                        if level > crate::state::WALLS_CAP {
                            return Err(format!(
                                "walls: {level} is above {}, as high as walls go",
                                crate::state::WALLS_CAP
                            ));
                        }
                        Ok((level, number(k, share)?))
                    })
                    .collect::<Result<_, String>>()?;
            }
            "temples" => self.temples = Some(amount(k, value)?),
            "temples_per_side" => {
                let n = number(k, value)?;
                if !(0.0..=MAX_TEMPLES_PER_SIDE).contains(&n) {
                    return Err(format!("{k}: {value} is not 0 to {MAX_TEMPLES_PER_SIDE}"));
                }
                self.temples_per_side = n;
            }
            "ruins" => self.ruins = amount(k, value)?,
            "site_city_gap" => self.site_city_gap = whole(k, value)?,
            "site_gap" => self.site_gap = whole(k, value)?,
            "roads" => self.roads = named(k, value, &ROADS)?,
            "road_loops" => self.road_loops = number(k, value)?,
            "bridges" => self.bridges = on_off(k, value)?,
            "max_bridge" => self.max_bridge = whole(k, value)?,
            "passes" => self.passes = amount(k, value)?,
            "sea_cities" => self.sea_cities = on_off(k, if value.is_empty() { "on" } else { value })?,
            "sea_city_count" => self.sea_city_count = amount(k, value)?,
            "fly_cities" => self.fly_cities = on_off(k, if value.is_empty() { "on" } else { value })?,
            "fly_city_count" => self.fly_city_count = Some(whole(k, value)?),
            "ports" => {
                self.ports = match value {
                    "auto" | "on" => true,
                    "off" => false,
                    _ => return Err(format!("ports: \"{value}\" is not auto or off")),
                }
            }
            "ports_per_side" => self.ports_per_side = whole(k, value)?,
            "fairness" => self.fairness = named(k, value, &FAIRNESS)?,
            "symmetry" => self.symmetry = named(k, value, &SYMMETRY)?,
            "attempts" => self.attempts = whole(k, value)?.max(1),
            "accept" => {
                self.accept = match value.split_whitespace().collect::<Vec<_>>()[..] {
                    ["first"] => Accept::First,
                    ["best", n] => Accept::Best(whole(k, n)?.max(1)),
                    _ => return Err(format!("accept: \"{value}\" is not first or best N")),
                };
            }
            _ => return Err(format!("there is no knob \"{key}\"")),
        }
        Ok(())
    }

    /// Every knob, a line each, as `with_text` reads them: the size and the
    /// cities as `plan` says, so that it makes the same map again.
    pub fn to_text(&self, plan: Option<&Plan>) -> String {
        let mut out = String::new();
        let mut line = |k: &str, v: String| out.push_str(&format!("{k:<16} {v}\n"));
        line("seed", self.seed.to_string());
        line("side_colours", self.colours.iter().map(|s| side_name(*s)).collect::<Vec<_>>().join(","));
        match plan {
            Some(p) => {
                line("size", format!("{}x{}", p.rows, p.cols));
                line("cities", p.cities.to_string());
            }
            None => {
                match self.size {
                    Some(Size::Named(n)) => line("size", n.to_string()),
                    Some(Size::Exact(r, c)) => line("size", format!("{r}x{c}")),
                    None => {}
                }
                if let Some(c) = self.cities {
                    line("cities", c.to_string());
                }
                if let Some(d) = self.density {
                    line("density", trim(d));
                }
            }
        }
        line("cities_per_side", self.cities_per_side.to_string());
        line("aspect", trim(self.aspect));
        line("water", trim(self.water));
        line("layout", name_of(&LAYOUTS, self.layout).to_string());
        line("sea_edges", self.sea_edges.iter().map(|e| name_of(&EDGES, *e)).collect::<Vec<_>>().join(","));
        line("lakes", amount_text(self.lakes, true));
        line("min_island", self.min_island.to_string());
        line("mix", MIX.iter().zip(self.mix).map(|(m, s)| format!("{m} {}", trim(s))).collect::<Vec<_>>().join(" "));
        line("feature_scale", trim(self.feature_scale));
        line("roughness", trim(self.roughness));
        line("range_style", name_of(&STYLES, self.range_style).to_string());
        line("hill_ring", trim(self.hill_ring));
        line("rivers", amount_text(self.rivers, true));
        line("river_width", format!("{}..{}", self.river_width.0, self.river_width.1));
        if let Some(g) = self.city_min_gap {
            line("city_min_gap", g.to_string());
        }
        if let Some(g) = self.city_gap_target {
            line("city_gap_target", trim(g));
        }
        line("coastal_cities", trim(self.coastal_cities));
        line("edge_margin", self.edge_margin.to_string());
        line("walls", self.walls.iter().map(|(w, s)| format!("{w}:{}", trim(*s))).collect::<Vec<_>>().join(" "));
        line("temples_per_side", trim(self.temples_per_side));
        if let Some(t) = self.temples {
            line("temples", amount_text(t, false));
        }
        line("ruins", amount_text(self.ruins, false));
        line("site_city_gap", self.site_city_gap.to_string());
        line("site_gap", self.site_gap.to_string());
        line("roads", name_of(&ROADS, self.roads).to_string());
        line("road_loops", trim(self.road_loops));
        line("bridges", if self.bridges { "on" } else { "off" }.to_string());
        line("max_bridge", self.max_bridge.to_string());
        line("passes", amount_text(self.passes, false));
        line("sea_cities", if self.sea_cities { "on" } else { "off" }.to_string());
        line("sea_city_count", amount_text(self.sea_city_count, false));
        line("fly_cities", if self.fly_cities { "on" } else { "off" }.to_string());
        if let Some(n) = self.fly_city_count {
            line("fly_city_count", n.to_string());
        }
        line("ports", if self.ports { "auto" } else { "off" }.to_string());
        line("ports_per_side", self.ports_per_side.to_string());
        line("fairness", name_of(&FAIRNESS, self.fairness).to_string());
        line("symmetry", name_of(&SYMMETRY, self.symmetry).to_string());
        line("attempts", self.attempts.to_string());
        line(
            "accept",
            match self.accept {
                Accept::First => "first".to_string(),
                Accept::Best(n) => format!("best {n}"),
            },
        );
        out
    }

    /// The size, the cities and their density: of the three, the two given
    /// say the third (one left out takes its default). Warns of a density
    /// far from the usual, and of fewer than 3 cities a side.
    pub fn plan(&self) -> Result<Plan, String> {
        let land_share = 1.0 - self.water.clamp(0.0, 0.95);
        let default_density = 170.0;
        let shape = |hexes: f64| {
            let cols = (hexes / self.aspect.max(0.1)).sqrt().round().max(1.0);
            let rows = (hexes / cols).round();
            (rows as u16, cols as u16)
        };
        let given_size = self.size.map(|s| match s {
            Size::Exact(r, c) => (r, c),
            Size::Named(n) => shape(f64::from(SIZES.iter().find(|(m, _)| *m == n).map_or(0, |(_, h)| *h))),
        });
        let (rows, cols, cities, density) = match (given_size, self.cities, self.density) {
            (Some((r, c)), Some(cities), _) => {
                let land = f64::from(r) * f64::from(c) * land_share;
                (r, c, cities, land / f64::from(cities.max(1)))
            }
            (Some((r, c)), None, density) => {
                let density = density.unwrap_or(default_density);
                let land = f64::from(r) * f64::from(c) * land_share;
                (r, c, (land / density).round().max(1.0) as u32, density)
            }
            (None, cities, density) => {
                let cities = cities.unwrap_or(u32::from(self.sides) * self.cities_per_side);
                let density = density.unwrap_or(default_density);
                let (r, c) = shape(f64::from(cities) * density / land_share);
                (r, c, cities, density)
            }
        };
        if !(MIN_ROWS..=MAX_ROWS).contains(&rows) || !(MIN_COLS..=MAX_COLS).contains(&cols) {
            return Err(format!(
                "the map would be {rows} x {cols}: rows {MIN_ROWS} to {MAX_ROWS}, columns {MIN_COLS} to {MAX_COLS}"
            ));
        }
        let mut warnings = Vec::new();
        if !(100.0..=300.0).contains(&density) {
            warnings.push(format!("{:.0} land hexes for each city is far from the usual (100 to 300)", density));
        }
        if cities < 3 * u32::from(self.sides) {
            warnings.push(format!("{cities} cities for {} sides is fewer than 3 a side", self.sides));
        }
        if cities < u32::from(self.sides) {
            return Err(format!("{cities} cities cannot hold {} capitals", self.sides));
        }
        Ok(Plan { rows, cols, cities, density, warnings })
    }

    /// The checkboxes ask for kinds of unit: is each in the units template?
    pub fn check_units(&self, kinds: &[UnitKind], template: &str) -> Result<(), String> {
        let has = |class: MoveClass| kinds.iter().any(|k| movement_class(k) == class);
        if self.sea_cities && !has(MoveClass::Naval) {
            return Err(format!("sea_cities is on, and the units template ({template}) has no ship"));
        }
        if self.fly_cities && !has(MoveClass::Flyer) {
            return Err(format!("fly_cities is on, and the units template ({template}) has no flyer"));
        }
        Ok(())
    }

    /// The least hex distance between cities (unless it is set: 0.38 of the
    /// square root of the density, and at least 4).
    pub fn min_gap(&self, plan: &Plan) -> u32 {
        self.city_min_gap.unwrap_or_else(|| ((0.38 * plan.density.sqrt()).round() as u32).max(4))
    }

    /// The middle distance between neighbouring cities aimed at.
    pub fn gap_target(&self, plan: &Plan) -> f64 {
        self.city_gap_target.unwrap_or(0.6 * plan.density.sqrt())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::scenario::parse_units;

    #[test]
    fn the_named_sizes_are_the_tables() {
        let mut c = Config::default();
        for (name, rows, cols, cities) in [
            ("tiny", 38, 44, 8),
            ("small", 54, 62, 16),
            ("medium", 77, 88, 32),
            ("large", 94, 108, 48),
            ("huge", 121, 140, 80),
        ] {
            c.set("size", name).unwrap();
            let p = c.plan().unwrap();
            assert_eq!((p.rows, p.cols), (rows, cols), "{name}");
            assert!(p.cities.abs_diff(cities) <= 1, "{name}: {} cities", p.cities);
        }
    }

    #[test]
    fn two_of_size_cities_and_density_say_the_third() {
        // Cities and density: one city needs about 210 hexes.
        let c = Config::default();
        let p = c.plan().unwrap();
        assert_eq!(p.cities, 40, "4 sides of 10");
        let per_city = f64::from(p.rows) * f64::from(p.cols) / f64::from(p.cities);
        assert!((per_city - 210.0).abs() < 5.0, "{per_city}");
        // Size and cities: the density.
        let mut c = Config::default();
        c.set("size", "100x100").unwrap();
        c.set("cities", "50").unwrap();
        assert!((c.plan().unwrap().density - 162.0).abs() < 1e-9);
        // Size and density: the cities.
        let mut c = Config::default();
        c.set("size", "100x100").unwrap();
        c.set("density", "dense").unwrap();
        assert_eq!(c.plan().unwrap().cities, 68);
        // Warnings, and what cannot be.
        c.set("density", "60").unwrap();
        assert!(c.plan().unwrap().warnings[0].contains("far from the usual"));
        c.set("size", "10x10").unwrap();
        assert!(c.plan().is_err());
    }

    #[test]
    fn a_config_written_reads_back_the_same() {
        let mut c = Config::preset("varied").unwrap();
        c.set("seed", "123456789").unwrap();
        c.set("lakes", "3").unwrap();
        c.set("accept", "best 4").unwrap();
        c.set("sea-cities", "on").unwrap();
        let again = Config::default().with_text(&c.to_text(None)).unwrap();
        assert_eq!(again, c);
        // Resolved, it says the size and the cities, and plans the same.
        let plan = c.plan().unwrap();
        let resolved = Config::default().with_text(&c.to_text(Some(&plan))).unwrap();
        assert_eq!((resolved.plan().unwrap().rows, resolved.plan().unwrap().cols), (plan.rows, plan.cols));
        assert_eq!(resolved.plan().unwrap().cities, plan.cities);
        assert!(Config::default().with_text("nonsense 3\n").unwrap_err().contains("no knob"));
    }

    #[test]
    fn the_classic_preset_is_the_default() {
        assert_eq!(Config::preset("classic").unwrap(), Config::default());
        for (name, _) in PRESETS {
            assert!(Config::preset(name).is_ok(), "{name}");
        }
        assert!(Config::preset("islands").unwrap().sea_cities);
    }

    #[test]
    fn a_checkbox_needs_its_kind_of_unit() {
        let foot = parse_units("unit a\nmoves 10\nenter grass 2\n").unwrap();
        let mut c = Config::default();
        assert!(c.check_units(&foot, "t.txt").is_ok());
        c.sea_cities = true;
        assert_eq!(
            c.check_units(&foot, "t.txt").unwrap_err(),
            "sea_cities is on, and the units template (t.txt) has no ship"
        );
        c.sea_cities = false;
        c.fly_cities = true;
        assert!(c.check_units(&foot, "t.txt").unwrap_err().contains("fly_cities is on"));
        let template = parse_units(include_str!("../../assets/mapgen/units.txt")).unwrap();
        c.sea_cities = true;
        assert!(c.check_units(&template, "units.txt").is_ok(), "the default template has both");
    }
}
