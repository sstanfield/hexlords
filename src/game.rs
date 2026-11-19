//! The game the player sees: the map, the selected unit and where it can go.
//!
//! C++: src/Application.cpp Application::Application (the demo map),
//! src/render/maprender.cpp MapRenderer::selectHex, moveUp, moveDown, and the
//! cursor keys of src/gui/mapwidget.cpp MapWidget::OnEvent

use std::path::{Path, PathBuf};

use crate::state::scenario::{self, Scenario};
use crate::state::{Direction, Hex, Map, MoveCost, MoveRange, Side, TileType, UnitId, compute_range};

/// What `Game::select_hex` did.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SelectOutcome {
    /// Nothing was selected and the hex had a unit: it is selected now.
    Selected(UnitId),
    /// The selected unit's own hex was chosen: nothing is selected now.
    Deselected,
    /// The selected unit moved to the hex, paid `cost` and was deselected.
    Moved { unit: UnitId, cost: u32 },
    /// Nothing changed: no unit here to select, or the selected unit cannot go here.
    Nothing,
}

pub struct Game {
    pub map: Map,
    selected: Option<UnitId>,
    range: Option<MoveRange>,
    /// The map's revision when `range` was computed.
    range_revision: u64,
    /// The map's own asset directory, if it brought one: a picture found
    /// there is drawn instead of the game's of the same path.
    pub assets: Option<PathBuf>,
    turn: u32,
}

impl Game {
    pub fn new(map: Map) -> Game {
        Game { map, selected: None, range: None, range_revision: 0, assets: None, turn: 1 }
    }

    /// The game on a map that was read from its files, with its cities and
    /// its units. The cursor starts on the first city that has a garrison (or
    /// the first city, or the middle of the map).
    pub fn from_scenario(scenario: Scenario) -> Game {
        let Scenario { mut map, assets, .. } = scenario;
        let start = map
            .cities()
            .iter()
            .find(|c| !map.units_at(c.pos).is_empty())
            .or_else(|| map.cities().iter().next())
            .map(|c| c.pos);
        if let Some(hex) = start {
            map.cursor = hex;
        }
        Game { assets, ..Game::new(map) }
    }

    /// Read a map (a `.map` file or a map directory, `docs/map-format.md`)
    /// and start the game on it. A map that brings no units gets the demo's
    /// one (see [`Game::on_map`]). Also returns what the reader warns of.
    pub fn load(path: &Path) -> Result<(Game, Vec<String>), String> {
        let scenario = scenario::load(path).map_err(|e| e.to_string())?;
        let warnings = scenario.warnings.clone();
        let game = if scenario.map.units().iter().next().is_some() {
            Game::from_scenario(scenario)
        } else {
            let assets = scenario.assets.clone();
            Game { assets, ..Game::on_map(scenario.map) }
        };
        Ok((game, warnings))
    }

    /// The turn that is being played: 1 at the start.
    pub fn turn(&self) -> u32 {
        self.turn
    }

    /// The turn is over: every unit of every side gets its movement points
    /// back for the next (see `Units::reset_move_points`), and nothing is
    /// selected. (There are no rules for whose turn it is yet: this is the
    /// least a turn can be.)
    pub fn end_turn(&mut self) {
        for side in Side::PLAYERS.into_iter().chain([Side::Grey]) {
            self.map.reset_move_points(side);
        }
        self.deselect();
        self.turn += 1;
    }

    /// The demo: a 100 x 100 grass map with a mountain diagonal and one cavalry
    /// unit in the middle (which is on the diagonal, so it stands on a mountain it
    /// could not enter). C++: Application::Application
    pub fn demo() -> Game {
        let mut map = Map::new(100, 100, TileType::Grass).expect("100 x 100 is a valid map size");
        for i in 0..100 {
            if let Some(tile) = map.tile_mut(Hex::new(i, i)) {
                tile.kind = TileType::Mountain;
            }
        }
        add_demo_unit(&mut map, Hex::new(50, 50));
        Game::new(map)
    }

    /// The demo on a map of your own (`--map`): the same cavalry unit, on the
    /// hex nearest the middle of the map that it can stand on, with the cursor
    /// on it. A map that is all water and mountains gets no unit.
    pub fn on_map(mut map: Map) -> Game {
        let middle = Hex::new(map.rows() / 2, map.cols() / 2);
        let costs = demo_costs();
        let can_stand = |map: &Map, hex: Hex| map.tile(hex).is_some_and(|t| costs.entry(t.kind).is_some());
        // Breadth first from the middle: the first hex found is a nearest one.
        let mut seen = vec![false; usize::from(map.rows()) * usize::from(map.cols())];
        let index = |hex: Hex| usize::from(hex.row) * usize::from(map.cols()) + usize::from(hex.col);
        let mut queue = std::collections::VecDeque::from([middle]);
        seen[index(middle)] = true;
        let mut start = None;
        while let Some(hex) = queue.pop_front() {
            if can_stand(&map, hex) {
                start = Some(hex);
                break;
            }
            for next in map.neighbours(hex) {
                if !std::mem::replace(&mut seen[index(next)], true) {
                    queue.push_back(next);
                }
            }
        }
        if let Some(hex) = start {
            add_demo_unit(&mut map, hex);
            map.cursor = hex;
        }
        Game::new(map)
    }

    pub fn selected(&self) -> Option<UnitId> {
        self.selected
    }

    /// Where the selected unit can go.
    pub fn range(&self) -> Option<&MoveRange> {
        self.range.as_ref()
    }

    /// True if a unit is selected and it can move to `hex` with the movement it has
    /// left. These are the hexes the map marks.
    pub fn can_move_to(&self, hex: Hex) -> bool {
        self.range.as_ref().is_some_and(|r| r.cost(hex).is_some())
    }

    pub fn deselect(&mut self) {
        self.selected = None;
        self.range = None;
    }

    /// Select one unit by name (a row of a stack was chosen): it is the one a
    /// chosen hex will move. False, and nothing changes, if there is no such
    /// unit any more.
    pub fn select_unit(&mut self, id: UnitId) -> bool {
        self.select(id)
    }

    fn select(&mut self, id: UnitId) -> bool {
        let Some(unit) = self.map.units().get(id) else { return false };
        self.range = Some(compute_range(&self.map, unit));
        self.range_revision = self.map.revision();
        self.selected = Some(id);
        true
    }

    /// Bring the selection up to date with the map, which is public and may have
    /// changed since the unit was selected: a unit that is gone is no longer
    /// selected, and a range computed from an older map is computed again.
    /// `select_hex` does this itself; whoever draws the range calls it first.
    /// Returns true if anything changed.
    pub fn refresh(&mut self) -> bool {
        let Some(id) = self.selected else { return false };
        if self.map.units().get(id).is_none() {
            self.deselect();
            return true;
        }
        if self.range.is_some() && self.range_revision == self.map.revision() {
            return false;
        }
        self.select(id)
    }

    /// The player chose a hex (a click, or Enter at the cursor).
    ///
    /// - A unit is selected and this is its hex: deselect it.
    /// - A unit is selected and it can reach the hex: it moves there, pays the
    ///   cost and is deselected.
    /// - Nothing is selected and the hex has units: select the first.
    ///
    /// C++: MapRenderer::selectHex. There a selected unit could go to any hex that
    /// the flood fill had not reached, mountains included, for free; here it stays
    /// selected and nothing happens.
    pub fn select_hex(&mut self, hex: Hex) -> SelectOutcome {
        self.refresh();
        let Some(id) = self.selected else {
            return match self.map.units_at(hex).first().copied() {
                Some(first) if self.select(first) => SelectOutcome::Selected(first),
                _ => SelectOutcome::Nothing,
            };
        };
        let Some(unit) = self.map.units().get(id) else { return SelectOutcome::Nothing };
        if unit.pos() == hex {
            self.deselect();
            return SelectOutcome::Deselected;
        }
        let Some(cost) = self.range.as_ref().and_then(|r| r.cost(hex)) else { return SelectOutcome::Nothing };
        if cost > unit.movement || self.map.move_unit(id, hex).is_err() {
            return SelectOutcome::Nothing; // a full stack, for one
        }
        if let Some(unit) = self.map.unit_mut(id) {
            unit.movement -= cost;
        }
        self.deselect();
        SelectOutcome::Moved { unit: id, cost }
    }

    /// Move the cursor one hex, stopping at the map's edges. Returns true if it
    /// moved. C++: MapWidget::OnEvent (left, right), MapRenderer::moveUp, moveDown
    pub fn move_cursor(&mut self, dir: Direction) -> bool {
        let Hex { row, col } = self.map.cursor;
        let to = match dir {
            Direction::Left if col > 0 => Hex::new(row, col - 1),
            Direction::Right if col + 1 < self.map.cols() => Hex::new(row, col + 1),
            Direction::Up if row > 0 => Hex::new(row - 1, col),
            Direction::Down if row + 1 < self.map.rows() => Hex::new(row + 1, col),
            _ => return false,
        };
        self.map.cursor = to;
        true
    }

    /// Put the cursor on `hex`, if it is on the map.
    pub fn set_cursor(&mut self, hex: Hex) -> bool {
        let on_map = self.map.contains(hex);
        if on_map {
            self.map.cursor = hex;
        }
        on_map
    }
}

/// A hex's terrain in words, with its road: "grass, road", "water, bridge",
/// "mountain, pass".
pub fn describe_terrain(kind: TileType, road: bool) -> String {
    let road = match (road, kind) {
        (false, _) => "",
        (true, TileType::Water) => ", bridge",
        (true, TileType::Mountain) => ", pass",
        (true, _) => ", road",
    };
    format!("{}{road}", kind.name())
}

/// What the demo's cavalry pays to enter each terrain. C++: Application::Application
fn demo_costs() -> MoveCost {
    MoveCost {
        grass: 2,
        forest: 4,
        swamp: 5,
        desert: 3,
        hill: 4,
        mountain: 0,
        water: 0,
        shore: 0,
        bridge: 1,
        road: 2,
        city: 1,
    }
}

/// C++: Application::Application
fn add_demo_unit(map: &mut Map, hex: Hex) {
    let cost = map.acquire_move_cost(demo_costs());
    let cav = map.new_unit(Side::Black, "Test Cav", 3, 5, 20, cost);
    map.place_unit(cav, hex).expect("a hex of a new map is free");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::MAX_STACK;

    const START: Hex = Hex::new(50, 50);

    #[test]
    fn a_map_of_your_own_gets_the_demo_unit_near_its_middle() {
        // Grass: the unit is in the middle, and the cursor on it.
        let game = Game::on_map(Map::new(30, 50, TileType::Grass).unwrap());
        let middle = Hex::new(15, 25);
        assert_eq!(game.map.cursor, middle);
        let &[id] = game.map.units_at(middle) else { panic!("one unit in the middle") };
        assert_eq!(game.map.units().get(id).unwrap().name, "Test Cav");

        // A lake in the middle: the unit stands on its shore, as near as can be.
        let mut map = Map::new(30, 50, TileType::Grass).unwrap();
        let mut lake = vec![middle];
        for _ in 0..3 {
            for hex in lake.clone() {
                lake.extend(map.neighbours(hex));
            }
        }
        for hex in &lake {
            map.tile_mut(*hex).unwrap().kind = TileType::Water;
        }
        let game = Game::on_map(map);
        let start = game.map.cursor;
        assert_eq!(game.map.units_at(start).len(), 1);
        assert_eq!(game.map.tile(start).unwrap().kind, TileType::Grass);
        assert!(game.map.neighbours(start).any(|n| lake.contains(&n)), "{start} is on the shore");
        assert_eq!(game.map.units().iter().count(), 1);

        // Nowhere to stand: no unit, and the cursor stays in the middle.
        let game = Game::on_map(Map::new(20, 20, TileType::Water).unwrap());
        assert_eq!(game.map.units().iter().count(), 0);
        assert_eq!(game.map.cursor, Hex::new(10, 10));
    }

    #[test]
    fn the_demo_state() {
        let game = Game::demo();
        let map = &game.map;
        assert_eq!((map.rows(), map.cols()), (100, 100));
        assert_eq!(map.cursor, START);
        for i in 0..100u16 {
            assert_eq!(map.tile(Hex::new(i, i)).unwrap().kind, TileType::Mountain);
        }
        assert_eq!(map.tile(Hex::new(50, 51)).unwrap().kind, TileType::Grass);
        assert_eq!(map.to_text().matches('m').count(), 100);
        assert_eq!(map.to_text().matches('g').count(), 9900);

        assert_eq!(map.units().iter().count(), 1);
        let &[id] = map.units_at(START) else { panic!("one unit in the middle") };
        let cav = map.units().get(id).unwrap();
        assert_eq!((cav.side(), cav.name.as_str()), (Side::Black, "Test Cav"));
        assert_eq!((cav.upkeep, cav.strength, cav.movement, cav.base_move_points), (3, 5, 20, 20));
        let costs = map.units().move_cost(cav.move_cost).unwrap();
        assert_eq!(costs.entry(TileType::Grass), Some(2));
        assert_eq!(costs.entry(TileType::Mountain), None);
        assert!(game.selected().is_none() && game.range().is_none());
    }

    /// The milestone 7 flow: select the unit, move it three grass hexes east, check
    /// what it has left, and that it ends deselected.
    #[test]
    fn select_move_deselect() {
        let mut game = Game::demo();
        let &[id] = game.map.units_at(START) else { panic!() };
        assert_eq!(game.select_hex(START), SelectOutcome::Selected(id));
        assert_eq!(game.selected(), Some(id));
        // The range works from the mountain the unit starts on.
        assert!(game.can_move_to(Hex::new(50, 51)));
        assert!(!game.can_move_to(START));
        assert!(!game.can_move_to(Hex::new(51, 51)), "a mountain");
        assert_eq!(game.range().unwrap().cost(Hex::new(50, 53)), Some(6));

        let target = Hex::new(50, 53);
        assert_eq!(game.select_hex(target), SelectOutcome::Moved { unit: id, cost: 6 });
        assert_eq!(game.map.units().get(id).unwrap().movement, 14);
        assert_eq!(game.map.units().get(id).unwrap().pos(), target);
        assert_eq!(game.map.units_at(target), [id]);
        assert!(game.map.units_at(START).is_empty());
        assert!(game.selected().is_none() && game.range().is_none());
        assert!(!game.can_move_to(Hex::new(50, 54)));

        // Selecting again uses what is left; choosing its own hex deselects.
        assert_eq!(game.select_hex(target), SelectOutcome::Selected(id));
        assert!(game.range().unwrap().reachable().all(|(_, c)| c <= 14));
        assert_eq!(game.select_hex(target), SelectOutcome::Deselected);
        assert!(game.selected().is_none());
        assert_eq!(game.map.units().get(id).unwrap().movement, 14);
    }

    #[test]
    fn a_unit_cannot_go_where_it_cannot_reach() {
        let mut game = Game::demo();
        let &[id] = game.map.units_at(START) else { panic!() };
        game.select_hex(START);
        // The C++ teleported to all three of these for free (deviation D2).
        for hex in [Hex::new(0, 0), Hex::new(51, 51), Hex::new(50, 70)] {
            assert_eq!(game.select_hex(hex), SelectOutcome::Nothing, "{hex}");
            assert_eq!(game.selected(), Some(id), "still selected");
            assert_eq!(game.map.units().get(id).unwrap().pos(), START);
            assert_eq!(game.map.units().get(id).unwrap().movement, 20);
        }
        // Off the map.
        assert_eq!(game.select_hex(Hex::new(100, 100)), SelectOutcome::Nothing);
    }

    #[test]
    fn the_range_is_everything_within_twenty_points() {
        let mut game = Game::demo();
        game.select_hex(START);
        let range = game.range().unwrap();
        // Ten grass steps in a straight line, and no further.
        assert_eq!(range.cost(Hex::new(40, 50)), Some(20));
        assert_eq!(range.cost(Hex::new(39, 50)), None);
        assert!(
            range
                .reachable()
                .all(|(h, c)| { c <= 20 && c % 2 == 0 && game.map.tile(h).unwrap().kind == TileType::Grass })
        );
        assert!(range.reachable().count() > 200);
    }

    #[test]
    fn empty_hexes_select_nothing() {
        let mut game = Game::demo();
        assert_eq!(game.select_hex(Hex::new(10, 10)), SelectOutcome::Nothing);
        assert!(game.selected().is_none());
    }

    #[test]
    fn a_full_stack_refuses_the_move_and_keeps_the_selection() {
        let mut game = Game::demo();
        let &[cav] = game.map.units_at(START) else { panic!() };
        let cost = game.map.units().get(cav).unwrap().move_cost;
        let target = Hex::new(50, 51);
        for i in 0..MAX_STACK {
            let id = game.map.new_unit(Side::Black, &format!("Guard {i}"), 1, 1, 0, cost);
            game.map.place_unit(id, target).unwrap();
        }
        game.select_hex(START);
        assert_eq!(game.select_hex(target), SelectOutcome::Nothing);
        assert_eq!(game.selected(), Some(cav));
        assert_eq!(game.map.units().get(cav).unwrap().movement, 20);
        assert_eq!(game.map.units().get(cav).unwrap().pos(), START);
    }

    #[test]
    fn the_first_unit_of_a_stack_is_selected() {
        let mut game = Game::demo();
        let &[cav] = game.map.units_at(START) else { panic!() };
        let cost = game.map.units().get(cav).unwrap().move_cost;
        let second = game.map.new_unit(Side::Black, "Second", 1, 1, 8, cost);
        game.map.place_unit(second, START).unwrap();
        assert_eq!(game.select_hex(START), SelectOutcome::Selected(cav));
        game.deselect();
        // With the first one gone, the next is first (the C++ crashed on the hole
        // it left; deviation D4).
        game.map.kill_unit(cav);
        assert_eq!(game.select_hex(START), SelectOutcome::Selected(second));
    }

    #[test]
    fn a_selected_unit_that_dies_is_forgotten() {
        let mut game = Game::demo();
        let &[cav] = game.map.units_at(START) else { panic!() };
        game.select_hex(START);
        game.map.kill_unit(cav);
        assert_eq!(game.select_hex(Hex::new(50, 51)), SelectOutcome::Nothing);
        assert!(game.selected().is_none() && game.range().is_none());
    }

    #[test]
    fn a_new_unit_in_a_dead_units_slot_does_not_inherit_the_selection() {
        let mut game = Game::demo();
        let &[cav] = game.map.units_at(START) else { panic!() };
        let cost = game.map.units().get(cav).unwrap().move_cost;
        game.select_hex(START);
        // The selected unit dies and another takes its slot, far away.
        game.map.kill_unit(cav);
        let other = game.map.new_unit(Side::Black, "Other", 1, 1, 20, cost);
        assert_eq!((other.side, other.index), (cav.side, cav.index));
        let far = Hex::new(10, 10);
        game.map.place_unit(other, far).unwrap();
        // A hex the dead unit could have reached: the new one must not jump there.
        assert_eq!(game.select_hex(Hex::new(50, 53)), SelectOutcome::Nothing);
        assert!(game.selected().is_none());
        assert_eq!(game.map.units().get(other).unwrap().pos(), far);
        assert_eq!(game.map.units().get(other).unwrap().movement, 20);
        assert_eq!(game.map.units_at(far), [other]);
        assert!(game.map.units_at(Hex::new(50, 53)).is_empty());
    }

    #[test]
    fn the_range_follows_changes_made_behind_the_games_back() {
        let mut game = Game::demo();
        let &[cav] = game.map.units_at(START) else { panic!() };
        game.select_hex(START);
        assert!(game.can_move_to(Hex::new(50, 51)));
        assert!(!game.refresh(), "nothing changed yet");

        // The unit is moved by something else: the range is from where it is now.
        let there = Hex::new(20, 20 + 1);
        game.map.move_unit(cav, there).unwrap();
        assert!(game.refresh());
        assert_eq!(game.selected(), Some(cav));
        assert_eq!(game.range().unwrap().origin(), there);
        assert!(!game.can_move_to(Hex::new(50, 51)));
        assert!(game.can_move_to(Hex::new(20, 22)));

        // The terrain changes: water next door is out of reach at once.
        let next_door = Hex::new(20, 22);
        game.map.tile_mut(next_door).unwrap().kind = TileType::Water;
        assert_eq!(game.select_hex(next_door), SelectOutcome::Nothing, "select_hex refreshes by itself");
        assert!(!game.can_move_to(next_door));

        // The unit has fewer points: the range shrinks.
        game.map.unit_mut(cav).unwrap().movement = 2;
        assert!(game.refresh());
        assert!(game.range().unwrap().reachable().all(|(_, c)| c == 2));
        assert!(!game.refresh());

        // The unit dies: refresh forgets it.
        game.map.kill_unit(cav);
        assert!(game.refresh());
        assert!(game.selected().is_none() && game.range().is_none());
        assert!(!game.refresh());
    }

    #[test]
    fn the_cursor_stops_at_the_edges() {
        let mut game = Game::demo();
        assert!(game.move_cursor(Direction::Left));
        assert!(game.move_cursor(Direction::Up));
        assert_eq!(game.map.cursor, Hex::new(49, 49));
        assert!(game.move_cursor(Direction::Right));
        assert!(game.move_cursor(Direction::Down));
        assert_eq!(game.map.cursor, START);

        assert!(game.set_cursor(Hex::new(0, 0)));
        assert!(!game.move_cursor(Direction::Left));
        assert!(!game.move_cursor(Direction::Up));
        assert_eq!(game.map.cursor, Hex::new(0, 0));
        assert!(game.set_cursor(Hex::new(99, 99)));
        assert!(!game.move_cursor(Direction::Right));
        assert!(!game.move_cursor(Direction::Down));
        assert_eq!(game.map.cursor, Hex::new(99, 99));
        assert!(!game.set_cursor(Hex::new(100, 0)));
        assert_eq!(game.map.cursor, Hex::new(99, 99));
    }

    /// A map with a city, a road out of it and two units in it.
    fn scenario() -> Scenario {
        let mut rows = vec!["g".repeat(20); 20];
        rows[5] = "gggggcGGWMgggggggggg".to_string();
        let terrain = format!("20\n20\n{}\n", rows.join("\n"));
        let units = "unit riders\nname Riders\nmoves 10\nenter grass 2, city 1, road 1, bridge 1\n";
        let cities = "city 5 5\nname Wrenfold\nside blue\nwalls 1\nproduce riders 3\ngarrison riders 2\n";
        let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
        Scenario { map, assets: None, warnings }
    }

    #[test]
    fn a_map_with_its_own_units_starts_at_its_first_garrison() {
        let mut game = Game::from_scenario(scenario());
        let city = Hex::new(5, 5);
        assert_eq!(game.map.cursor, city);
        assert_eq!((game.turn(), game.selected()), (1, None));
        assert_eq!(game.map.units_at(city).len(), 2);

        // A rider goes down the road and over the bridge: 1 a step.
        assert!(matches!(game.select_hex(city), SelectOutcome::Selected(_)));
        assert!(game.can_move_to(Hex::new(5, 8)));
        assert_eq!(game.range().unwrap().cost(Hex::new(5, 8)), Some(3));
        assert_eq!(game.range().unwrap().cost(Hex::new(5, 9)), Some(4), "the pass");
        assert!(matches!(game.select_hex(Hex::new(5, 9)), SelectOutcome::Moved { cost: 4, .. }));
        let rider = game.map.units_at(Hex::new(5, 9))[0];
        assert_eq!(game.map.units().get(rider).unwrap().movement, 6);

        // The turn ends: it has its points back (and the two it may carry over).
        game.select_hex(Hex::new(5, 9));
        game.end_turn();
        assert_eq!((game.turn(), game.selected()), (2, None));
        assert_eq!(game.map.units().get(rider).unwrap().movement, 12);
        let other = game.map.units_at(city)[0];
        assert_eq!(game.map.units().get(other).unwrap().movement, 12);
    }

    #[test]
    fn terrain_in_words() {
        assert_eq!(describe_terrain(TileType::Grass, false), "grass");
        assert_eq!(describe_terrain(TileType::Forest, true), "forest, road");
        assert_eq!(describe_terrain(TileType::Water, true), "water, bridge");
        assert_eq!(describe_terrain(TileType::Mountain, true), "mountain, pass");
        assert_eq!(describe_terrain(TileType::Water, false), "water");
    }

    #[test]
    fn a_map_without_units_gets_the_demos() {
        let dir = std::env::temp_dir().join(format!("hexgame-game-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("bare/assets")).unwrap();
        let terrain = format!("20\n20\n{}", "gggggggggggggggggggg\n".repeat(20));
        std::fs::write(dir.join("bare/terrain.map"), &terrain).unwrap();
        std::fs::write(dir.join("plain.map"), terrain.replacen("gg", "cg", 1)).unwrap();

        let (game, warnings) = Game::load(&dir.join("bare")).unwrap();
        assert_eq!(game.map.units().iter().count(), 1);
        assert_eq!(game.assets, Some(dir.join("bare/assets")));
        assert!(warnings.is_empty());
        let (game, warnings) = Game::load(&dir.join("plain.map")).unwrap();
        assert_eq!(game.map.units().iter().count(), 1);
        assert_eq!(warnings, ["the city tile at (0, 0) has no entry in cities.txt".to_string()]);
        assert!(game.assets.is_none());
        let e = Game::load(&dir.join("none.map")).err().unwrap();
        assert!(e.contains("none.map"), "{e}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
