//! The map: a grid of tiles, the units standing on them, and the cities.
//!
//! C++: src/state/map.h, src/state/map.cpp (Tile, Map)
//!
//! The C++ `Tile` also had `currentMoveCost` and `stacked`, scratch fields of the
//! movement flood fill. They live in `movement::MoveRange` now, so selecting a
//! unit no longer changes the map.

use std::fmt;
use std::path::Path;

use super::cities::{Cities, City, CityId, UnitProd};
use super::hex::Hex;
use super::terrain::{MoveCost, TileType};
use super::units::{MoveCostId, Side, Unit, UnitId, Units};

/// The most units that can stand on one tile.
pub const MAX_STACK: usize = 8;
pub const MIN_ROWS: u16 = 20;
pub const MAX_ROWS: u16 = 600;
pub const MIN_COLS: u16 = 20;
pub const MAX_COLS: u16 = 600;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Tile {
    pub kind: TileType,
    pub frozen: bool,
    pub road: bool,
    /// At most [`MAX_STACK`], in the order they arrived.
    units: Vec<UnitId>,
}

impl Tile {
    fn new(kind: TileType) -> Tile {
        Tile { kind, frozen: false, road: false, units: Vec::new() }
    }

    /// The units on this tile, in the order they arrived.
    pub fn units(&self) -> &[UnitId] {
        &self.units
    }
}

/// Why a map could not be made or read.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MapError {
    /// Each axis must be 20..=600.
    BadDimensions {
        rows: i64,
        cols: i64,
    },
    /// The map file is malformed. `line` is 1-based.
    Parse {
        line: usize,
        message: String,
    },
    Io(String),
}

impl fmt::Display for MapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MapError::BadDimensions { rows, cols } => {
                write!(f, "invalid map dimensions: {rows} rows, {cols} columns (each must be {MIN_ROWS} to {MAX_ROWS})")
            }
            MapError::Parse { line, message } => write!(f, "map file line {line}: {message}"),
            MapError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for MapError {}

/// Why a unit could not be put on a tile.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum PlaceError {
    NoSuchUnit,
    OffMap,
    /// The tile already holds [`MAX_STACK`] units.
    StackFull,
    /// `place_unit` is for units that are not on the map yet; use `move_unit`.
    AlreadyOnMap,
    /// `move_unit` is for units that are on the map; use `place_unit` first.
    NotOnMap,
}

impl fmt::Display for PlaceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            PlaceError::NoSuchUnit => "no such unit",
            PlaceError::OffMap => "the hex is not on the map",
            PlaceError::StackFull => "the stack is full",
            PlaceError::AlreadyOnMap => "the unit is already on the map",
            PlaceError::NotOnMap => "the unit is not on the map",
        })
    }
}

impl std::error::Error for PlaceError {}

/// The map owns its units and cities, and everything that changes them goes
/// through its methods, which keep the tiles' unit lists, the units' positions
/// and the revision counters in step.
#[derive(Clone, Debug)]
pub struct Map {
    rows: u16,
    cols: u16,
    /// Row-major: `tiles[row * cols + col]`.
    tiles: Vec<Tile>,
    /// The hex the player points at. C++: Map::row, Map::col
    pub cursor: Hex,
    units: Units,
    cities: Cities,
    /// Counts the changes to anything but the cursor.
    revision: u64,
    /// Counts the changes to the terrain.
    terrain_revision: u64,
}

impl Map {
    /// A map of one terrain, with the cursor in the middle. C++: Map::Map, allocMap
    pub fn new(rows: u16, cols: u16, default: TileType) -> Result<Map, MapError> {
        if !(MIN_ROWS..=MAX_ROWS).contains(&rows) || !(MIN_COLS..=MAX_COLS).contains(&cols) {
            return Err(MapError::BadDimensions { rows: rows.into(), cols: cols.into() });
        }
        Ok(Map {
            rows,
            cols,
            tiles: vec![Tile::new(default); usize::from(rows) * usize::from(cols)],
            cursor: Hex::new(rows / 2, cols / 2),
            units: Units::new(),
            cities: Cities::new(),
            revision: 0,
            terrain_revision: 0,
        })
    }

    /// Read a map file: the number of rows, the number of columns, then one line
    /// per row with one letter per tile (see `TileType::from_char`). Tiles a short
    /// or missing line leaves out are grass, as in the C++; a letter that is not a
    /// terrain is an error. Both `\n` and `\r\n` end a line.
    /// C++: Map::Map(std::string file), which wrote through a null pointer
    /// (deviation D1)
    pub fn parse(text: &str) -> Result<Map, MapError> {
        let mut lines = text.lines().map(str::trim_end);
        let mut dimension = |line: usize, what: &str| -> Result<i64, MapError> {
            let s = lines.next().unwrap_or("").trim();
            s.parse::<i64>().map_err(|_| MapError::Parse { line, message: format!("expected {what}, found \"{s}\"") })
        };
        let rows = dimension(1, "the number of rows")?;
        let cols = dimension(2, "the number of columns")?;
        let bad = || MapError::BadDimensions { rows, cols };
        let mut map = Map::new(
            u16::try_from(rows).map_err(|_| bad())?,
            u16::try_from(cols).map_err(|_| bad())?,
            TileType::Grass,
        )?;
        let width = usize::from(map.cols);
        for (r, line) in lines.take(usize::from(map.rows)).enumerate() {
            let mut count = 0;
            for (c, ch) in line.chars().enumerate() {
                let parse_error = |message: String| MapError::Parse { line: r + 3, message };
                if c >= width {
                    return Err(parse_error(format!("more than {width} tiles")));
                }
                let kind = TileType::from_char(ch)
                    .ok_or_else(|| parse_error(format!("unknown terrain '{ch}' at column {}", c + 1)))?;
                map.tiles[r * width + c].kind = kind;
                count += 1;
            }
            debug_assert!(count <= width);
        }
        Ok(map)
    }

    /// The map as a map file (terrain only), with `\n` line ends. C++: Map::write
    pub fn to_text(&self) -> String {
        let mut out = format!("{}\n{}\n", self.rows, self.cols);
        for row in self.tiles.chunks(usize::from(self.cols)) {
            out.extend(row.iter().map(|t| t.kind.to_char()));
            out.push('\n');
        }
        out
    }

    pub fn load(path: &Path) -> Result<Map, MapError> {
        let text = std::fs::read_to_string(path).map_err(|e| MapError::Io(format!("{}: {e}", path.display())))?;
        Map::parse(&text)
    }

    pub fn save(&self, path: &Path) -> Result<(), MapError> {
        std::fs::write(path, self.to_text()).map_err(|e| MapError::Io(format!("{}: {e}", path.display())))
    }

    pub fn rows(&self) -> u16 {
        self.rows
    }

    pub fn cols(&self) -> u16 {
        self.cols
    }

    pub fn contains(&self, hex: Hex) -> bool {
        hex.row < self.rows && hex.col < self.cols
    }

    /// The hex at signed coordinates, if it is on the map.
    pub fn hex_at(&self, row: i32, col: i32) -> Option<Hex> {
        Hex::on_map(row, col, self.rows, self.cols)
    }

    fn index(&self, hex: Hex) -> Option<usize> {
        self.contains(hex).then(|| usize::from(hex.row) * usize::from(self.cols) + usize::from(hex.col))
    }

    pub fn tile(&self, hex: Hex) -> Option<&Tile> {
        self.index(hex).map(|i| &self.tiles[i])
    }

    /// The tile, to change its terrain or flags (its units change through
    /// `place_unit`, `move_unit` and the kill methods). This counts as a change
    /// to the terrain.
    pub fn tile_mut(&mut self, hex: Hex) -> Option<&mut Tile> {
        let i = self.index(hex)?;
        self.terrain_changed();
        Some(&mut self.tiles[i])
    }

    /// A number that goes up whenever the map changes (its terrain, its units,
    /// its cities; not the cursor). What is computed from the map, like a unit's
    /// movement range, is out of date when this has moved on.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// A number that goes up whenever the terrain changes. The minimap's picture
    /// is out of date when this has moved on.
    pub fn terrain_revision(&self) -> u64 {
        self.terrain_revision
    }

    fn changed(&mut self) {
        self.revision += 1;
    }

    fn terrain_changed(&mut self) {
        self.revision += 1;
        self.terrain_revision += 1;
    }

    /// Every unit in the game. They change through the map's methods.
    pub fn units(&self) -> &Units {
        &self.units
    }

    pub fn cities(&self) -> &Cities {
        &self.cities
    }

    /// Create a unit. It is not on the map until `place_unit` puts it there.
    /// C++: UnitManager::newUnit
    pub fn new_unit(
        &mut self,
        side: Side,
        name: &str,
        upkeep: u32,
        strength: u32,
        movement: u32,
        move_cost: MoveCostId,
    ) -> UnitId {
        self.changed();
        self.units.new_unit(side, name, upkeep, strength, movement, move_cost)
    }

    /// The id of the movement cost table with these costs (see
    /// `Units::acquire_move_cost`).
    pub fn acquire_move_cost(&mut self, cost: MoveCost) -> MoveCostId {
        self.units.acquire_move_cost(cost)
    }

    /// A unit, to change what it can do (its movement points, strength, name).
    /// Where it is changes through `move_unit`.
    pub fn unit_mut(&mut self, id: UnitId) -> Option<&mut Unit> {
        self.units.get(id)?;
        self.changed();
        self.units.get_mut(id)
    }

    /// Start a side's turn (see `Units::reset_move_points`).
    pub fn reset_move_points(&mut self, side: Side) {
        self.changed();
        self.units.reset_move_points(side);
    }

    /// The neighbours of `hex` that are on the map.
    pub fn neighbours(&self, hex: Hex) -> impl Iterator<Item = Hex> {
        hex.neighbours(self.rows, self.cols)
    }

    /// The units on a hex (none if it is off the map).
    pub fn units_at(&self, hex: Hex) -> &[UnitId] {
        self.tile(hex).map_or(&[], |t| t.units())
    }

    fn is_on_map(&self, unit: &Unit) -> bool {
        self.units_at(unit.pos()).contains(&unit.id())
    }

    /// Put a new unit on the map. C++: Map::placeUnit (which ignored a full stack)
    pub fn place_unit(&mut self, id: UnitId, hex: Hex) -> Result<(), PlaceError> {
        let unit = self.units.get(id).ok_or(PlaceError::NoSuchUnit)?;
        if self.is_on_map(unit) {
            return Err(PlaceError::AlreadyOnMap);
        }
        self.push_unit(id, hex)
    }

    fn push_unit(&mut self, id: UnitId, hex: Hex) -> Result<(), PlaceError> {
        let i = self.index(hex).ok_or(PlaceError::OffMap)?;
        if self.tiles[i].units.len() >= MAX_STACK {
            return Err(PlaceError::StackFull);
        }
        self.tiles[i].units.push(id);
        if let Some(unit) = self.units.get_mut(id) {
            unit.set_pos(hex);
        }
        self.changed();
        Ok(())
    }

    fn remove_from_tile(&mut self, id: UnitId, hex: Hex) {
        if let Some(i) = self.index(hex) {
            self.tiles[i].units.retain(|u| *u != id);
        }
    }

    /// Move a unit that is on the map to another hex. Nothing changes if it cannot
    /// go there. C++: Map::moveUnit (which lost the unit if the stack was full)
    pub fn move_unit(&mut self, id: UnitId, to: Hex) -> Result<(), PlaceError> {
        let unit = self.units.get(id).ok_or(PlaceError::NoSuchUnit)?;
        let from = unit.pos();
        if !self.is_on_map(unit) {
            return Err(PlaceError::NotOnMap);
        }
        if from == to {
            return Ok(());
        }
        self.push_unit(id, to)?;
        self.remove_from_tile(id, from);
        Ok(())
    }

    /// Remove a unit from the map and the game. C++: Map::killUnit
    pub fn kill_unit(&mut self, id: UnitId) -> Option<Unit> {
        let unit = self.units.delete(id)?;
        self.remove_from_tile(id, unit.pos());
        self.changed();
        Some(unit)
    }

    /// Remove every unit on a hex. Returns how many there were. C++: Map::killTile
    pub fn kill_tile(&mut self, hex: Hex) -> usize {
        let Some(i) = self.index(hex) else { return 0 };
        let ids = std::mem::take(&mut self.tiles[i].units);
        for id in &ids {
            self.units.delete(*id);
        }
        if !ids.is_empty() {
            self.changed();
        }
        ids.len()
    }

    /// Found a neutral city: the tile becomes a city tile. `None` if the hex is off
    /// the map or has a city already. C++: Map::addCity (which allowed a second
    /// city on a hex, where `getCity` could never find it)
    pub fn add_city(&mut self, name: &str, hex: Hex, walls: u32) -> Option<CityId> {
        if self.cities.at(hex).is_some() {
            return None;
        }
        self.tile_mut(hex)?.kind = TileType::City;
        Some(self.cities.new_city(name, hex, walls))
    }

    /// Add a kind of unit to what a city can build. False if there is no such
    /// city. C++: CityManager::addCityProd
    pub fn add_city_prod(&mut self, id: CityId, prod: UnitProd) -> bool {
        self.changed();
        self.cities.add_city_prod(id, prod)
    }

    /// C++: Map::getCity
    pub fn city_at(&self, hex: Hex) -> Option<&City> {
        self.cities.at(hex)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map() -> Map {
        Map::new(20, 30, TileType::Grass).unwrap()
    }

    fn add_unit(map: &mut Map, name: &str) -> UnitId {
        let mc = map.acquire_move_cost(MoveCost { grass: 2, ..MoveCost::default() });
        map.new_unit(Side::Black, name, 1, 1, 10, mc)
    }

    #[test]
    fn a_new_map_is_one_terrain_with_the_cursor_in_the_middle() {
        let m = Map::new(20, 30, TileType::Water).unwrap();
        assert_eq!((m.rows(), m.cols()), (20, 30));
        assert_eq!(m.cursor, Hex::new(10, 15));
        assert_eq!(m.tile(Hex::new(19, 29)).unwrap().kind, TileType::Water);
        assert!(m.tile(Hex::new(20, 0)).is_none());
        assert!(m.tile(Hex::new(0, 30)).is_none());
        assert_eq!(m.hex_at(19, 29), Some(Hex::new(19, 29)));
        assert_eq!(m.hex_at(-1, 0), None);
    }

    #[test]
    fn bad_dimensions_are_rejected() {
        for (rows, cols) in [(19, 20), (20, 19), (601, 20), (20, 601), (0, 0)] {
            assert_eq!(
                Map::new(rows, cols, TileType::Grass).err(),
                Some(MapError::BadDimensions { rows: rows.into(), cols: cols.into() }),
                "{rows} x {cols}"
            );
        }
        assert!(Map::new(20, 20, TileType::Grass).is_ok());
        assert!(Map::new(600, 600, TileType::Grass).is_ok());
    }

    #[test]
    fn a_map_file_round_trips() {
        let mut m = map();
        // Every terrain somewhere, including the corners.
        for (i, kind) in TileType::ALL.into_iter().enumerate() {
            m.tile_mut(Hex::new(i as u16, (i * 2) as u16)).unwrap().kind = kind;
        }
        m.tile_mut(Hex::new(0, 0)).unwrap().kind = TileType::Mountain;
        m.tile_mut(Hex::new(19, 29)).unwrap().kind = TileType::Water;
        let text = m.to_text();
        assert!(text.starts_with("20\n30\nm"));
        assert_eq!(text.lines().count(), 22);
        let back = Map::parse(&text).unwrap();
        assert_eq!((back.rows(), back.cols()), (20, 30));
        for row in 0..20 {
            for col in 0..30 {
                let h = Hex::new(row, col);
                assert_eq!(back.tile(h).unwrap().kind, m.tile(h).unwrap().kind, "{h}");
            }
        }
        assert_eq!(back.to_text(), text);
    }

    #[test]
    fn a_map_file_round_trips_through_the_file_system() {
        let mut m = map();
        m.tile_mut(Hex::new(3, 7)).unwrap().kind = TileType::Forest;
        let path = std::env::temp_dir().join(format!("hexgame-map-test-{}.txt", std::process::id()));
        m.save(&path).unwrap();
        let back = Map::load(&path);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(back.unwrap().to_text(), m.to_text());
        assert!(matches!(Map::load(&path), Err(MapError::Io(_))));
    }

    #[test]
    fn windows_line_ends_parse() {
        let unix = map().to_text();
        let windows = unix.replace('\n', "\r\n");
        assert_eq!(Map::parse(&windows).unwrap().to_text(), unix);
    }

    #[test]
    fn short_and_missing_lines_are_grass() {
        let text = format!("20\n20\n{}\nmm\n", "w".repeat(20));
        let m = Map::parse(&text).unwrap();
        assert_eq!(m.tile(Hex::new(0, 19)).unwrap().kind, TileType::Water);
        assert_eq!(m.tile(Hex::new(1, 1)).unwrap().kind, TileType::Mountain);
        assert_eq!(m.tile(Hex::new(1, 2)).unwrap().kind, TileType::Grass);
        assert_eq!(m.tile(Hex::new(19, 19)).unwrap().kind, TileType::Grass);
    }

    #[test]
    fn bad_map_files_are_rejected() {
        assert_eq!(Map::parse("10\n20\n").err(), Some(MapError::BadDimensions { rows: 10, cols: 20 }));
        assert_eq!(Map::parse("20\n70000\n").err(), Some(MapError::BadDimensions { rows: 20, cols: 70000 }));
        assert_eq!(Map::parse("-20\n20\n").err(), Some(MapError::BadDimensions { rows: -20, cols: 20 }));
        assert!(matches!(Map::parse(""), Err(MapError::Parse { line: 1, .. })));
        assert!(matches!(Map::parse("20\n"), Err(MapError::Parse { line: 2, .. })));
        assert!(matches!(Map::parse("twenty\n20\n"), Err(MapError::Parse { line: 1, .. })));
        // An unknown letter, on the second row (file line 4).
        let text = format!("20\n20\n{}\nggxg\n", "g".repeat(20));
        match Map::parse(&text) {
            Err(MapError::Parse { line: 4, message }) => assert!(message.contains("'x'"), "{message}"),
            other => panic!("{other:?}"),
        }
        // A row with too many tiles.
        let text = format!("20\n20\n{}\n", "g".repeat(21));
        assert!(matches!(Map::parse(&text), Err(MapError::Parse { line: 3, .. })));
    }

    #[test]
    fn placing_and_moving_units() {
        let mut m = map();
        let a = add_unit(&mut m, "A");
        let (from, to) = (Hex::new(5, 5), Hex::new(5, 6));
        assert_eq!(m.move_unit(a, to), Err(PlaceError::NotOnMap));
        m.place_unit(a, from).unwrap();
        assert_eq!(m.units_at(from), [a]);
        assert_eq!(m.units().get(a).unwrap().pos(), from);
        assert_eq!(m.place_unit(a, to), Err(PlaceError::AlreadyOnMap));

        m.move_unit(a, to).unwrap();
        assert!(m.units_at(from).is_empty());
        assert_eq!(m.units_at(to), [a]);
        assert_eq!(m.units().get(a).unwrap().pos(), to);
        // Moving to where it is changes nothing.
        m.move_unit(a, to).unwrap();
        assert_eq!(m.units_at(to), [a]);

        assert_eq!(m.move_unit(a, Hex::new(20, 0)), Err(PlaceError::OffMap));
        assert_eq!(m.units_at(to), [a]);
        let ghost = UnitId { side: Side::Red, index: 3, generation: 0 };
        assert_eq!(m.place_unit(ghost, from), Err(PlaceError::NoSuchUnit));
        assert_eq!(m.move_unit(ghost, from), Err(PlaceError::NoSuchUnit));
    }

    #[test]
    fn a_stack_holds_eight() {
        let mut m = map();
        let hex = Hex::new(4, 4);
        let ids: Vec<UnitId> = (0..MAX_STACK).map(|i| add_unit(&mut m, &format!("U{i}"))).collect();
        for id in &ids {
            m.place_unit(*id, hex).unwrap();
        }
        assert_eq!(m.units_at(hex), ids);

        let ninth = add_unit(&mut m, "ninth");
        assert_eq!(m.place_unit(ninth, hex), Err(PlaceError::StackFull));
        // A unit that cannot join the stack stays where it was.
        let next_door = Hex::new(4, 5);
        m.place_unit(ninth, next_door).unwrap();
        assert_eq!(m.move_unit(ninth, hex), Err(PlaceError::StackFull));
        assert_eq!(m.units_at(next_door), [ninth]);
        assert_eq!(m.units().get(ninth).unwrap().pos(), next_door);
        assert_eq!(m.units_at(hex).len(), MAX_STACK);

        // One leaves and there is room again.
        m.move_unit(ids[0], Hex::new(3, 4)).unwrap();
        m.move_unit(ninth, hex).unwrap();
        assert_eq!(m.units_at(hex).last(), Some(&ninth));
    }

    #[test]
    fn killing_units() {
        let mut m = map();
        let hex = Hex::new(4, 4);
        let a = add_unit(&mut m, "A");
        let b = add_unit(&mut m, "B");
        let c = add_unit(&mut m, "C");
        for id in [a, b, c] {
            m.place_unit(id, hex).unwrap();
        }
        assert_eq!(m.kill_unit(b).map(|u| u.name), Some("B".to_string()));
        assert_eq!(m.units_at(hex), [a, c]);
        assert!(m.units().get(b).is_none());
        assert!(m.kill_unit(b).is_none());
        // The freed slot is reused by a unit with a new id, which is not on the map.
        let d = add_unit(&mut m, "D");
        assert_eq!((d.index, d.generation), (b.index, b.generation + 1));
        assert_eq!(m.units_at(hex), [a, c]);
        // The dead unit's id does nothing to the new one.
        assert!(m.kill_unit(b).is_none());
        assert_eq!(m.move_unit(b, Hex::new(1, 1)), Err(PlaceError::NoSuchUnit));
        assert_eq!(m.place_unit(b, Hex::new(1, 1)), Err(PlaceError::NoSuchUnit));
        assert!(m.unit_mut(b).is_none());
        assert_eq!(m.units().get(d).unwrap().name, "D");
        m.place_unit(d, Hex::new(9, 9)).unwrap();
        assert_eq!(m.units_at(Hex::new(9, 9)), [d]);

        assert_eq!(m.kill_tile(hex), 2);
        assert!(m.units_at(hex).is_empty());
        assert!(m.units().get(a).is_none() && m.units().get(c).is_none());
        assert_eq!(m.kill_tile(hex), 0);
        assert_eq!(m.kill_tile(Hex::new(99, 99)), 0);
        assert_eq!(m.units().iter().count(), 1);
        // Every unit the map still knows is on the tile it says it is on.
        for unit in m.units().iter() {
            assert!(m.units_at(unit.pos()).contains(&unit.id()));
        }
    }

    #[test]
    fn a_city_changes_its_tile() {
        let mut m = map();
        let hex = Hex::new(2, 3);
        let id = m.add_city("Alder", hex, 1).unwrap();
        assert_eq!(m.tile(hex).unwrap().kind, TileType::City);
        assert_eq!(m.city_at(hex).map(|c| c.id), Some(id));
        assert!(m.city_at(Hex::new(2, 4)).is_none());
        assert!(m.add_city("Nowhere", Hex::new(50, 50), 0).is_none());
        // One city to a hex.
        assert!(m.add_city("Again", hex, 3).is_none());
        assert_eq!(m.cities().len(), 1);
        let prod = UnitProd { basename: "Militia".into(), time: 1, cost: 4, upkeep: 2, strength: 1, movement: 8 };
        assert!(m.add_city_prod(id, prod.clone()));
        assert_eq!(m.city_at(hex).unwrap().units, [prod]);
    }

    #[test]
    fn revisions_count_changes() {
        let mut m = map();
        let (r0, t0) = (m.revision(), m.terrain_revision());
        // Looking changes nothing, and neither does the cursor.
        let _ = (m.tile(Hex::new(1, 1)), m.units_at(Hex::new(1, 1)), m.to_text());
        m.cursor = Hex::new(3, 3);
        assert_eq!((m.revision(), m.terrain_revision()), (r0, t0));

        // Units change the map but not its terrain.
        let a = add_unit(&mut m, "A");
        m.place_unit(a, Hex::new(5, 5)).unwrap();
        let r1 = m.revision();
        assert!(r1 > r0);
        m.move_unit(a, Hex::new(5, 6)).unwrap();
        let r2 = m.revision();
        assert!(r2 > r1);
        m.unit_mut(a).unwrap().movement = 3;
        let r3 = m.revision();
        assert!(r3 > r2);
        m.reset_move_points(Side::Black);
        let r4 = m.revision();
        assert!(r4 > r3);
        m.kill_unit(a);
        let r5 = m.revision();
        assert!(r5 > r4);
        assert_eq!(m.terrain_revision(), t0);
        // What fails changes nothing.
        assert!(m.move_unit(a, Hex::new(1, 1)).is_err() && m.unit_mut(a).is_none() && m.kill_unit(a).is_none());
        assert_eq!(m.kill_tile(Hex::new(5, 6)), 0);
        assert!(m.tile_mut(Hex::new(99, 99)).is_none());
        assert_eq!((m.revision(), m.terrain_revision()), (r5, t0));

        // Terrain changes both.
        m.tile_mut(Hex::new(2, 2)).unwrap().kind = TileType::Water;
        assert!(m.revision() > r5 && m.terrain_revision() > t0);
        let t1 = m.terrain_revision();
        m.add_city("Alder", Hex::new(4, 4), 0).unwrap();
        assert!(m.terrain_revision() > t1);
    }
}
