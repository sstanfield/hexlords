//! A whole map from its files: the terrain and roads, the kinds of unit, the
//! cities with what they build and who stands in them. `docs/map-format.md` is
//! the format's own page; this is its reader.
//!
//! A map is a file (`name.map`: terrain and roads, nothing else) or a
//! directory:
//!
//! ```text
//! name/
//!   terrain.map    the terrain and the roads (the single file's format)
//!   units.txt      the kinds of unit this map has
//!   cities.txt     its cities
//!   assets/        pictures to use instead of the game's (optional)
//! ```
//!
//! `units.txt` and `cities.txt` are lists of records. A record starts with a
//! line `unit <id>` or `city <row> <column>`; the lines after it, up to the
//! next record, are `key value`. Empty lines and lines that start with `#`
//! are skipped, and how a line is indented does not matter.
//!
//! Not a port: the C++ had no such files.

use std::fmt;
use std::path::{Path, PathBuf};

use super::cities::UnitProd;
use super::hex::Hex;
use super::map::{MAX_STACK, Map, MapError};
use super::terrain::{MoveCost, TileType};
use super::units::{DEFAULT_UNIT_IMAGE, Side, UnitKind};

/// The files of a map directory.
pub const TERRAIN_FILE: &str = "terrain.map";
pub const UNITS_FILE: &str = "units.txt";
pub const CITIES_FILE: &str = "cities.txt";
/// The directory, in a map directory, of pictures that replace the game's.
pub const ASSETS_DIR: &str = "assets";

/// A map, read.
#[derive(Clone, Debug)]
pub struct Scenario {
    /// The terrain, the kinds of unit, the cities and their garrisons.
    pub map: Map,
    /// The map's own asset directory, if it has one: a picture found there is
    /// used instead of the game's of the same path.
    pub assets: Option<PathBuf>,
    /// What was read but is probably not meant: a city tile nobody named.
    pub warnings: Vec<String>,
}

/// Why a map could not be read: which file, which line (0: the file as a
/// whole), and what is wrong there.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScenarioError {
    pub file: String,
    pub line: usize,
    pub message: String,
}

impl fmt::Display for ScenarioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            0 => write!(f, "{}: {}", self.file, self.message),
            line => write!(f, "{} line {line}: {}", self.file, self.message),
        }
    }
}

impl std::error::Error for ScenarioError {}

fn error<T>(file: &str, line: usize, message: impl Into<String>) -> Result<T, ScenarioError> {
    Err(ScenarioError { file: file.to_string(), line, message: message.into() })
}

/// A record of `units.txt` or `cities.txt`: its first line's words after the
/// keyword, and its `key value` lines, each with its line number.
struct Record<'a> {
    line: usize,
    head: Vec<&'a str>,
    fields: Vec<(usize, &'a str, &'a str)>,
}

/// Split a file into the records that start with `keyword`.
fn records<'a>(file: &str, text: &'a str, keyword: &str) -> Result<Vec<Record<'a>>, ScenarioError> {
    let mut out: Vec<Record<'a>> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let (number, line) = (i + 1, raw.trim());
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line.split_once(char::is_whitespace).map_or((line, ""), |(k, v)| (k, v.trim()));
        if key == keyword {
            out.push(Record { line: number, head: value.split_whitespace().collect(), fields: Vec::new() });
        } else if let Some(record) = out.last_mut() {
            if value.is_empty() {
                return error(file, number, format!("\"{key}\" needs a value"));
            }
            record.fields.push((number, key, value));
        } else {
            return error(file, number, format!("expected \"{keyword} ...\", found \"{key}\""));
        }
    }
    Ok(out)
}

/// A whole number of at least `least`.
fn number(file: &str, line: usize, what: &str, text: &str, least: u32) -> Result<u32, ScenarioError> {
    match text.parse::<u32>() {
        Ok(n) if n >= least => Ok(n),
        Ok(_) => error(file, line, format!("{what} must be at least {least}")),
        Err(_) => error(file, line, format!("{what} must be a number, found \"{text}\"")),
    }
}

/// A side's name in a file.
pub fn side_named(word: &str) -> Option<Side> {
    Some(match word {
        "black" => Side::Black,
        "white" => Side::White,
        "red" => Side::Red,
        "green" => Side::Green,
        "blue" => Side::Blue,
        "orange" => Side::Orange,
        "yellow" => Side::Yellow,
        "purple" => Side::Purple,
        "grey" | "gray" | "neutral" | "none" => Side::Grey,
        _ => return None,
    })
}

/// A side's name, as files and the status line have it. Grey is nobody's.
pub fn side_name(side: Side) -> &'static str {
    match side {
        Side::Black => "black",
        Side::White => "white",
        Side::Red => "red",
        Side::Green => "green",
        Side::Blue => "blue",
        Side::Orange => "orange",
        Side::Yellow => "yellow",
        Side::Purple => "purple",
        Side::Grey => "neutral",
    }
}

/// Read `units.txt`: the kinds of unit, in the file's order.
pub fn parse_units(text: &str) -> Result<Vec<UnitKind>, ScenarioError> {
    let file = UNITS_FILE;
    let mut kinds: Vec<UnitKind> = Vec::new();
    for record in records(file, text, "unit")? {
        let id = match record.head[..] {
            [id] => id,
            _ => return error(file, record.line, "expected \"unit <id>\": one word"),
        };
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') {
            return error(file, record.line, format!("a unit's id is lower-case letters, digits and _, not \"{id}\""));
        }
        if kinds.iter().any(|k| k.id == id) {
            return error(file, record.line, format!("there are two units called \"{id}\""));
        }
        let mut kind = UnitKind {
            id: id.to_string(),
            name: id.to_string(),
            image: DEFAULT_UNIT_IMAGE.to_string(),
            strength: 1,
            moves: 0,
            upkeep: 0,
            price: 0,
            cost: MoveCost::default(),
        };
        let mut seen: Vec<&str> = Vec::new();
        for (line, key, value) in record.fields {
            if key != "enter" && seen.contains(&key) {
                return error(file, line, format!("\"{key}\" is given twice for {id}"));
            }
            seen.push(key);
            match key {
                "name" => kind.name = value.to_string(),
                "image" => {
                    let bad = value.starts_with('/')
                        || value.contains('\\')
                        || value.split('/').any(|part| part.is_empty() || part == "." || part == "..");
                    if bad {
                        return error(
                            file,
                            line,
                            format!("an image is a path inside an asset directory, not \"{value}\""),
                        );
                    }
                    kind.image = value.to_string();
                }
                "strength" => kind.strength = number(file, line, "strength", value, 0)?,
                "moves" => kind.moves = number(file, line, "moves", value, 1)?,
                "upkeep" => kind.upkeep = number(file, line, "upkeep", value, 0)?,
                "price" => kind.price = number(file, line, "price", value, 0)?,
                "enter" => {
                    for part in value.split(',').map(str::trim).filter(|p| !p.is_empty()) {
                        let mut words = part.split_whitespace();
                        let (Some(word), Some(points), None) = (words.next(), words.next(), words.next()) else {
                            return error(file, line, format!("expected \"<terrain> <points>\", found \"{part}\""));
                        };
                        let Some(price) = kind.cost.price_mut(word) else {
                            let words = MoveCost::WORDS.join(", ");
                            return error(file, line, format!("\"{word}\" is not one of {words}"));
                        };
                        if *price != 0 {
                            return error(file, line, format!("{word} is given twice for {id}"));
                        }
                        let points = number(file, line, &format!("the points to enter {word}"), points, 1)?;
                        *price = u16::try_from(points)
                            .or_else(|_| error(file, line, format!("the points to enter {word} are at most 65535")))?;
                    }
                }
                _ => {
                    return error(
                        file,
                        line,
                        format!("\"{key}\" is not one of name, image, strength, moves, upkeep, price, enter"),
                    );
                }
            }
        }
        if kind.moves == 0 {
            return error(file, record.line, format!("{id} has no \"moves\""));
        }
        if kind.cost == MoveCost::default() {
            return error(file, record.line, format!("{id} has no \"enter\": it could go nowhere"));
        }
        kinds.push(kind);
    }
    Ok(kinds)
}

/// A city as `cities.txt` has it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CityEntry {
    /// The line its record starts on.
    pub line: usize,
    pub hex: Hex,
    pub name: String,
    pub side: Side,
    pub walls: u32,
    /// What it can build.
    pub produce: Vec<Produce>,
    /// Who stands in it at the start: a unit's id and how many.
    pub garrison: Vec<(String, u32)>,
}

/// A kind of unit a city can build: the unit's id, the turns one takes, and
/// what one costs the city (if it does not say: the unit's `price`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Produce {
    pub unit: String,
    pub turns: u32,
    pub cost: Option<u32>,
}

/// Read `cities.txt`: the cities, in the file's order. Whether their hexes
/// are cities and their units exist is for [`build`] to say.
pub fn parse_cities(text: &str) -> Result<Vec<CityEntry>, ScenarioError> {
    let file = CITIES_FILE;
    let mut cities: Vec<CityEntry> = Vec::new();
    for record in records(file, text, "city")? {
        let hex = match record.head[..] {
            [row, col] => match (row.parse::<u16>(), col.parse::<u16>()) {
                (Ok(row), Ok(col)) => Hex::new(row, col),
                _ => return error(file, record.line, format!("\"{row} {col}\" is not a row and a column")),
            },
            _ => return error(file, record.line, "expected \"city <row> <column>\""),
        };
        let mut city = CityEntry {
            line: record.line,
            hex,
            name: String::new(),
            side: Side::Grey,
            walls: 0,
            produce: Vec::new(),
            garrison: Vec::new(),
        };
        let mut seen: Vec<&str> = Vec::new();
        for (line, key, value) in record.fields {
            let repeats = key == "produce" || key == "garrison";
            if !repeats && seen.contains(&key) {
                return error(file, line, format!("\"{key}\" is given twice for the city at {hex}"));
            }
            seen.push(key);
            let words: Vec<&str> = value.split_whitespace().collect();
            match key {
                "name" => city.name = value.to_string(),
                "side" => {
                    city.side = side_named(value).ok_or(()).or_else(|()| {
                        error(
                            file,
                            line,
                            format!("\"{value}\" is not a side (black, white, red, green, blue, orange, yellow, purple, neutral)"),
                        )
                    })?;
                }
                "walls" => city.walls = number(file, line, "walls", value, 0)?,
                "produce" => {
                    let (unit, turns, cost) = match words[..] {
                        [unit, turns] => (unit, turns, None),
                        [unit, turns, cost] => (unit, turns, Some(cost)),
                        _ => return error(file, line, "expected \"produce <unit> <turns> [<cost>]\""),
                    };
                    if city.produce.iter().any(|p| p.unit == unit) {
                        return error(file, line, format!("{unit} is produced twice"));
                    }
                    let turns = number(file, line, "the turns to build", turns, 1)?;
                    let cost = cost.map(|cost| number(file, line, "the cost", cost, 0)).transpose()?;
                    city.produce.push(Produce { unit: unit.to_string(), turns, cost });
                }
                "garrison" => {
                    let (unit, count) = match words[..] {
                        [unit] => (unit, 1),
                        [unit, count] => (unit, number(file, line, "the number of units", count, 1)?),
                        _ => return error(file, line, "expected \"garrison <unit> [<how many>]\""),
                    };
                    city.garrison.push((unit.to_string(), count));
                }
                _ => {
                    return error(file, line, format!("\"{key}\" is not one of name, side, walls, produce, garrison"));
                }
            }
        }
        if city.name.is_empty() {
            return error(file, record.line, format!("the city at {hex} has no \"name\""));
        }
        if let Some(other) = cities.iter().find(|c| c.hex == hex) {
            return error(file, record.line, format!("{} is at {hex} already", other.name));
        }
        if let Some(other) = cities.iter().find(|c| c.name == city.name) {
            return error(file, record.line, format!("there is a {} at {} already", other.name, other.hex));
        }
        // Summed wider than a count is read: two large counts must not wrap.
        let standing: u64 = city.garrison.iter().map(|(_, n)| u64::from(*n)).sum();
        if standing > MAX_STACK as u64 {
            return error(file, record.line, format!("{standing} units in {}: a hex holds {MAX_STACK}", city.name));
        }
        cities.push(city);
    }
    Ok(cities)
}

/// Put the kinds of unit and the cities on a map that has its terrain. The
/// units of a garrison stand in their city and belong to its side.
pub fn build(mut map: Map, kinds: Vec<UnitKind>, cities: &[CityEntry]) -> Result<(Map, Vec<String>), ScenarioError> {
    for kind in kinds {
        let id = kind.id.clone();
        if map.add_unit_kind(kind).is_none() {
            return error(UNITS_FILE, 0, format!("there are two units called \"{id}\""));
        }
    }
    let file = CITIES_FILE;
    for entry in cities {
        let CityEntry { line, hex, name, .. } = entry;
        match map.tile(*hex).map(|t| t.kind) {
            Some(TileType::City) => {}
            Some(kind) => {
                return error(
                    file,
                    *line,
                    format!(
                        "{name} is at {hex}, which is {} in {TERRAIN_FILE}, not a city (the letter c)",
                        kind.name()
                    ),
                );
            }
            None => {
                let size = format!("{} rows and {} columns", map.rows(), map.cols());
                return error(file, *line, format!("{name} is at {hex}, which is off the map ({size})"));
            }
        }
        let Some(id) = map.add_city(name, *hex, entry.walls) else {
            return error(file, *line, format!("there is a city at {hex} already"));
        };
        if let Some(city) = map.city_mut(id) {
            city.side = entry.side;
        }
        // The kinds do not change from here on.
        let known: Vec<String> = map.units().kinds().iter().map(|k| k.id.clone()).collect();
        let known = if known.is_empty() { format!("{UNITS_FILE} has none") } else { known.join(", ") };
        let unknown = |unit: &str| format!("{name}: there is no unit \"{unit}\" ({known})");
        for Produce { unit, turns, cost } in &entry.produce {
            let Some(kind) = map.units().kind_named(unit).and_then(|k| map.units().kind(k)).cloned() else {
                return error(file, *line, unknown(unit));
            };
            // The city's own cost, if it gives one; else the unit's price.
            let prod = UnitProd {
                basename: kind.id,
                time: *turns,
                cost: cost.unwrap_or(kind.price),
                upkeep: kind.upkeep,
                strength: kind.strength,
                movement: kind.moves,
            };
            map.add_city_prod(id, prod);
        }
        for (unit, count) in &entry.garrison {
            let Some(kind) = map.units().kind_named(unit) else { return error(file, *line, unknown(unit)) };
            for _ in 0..*count {
                let placed = map.new_unit_of(entry.side, kind).map(|u| map.place_unit(u, *hex));
                if !matches!(placed, Some(Ok(()))) {
                    return error(file, *line, format!("{name}: no room for its garrison"));
                }
            }
        }
    }
    // City tiles nobody named are cities all the same, but probably forgotten.
    let mut warnings = Vec::new();
    for row in 0..map.rows() {
        for col in 0..map.cols() {
            let hex = Hex::new(row, col);
            if map.tile(hex).is_some_and(|t| t.kind == TileType::City) && map.city_at(hex).is_none() {
                warnings.push(format!("the city tile at {hex} has no entry in {CITIES_FILE}"));
            }
        }
    }
    Ok((map, warnings))
}

/// A map from the text of its files (`None`: the file is not there).
pub fn parse(terrain: &str, units: Option<&str>, cities: Option<&str>) -> Result<(Map, Vec<String>), ScenarioError> {
    let map = Map::parse(terrain).map_err(|e| from_map_error(TERRAIN_FILE, e))?;
    let kinds = units.map(parse_units).transpose()?.unwrap_or_default();
    let cities = cities.map(parse_cities).transpose()?.unwrap_or_default();
    build(map, kinds, &cities)
}

fn from_map_error(file: &str, e: MapError) -> ScenarioError {
    match e {
        MapError::Parse { line, message } => ScenarioError { file: file.to_string(), line, message },
        other => ScenarioError { file: file.to_string(), line: 0, message: other.to_string() },
    }
}

/// The terrain file of a map: the file itself, or `terrain.map` in a map
/// directory.
pub fn terrain_path(path: &Path) -> PathBuf {
    if path.is_dir() { path.join(TERRAIN_FILE) } else { path.to_path_buf() }
}

/// Read a map: a `.map` file (terrain and roads only), or a map directory.
/// Errors name the file as it was asked for.
pub fn load(path: &Path) -> Result<Scenario, ScenarioError> {
    let read = |file: &Path| -> Result<String, ScenarioError> {
        std::fs::read_to_string(file).or_else(|e| error(&file.display().to_string(), 0, e.to_string()))
    };
    let optional = |file: PathBuf| -> Result<Option<String>, ScenarioError> {
        if file.is_file() { read(&file).map(Some) } else { Ok(None) }
    };
    // In a directory, the errors name its files by their whole path.
    let within = |e: ScenarioError| ScenarioError { file: path.join(&e.file).display().to_string(), ..e };
    if !path.is_dir() {
        let text = read(path)?;
        let map = Map::parse(&text).map_err(|e| from_map_error(&path.display().to_string(), e))?;
        let (map, warnings) = build(map, Vec::new(), &[])?;
        return Ok(Scenario { map, assets: None, warnings });
    }
    let terrain = read(&path.join(TERRAIN_FILE))?;
    let units = optional(path.join(UNITS_FILE))?;
    let cities = optional(path.join(CITIES_FILE))?;
    let (map, warnings) = parse(&terrain, units.as_deref(), cities.as_deref()).map_err(within)?;
    let assets = Some(path.join(ASSETS_DIR)).filter(|dir| dir.is_dir());
    Ok(Scenario { map, assets, warnings })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 20 x 20 map of grass with cities at (5, 5) and (12, 14), a road
    /// between two hexes of row 5, and a bridge.
    fn terrain() -> String {
        let mut rows = vec!["g".repeat(20); 20];
        rows[5] = "gggggcGGWggggggggggg".to_string();
        rows[12] = "ggggggggggggggcggggg".to_string();
        format!("20\n20\n{}\n", rows.join("\n"))
    }

    const UNITS: &str = "\
# The kinds of unit of this map.
unit spearmen
  name     Hill Spearmen
  image    units/spearmen.png
  strength 3
  moves    12
  upkeep   1
  price    6
  enter    grass 2, forest 3, hill 3
  enter    city 1, road 1, bridge 2

unit scouts
moves 20
enter grass 1
";

    const CITIES: &str = "\
city 5 5
  name     Wrenfold
  side     red
  walls    2
  produce  spearmen 2
  produce  scouts 1 3
  garrison spearmen 2
  garrison scouts

# Nobody's.
city 12 14
  name Low Tarn
";

    fn message(result: Result<(Map, Vec<String>), ScenarioError>) -> String {
        result.err().map(|e| e.to_string()).unwrap_or_else(|| "it was read".to_string())
    }

    #[test]
    fn a_whole_map_is_read() {
        let (map, warnings) = parse(&terrain(), Some(UNITS), Some(CITIES)).unwrap();
        assert_eq!(warnings, Vec::<String>::new());
        // The roads: capitals in the terrain.
        assert!(map.tile(Hex::new(5, 6)).unwrap().road && map.tile(Hex::new(5, 7)).unwrap().road);
        let bridge = map.tile(Hex::new(5, 8)).unwrap();
        assert!(bridge.road && bridge.kind == TileType::Water);
        assert!(!map.tile(Hex::new(5, 5)).unwrap().road && !map.tile(Hex::new(5, 9)).unwrap().road);

        // The kinds of unit.
        let kinds = map.units().kinds();
        assert_eq!(kinds.len(), 2);
        let spearmen = &kinds[0];
        assert_eq!((spearmen.id.as_str(), spearmen.name.as_str()), ("spearmen", "Hill Spearmen"));
        assert_eq!(spearmen.image, "units/spearmen.png");
        assert_eq!((spearmen.strength, spearmen.moves, spearmen.upkeep, spearmen.price), (3, 12, 1, 6));
        assert_eq!(
            spearmen.cost,
            MoveCost { grass: 2, forest: 3, hill: 3, city: 1, road: 1, bridge: 2, ..MoveCost::default() }
        );
        // What is left out has its default.
        let scouts = &kinds[1];
        assert_eq!((scouts.name.as_str(), scouts.image.as_str()), ("scouts", DEFAULT_UNIT_IMAGE));
        assert_eq!((scouts.strength, scouts.moves, scouts.upkeep, scouts.price), (1, 20, 0, 0));

        // The cities.
        assert_eq!(map.cities().len(), 2);
        let wrenfold = map.city_at(Hex::new(5, 5)).unwrap();
        assert_eq!((wrenfold.name.as_str(), wrenfold.side, wrenfold.walls), ("Wrenfold", Side::Red, 2));
        let builds: Vec<(&str, u32, u32, u32)> =
            wrenfold.units.iter().map(|p| (p.basename.as_str(), p.time, p.cost, p.movement)).collect();
        // The spearmen cost their price; the scouts what Wrenfold says.
        assert_eq!(builds, [("spearmen", 2, 6, 12), ("scouts", 1, 3, 20)]);
        let tarn = map.city_at(Hex::new(12, 14)).unwrap();
        assert_eq!((tarn.name.as_str(), tarn.side, tarn.walls, tarn.units.len()), ("Low Tarn", Side::Grey, 0, 0));

        // The garrison stands in its city, on its side, with its kind's numbers.
        let standing: Vec<_> = map.units_at(Hex::new(5, 5)).iter().map(|id| map.units().get(*id).unwrap()).collect();
        let names: Vec<&str> = standing.iter().map(|u| u.name.as_str()).collect();
        assert_eq!(names, ["Hill Spearmen", "Hill Spearmen", "scouts"]);
        assert!(standing.iter().all(|u| u.side() == Side::Red && u.pos() == Hex::new(5, 5)));
        assert_eq!((standing[0].movement, standing[2].movement), (12, 20));
        assert!(map.units_at(Hex::new(12, 14)).is_empty());
    }

    #[test]
    fn terrain_alone_is_a_map_and_unnamed_cities_are_a_warning() {
        let (map, warnings) = parse(&terrain(), None, None).unwrap();
        assert!(map.units().kinds().is_empty() && map.cities().is_empty());
        assert_eq!(
            warnings,
            [
                "the city tile at (5, 5) has no entry in cities.txt".to_string(),
                "the city tile at (12, 14) has no entry in cities.txt".to_string()
            ]
        );
    }

    #[test]
    fn what_is_wrong_with_the_units_is_said_with_its_line() {
        for (text, why) in [
            ("moves 3\n", "units.txt line 1: expected \"unit ...\", found \"moves\""),
            ("unit\n", "units.txt line 1: expected \"unit <id>\": one word"),
            ("unit Two Words\n", "units.txt line 1: expected \"unit <id>\": one word"),
            ("unit Pike\n", "units.txt line 1: a unit's id is lower-case letters, digits and _, not \"Pike\""),
            ("unit a\nmoves 3\nenter grass 1\nunit a\n", "units.txt line 4: there are two units called \"a\""),
            ("unit a\nenter grass 1\n", "units.txt line 1: a has no \"moves\""),
            ("unit a\nmoves 3\n", "units.txt line 1: a has no \"enter\": it could go nowhere"),
            ("unit a\nmoves\n", "units.txt line 2: \"moves\" needs a value"),
            ("unit a\nmoves 0\n", "units.txt line 2: moves must be at least 1"),
            ("unit a\nmoves many\n", "units.txt line 2: moves must be a number, found \"many\""),
            ("unit a\nmoves 3\nmoves 4\n", "units.txt line 3: \"moves\" is given twice for a"),
            (
                "unit a\nspeed 3\n",
                "units.txt line 2: \"speed\" is not one of name, image, strength, moves, upkeep, price, enter",
            ),
            (
                "unit a\nenter lava 2\n",
                "units.txt line 2: \"lava\" is not one of grass, forest, swamp, desert, hill, mountain, water, city, road, bridge",
            ),
            ("unit a\nenter grass\n", "units.txt line 2: expected \"<terrain> <points>\", found \"grass\""),
            ("unit a\nenter grass 0\n", "units.txt line 2: the points to enter grass must be at least 1"),
            ("unit a\nenter grass 2, grass 3\n", "units.txt line 2: grass is given twice for a"),
            (
                "unit a\nimage ../../secret.png\n",
                "units.txt line 2: an image is a path inside an asset directory, not \"../../secret.png\"",
            ),
            (
                "unit a\nimage /etc/passwd\n",
                "units.txt line 2: an image is a path inside an asset directory, not \"/etc/passwd\"",
            ),
        ] {
            assert_eq!(message(parse(&terrain(), Some(text), None)), why, "{text:?}");
        }
    }

    #[test]
    fn what_is_wrong_with_the_cities_is_said_with_its_line() {
        let city = |rest: &str| format!("city 5 5\n{rest}");
        for (text, why) in [
            ("city 5\n".to_string(), "cities.txt line 1: expected \"city <row> <column>\""),
            ("city five 5\n".to_string(), "cities.txt line 1: \"five 5\" is not a row and a column"),
            (city(""), "cities.txt line 1: the city at (5, 5) has no \"name\""),
            (city("name A\nname B\n"), "cities.txt line 3: \"name\" is given twice for the city at (5, 5)"),
            (
                city("name A\nside pink\n"),
                "cities.txt line 3: \"pink\" is not a side (black, white, red, green, blue, orange, yellow, purple, neutral)",
            ),
            (city("name A\nwalls high\n"), "cities.txt line 3: walls must be a number, found \"high\""),
            (
                city("name A\nmayor Bo\n"),
                "cities.txt line 3: \"mayor\" is not one of name, side, walls, produce, garrison",
            ),
            (city("name A\nproduce scouts\n"), "cities.txt line 3: expected \"produce <unit> <turns> [<cost>]\""),
            (city("name A\nproduce scouts 1 2 3\n"), "cities.txt line 3: expected \"produce <unit> <turns> [<cost>]\""),
            (city("name A\nproduce scouts 1 dear\n"), "cities.txt line 3: the cost must be a number, found \"dear\""),
            (city("name A\nproduce scouts 0\n"), "cities.txt line 3: the turns to build must be at least 1"),
            (city("name A\nproduce scouts 1\nproduce scouts 2\n"), "cities.txt line 4: scouts is produced twice"),
            (city("name A\nproduce pikes 1\n"), "cities.txt line 1: A: there is no unit \"pikes\" (spearmen, scouts)"),
            (city("name A\ngarrison pikes\n"), "cities.txt line 1: A: there is no unit \"pikes\" (spearmen, scouts)"),
            (city("name A\ngarrison scouts 1 2\n"), "cities.txt line 3: expected \"garrison <unit> [<how many>]\""),
            (
                city("name A\ngarrison scouts 5\ngarrison spearmen 4\n"),
                "cities.txt line 1: 9 units in A: a hex holds 8",
            ),
            (
                city("name A\ngarrison scouts 4294967295\ngarrison spearmen 4294967295\n"),
                "cities.txt line 1: 8589934590 units in A: a hex holds 8",
            ),
            (city("name A\ncity 5 5\nname B\n"), "cities.txt line 3: A is at (5, 5) already"),
            (city("name A\ncity 12 14\nname A\n"), "cities.txt line 3: there is a A at (5, 5) already"),
            (
                "city 5 6\nname A\n".to_string(),
                "cities.txt line 1: A is at (5, 6), which is grass in terrain.map, not a city (the letter c)",
            ),
            (
                "city 30 6\nname A\n".to_string(),
                "cities.txt line 1: A is at (30, 6), which is off the map (20 rows and 20 columns)",
            ),
        ] {
            assert_eq!(message(parse(&terrain(), Some(UNITS), Some(&text))), why, "{text:?}");
        }
        // Without units.txt no unit is known.
        assert_eq!(
            message(parse(&terrain(), None, Some(&city("name A\ngarrison scouts\n")))),
            "cities.txt line 1: A: there is no unit \"scouts\" (units.txt has none)"
        );
        // A mistake in the terrain names its file too.
        let bad = terrain().replacen("gggggcGGWg", "gggggcGGXg", 1);
        assert_eq!(message(parse(&bad, None, None)), "terrain.map line 8: unknown terrain 'X' at column 9");
    }

    #[test]
    fn sides_have_names() {
        for side in Side::PLAYERS {
            assert_eq!(side_named(side_name(side)), Some(side));
        }
        assert_eq!(side_named("neutral"), Some(Side::Grey));
        assert_eq!(side_named("grey"), Some(Side::Grey));
        assert_eq!(side_name(Side::Grey), "neutral");
        assert_eq!(side_named("Red"), None);
    }

    #[test]
    fn a_map_is_a_file_or_a_directory() {
        let dir = std::env::temp_dir().join(format!("hexgame-scenario-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("whole/assets/units")).unwrap();
        std::fs::write(dir.join("whole").join(TERRAIN_FILE), terrain()).unwrap();
        std::fs::write(dir.join("whole").join(UNITS_FILE), UNITS).unwrap();
        std::fs::write(dir.join("whole").join(CITIES_FILE), CITIES).unwrap();
        std::fs::write(dir.join("plain.map"), terrain()).unwrap();

        let whole = load(&dir.join("whole")).unwrap();
        assert_eq!(whole.map.cities().len(), 2);
        assert_eq!(whole.assets, Some(dir.join("whole/assets")));
        assert_eq!(terrain_path(&dir.join("whole")), dir.join("whole").join(TERRAIN_FILE));

        // A file is terrain and roads, and nothing else.
        let plain = load(&dir.join("plain.map")).unwrap();
        assert!(plain.map.cities().is_empty() && plain.assets.is_none());
        assert!(plain.map.tile(Hex::new(5, 6)).unwrap().road);
        assert_eq!(plain.warnings.len(), 2);
        assert_eq!(terrain_path(&dir.join("plain.map")), dir.join("plain.map"));

        // A directory without units, cities or assets is a map too; one
        // without terrain is not, and errors name the file with its path.
        std::fs::create_dir_all(dir.join("bare")).unwrap();
        std::fs::write(dir.join("bare").join(TERRAIN_FILE), terrain()).unwrap();
        assert!(load(&dir.join("bare")).unwrap().assets.is_none());
        std::fs::create_dir_all(dir.join("empty")).unwrap();
        let e = load(&dir.join("empty")).unwrap_err();
        assert!(e.file.ends_with("terrain.map") && e.line == 0, "{e}");
        std::fs::write(dir.join("whole").join(CITIES_FILE), "city 1 1\nname X\n").unwrap();
        let e = load(&dir.join("whole")).unwrap_err();
        assert_eq!(e.file, dir.join("whole").join(CITIES_FILE).display().to_string());
        assert!(
            e.to_string().ends_with(
                "cities.txt line 1: X is at (1, 1), which is grass in terrain.map, not a city (the letter c)"
            ),
            "{e}"
        );
        assert!(load(&dir.join("nothing.map")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
