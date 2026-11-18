//! Sides, units and the shared movement cost tables.
//!
//! C++: src/state/unit.h, src/state/unit.cpp (Side, MoveCost, Unit, UnitManager)

use super::hex::Hex;
use super::terrain::MoveCost;

/// Who owns a unit or a city. The first eight are players; grey is neutral.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub enum Side {
    Black,
    White,
    Red,
    Green,
    Blue,
    Orange,
    Yellow,
    Purple,
    /// Neutral: no side.
    Grey,
}

/// The number of player sides (grey, the neutral side, is not one of them).
pub const NUM_SIDES: usize = 8;

impl Side {
    /// The player sides, in order.
    pub const PLAYERS: [Side; NUM_SIDES] =
        [Side::Black, Side::White, Side::Red, Side::Green, Side::Blue, Side::Orange, Side::Yellow, Side::Purple];

    /// 0 for black up to 8 for grey.
    pub const fn index(self) -> usize {
        self as usize
    }
}

/// Names a unit: its side and its slot in that side's list. A slot is reused after
/// its unit is deleted, so an id held across a deletion may name a different unit.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct UnitId {
    pub side: Side,
    pub index: u32,
}

/// Names a movement cost table in [`Units`].
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct MoveCostId(pub u32);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Unit {
    pub id: UnitId,
    pub name: String,
    pub upkeep: u32,
    pub strength: u32,
    /// Movement points left this turn.
    pub movement: u32,
    pub move_cost: MoveCostId,
    /// Movement points at the start of a turn, before any carry-over.
    pub base_move_points: u32,
    /// Where the unit is. Only meaningful once the map has placed it.
    pub pos: Hex,
}

impl Unit {
    pub fn side(&self) -> Side {
        self.id.side
    }
}

/// Every unit in the game, by side, and the movement cost tables they share.
///
/// Deviation D12: the C++ kept lists for the eight player sides only, so a grey
/// unit indexed past the end of its array. Here grey has a list too.
#[derive(Clone, Debug, Default)]
pub struct Units {
    per_side: [Vec<Option<Unit>>; NUM_SIDES + 1],
    move_costs: Vec<MoveCost>,
}

impl Units {
    pub fn new() -> Units {
        Units::default()
    }

    /// Create a unit in the first free slot of its side. It starts with full
    /// movement and is not on the map yet. C++: UnitManager::newUnit
    pub fn new_unit(
        &mut self,
        side: Side,
        name: &str,
        upkeep: u32,
        strength: u32,
        movement: u32,
        move_cost: MoveCostId,
    ) -> UnitId {
        let list = &mut self.per_side[side.index()];
        let index = match list.iter().position(Option::is_none) {
            Some(i) => i,
            None => {
                list.push(None);
                list.len() - 1
            }
        };
        let id = UnitId { side, index: index as u32 };
        list[index] = Some(Unit {
            id,
            name: name.to_string(),
            upkeep,
            strength,
            movement,
            move_cost,
            base_move_points: movement,
            pos: Hex::default(),
        });
        id
    }

    /// Remove a unit, freeing its slot for reuse. Returns it if it existed.
    /// C++: UnitManager::deleteUnit
    pub fn delete(&mut self, id: UnitId) -> Option<Unit> {
        self.per_side[id.side.index()].get_mut(id.index as usize)?.take()
    }

    pub fn get(&self, id: UnitId) -> Option<&Unit> {
        self.per_side[id.side.index()].get(id.index as usize)?.as_ref()
    }

    pub fn get_mut(&mut self, id: UnitId) -> Option<&mut Unit> {
        self.per_side[id.side.index()].get_mut(id.index as usize)?.as_mut()
    }

    /// The units of one side.
    pub fn of_side(&self, side: Side) -> impl Iterator<Item = &Unit> {
        self.per_side[side.index()].iter().flatten()
    }

    /// Every unit.
    pub fn iter(&self) -> impl Iterator<Item = &Unit> {
        self.per_side.iter().flat_map(|list| list.iter().flatten())
    }

    /// The id of the table with these costs, adding it if no unit kind uses it
    /// yet. C++: UnitManager::aquireMoveCost
    pub fn acquire_move_cost(&mut self, cost: MoveCost) -> MoveCostId {
        let index = match self.move_costs.iter().position(|c| *c == cost) {
            Some(i) => i,
            None => {
                self.move_costs.push(cost);
                self.move_costs.len() - 1
            }
        };
        MoveCostId(index as u32)
    }

    /// C++: UnitManager::getMoveCost
    pub fn move_cost(&self, id: MoveCostId) -> Option<&MoveCost> {
        self.move_costs.get(id.0 as usize)
    }

    /// Start a side's turn: every unit gets its base movement plus what it carries
    /// over from the last turn, which is 0 if it had none left, 1 if it had 1 left
    /// and 2 otherwise. C++: UnitManager::resetMovePoints (which dereferenced empty
    /// slots; deviation D7)
    pub fn reset_move_points(&mut self, side: Side) {
        for u in self.per_side[side.index()].iter_mut().flatten() {
            u.movement = u.base_move_points + u.movement.min(2);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cav_costs() -> MoveCost {
        MoveCost {
            grass: 2,
            forest: 4,
            swamp: 5,
            desert: 3,
            hill: 4,
            bridge: 1,
            road: 2,
            city: 1,
            ..MoveCost::default()
        }
    }

    #[test]
    fn new_units_fill_slots_in_order() {
        let mut units = Units::new();
        let mc = units.acquire_move_cost(cav_costs());
        let a = units.new_unit(Side::Black, "A", 1, 2, 10, mc);
        let b = units.new_unit(Side::Black, "B", 1, 2, 10, mc);
        let c = units.new_unit(Side::Red, "C", 1, 2, 10, mc);
        assert_eq!(a, UnitId { side: Side::Black, index: 0 });
        assert_eq!(b, UnitId { side: Side::Black, index: 1 });
        assert_eq!(c, UnitId { side: Side::Red, index: 0 });
        let u = units.get(b).unwrap();
        assert_eq!((u.name.as_str(), u.side(), u.movement, u.base_move_points), ("B", Side::Black, 10, 10));
        assert_eq!(units.iter().count(), 3);
        assert_eq!(units.of_side(Side::Black).count(), 2);
    }

    #[test]
    fn a_slot_is_reused_after_a_kill() {
        let mut units = Units::new();
        let mc = units.acquire_move_cost(cav_costs());
        let a = units.new_unit(Side::Black, "A", 1, 2, 10, mc);
        let b = units.new_unit(Side::Black, "B", 1, 2, 10, mc);
        let c = units.new_unit(Side::Black, "C", 1, 2, 10, mc);
        assert_eq!(units.delete(b).map(|u| u.name), Some("B".to_string()));
        assert!(units.get(b).is_none());
        assert!(units.delete(b).is_none());
        // The hole is filled before the list grows.
        let d = units.new_unit(Side::Black, "D", 1, 2, 10, mc);
        assert_eq!(d, b);
        assert_eq!(units.get(d).unwrap().name, "D");
        let e = units.new_unit(Side::Black, "E", 1, 2, 10, mc);
        assert_eq!(e.index, 3);
        assert_eq!(units.get(a).unwrap().name, "A");
        assert_eq!(units.get(c).unwrap().name, "C");
    }

    #[test]
    fn unknown_ids_are_none() {
        let mut units = Units::new();
        let ghost = UnitId { side: Side::Blue, index: 7 };
        assert!(units.get(ghost).is_none());
        assert!(units.get_mut(ghost).is_none());
        assert!(units.delete(ghost).is_none());
    }

    #[test]
    fn the_neutral_side_can_have_units() {
        let mut units = Units::new();
        let mc = units.acquire_move_cost(cav_costs());
        let g = units.new_unit(Side::Grey, "Garrison", 0, 1, 0, mc);
        assert_eq!(units.get(g).unwrap().side(), Side::Grey);
        assert_eq!(Side::Grey.index(), NUM_SIDES);
        assert!(!Side::PLAYERS.contains(&Side::Grey));
    }

    #[test]
    fn move_costs_are_interned() {
        let mut units = Units::new();
        let a = units.acquire_move_cost(cav_costs());
        let b = units.acquire_move_cost(cav_costs());
        assert_eq!(a, b);
        let other = units.acquire_move_cost(MoveCost { grass: 3, ..cav_costs() });
        assert_ne!(a, other);
        assert_eq!(units.acquire_move_cost(MoveCost { grass: 3, ..cav_costs() }), other);
        assert_eq!(units.move_cost(a), Some(&cav_costs()));
        assert_eq!(units.move_cost(other).unwrap().grass, 3);
        assert!(units.move_cost(MoveCostId(99)).is_none());
    }

    #[test]
    fn reset_move_points_carries_over_at_most_two() {
        let mut units = Units::new();
        let mc = units.acquire_move_cost(cav_costs());
        let ids: Vec<UnitId> = (0..4).map(|i| units.new_unit(Side::Black, &format!("U{i}"), 1, 1, 20, mc)).collect();
        let other = units.new_unit(Side::White, "W", 1, 1, 20, mc);
        for (id, left) in ids.iter().zip([0, 1, 2, 14]) {
            units.get_mut(*id).unwrap().movement = left;
        }
        units.get_mut(other).unwrap().movement = 5;
        // An empty slot in the list must not matter (the C++ crashed on one).
        units.delete(ids[1]);
        let refill = units.new_unit(Side::Black, "U1", 1, 1, 20, mc);
        units.get_mut(refill).unwrap().movement = 1;
        let gone = units.new_unit(Side::Black, "gone", 1, 1, 20, mc);
        units.delete(gone);

        units.reset_move_points(Side::Black);
        let got: Vec<u32> = ids.iter().map(|id| units.get(*id).unwrap().movement).collect();
        assert_eq!(got, [20, 21, 22, 22]);
        // Other sides are untouched.
        assert_eq!(units.get(other).unwrap().movement, 5);
    }
}
