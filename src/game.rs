//! The game the player sees: the map, the selected unit and where it can go.
//!
//! C++: src/Application.cpp Application::Application (the demo map),
//! src/render/maprender.cpp MapRenderer::selectHex, moveUp, moveDown, and the
//! cursor keys of src/gui/mapwidget.cpp MapWidget::OnEvent

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
}

impl Game {
    pub fn new(map: Map) -> Game {
        Game { map, selected: None, range: None, range_revision: 0 }
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
        let cost = map.acquire_move_cost(MoveCost {
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
        });
        let cav = map.new_unit(Side::Black, "Test Cav", 3, 5, 20, cost);
        map.place_unit(cav, Hex::new(50, 50)).expect("the middle of the map is free");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::MAX_STACK;

    const START: Hex = Hex::new(50, 50);

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
}
