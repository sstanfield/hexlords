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

/// Names a unit: its side, its slot in that side's list, and which of the units
/// that have used that slot it is. A slot is reused after its unit is deleted,
/// but with the next generation, so an id held across a deletion names nothing:
/// every lookup with it gives `None`, never the slot's new unit.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct UnitId {
    pub side: Side,
    pub index: u32,
    pub generation: u32,
}

/// Names a movement cost table in [`Units`].
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub struct MoveCostId(pub u32);

/// The picture of a unit whose kind names none, relative to an asset directory.
pub const DEFAULT_UNIT_IMAGE: &str = "units/default_unit.png";

/// How a kind of unit goes about the map: on land (by its prices for each
/// terrain), by sea (a ship: over water, into its side's cities, carrying
/// land units), or through the air (a flyer: wherever its prices let it, and
/// a commander with it).
#[derive(Copy, Clone, Debug, Default, Eq, PartialEq, Hash)]
pub enum Travel {
    #[default]
    Land,
    Sea,
    Air,
}

impl Travel {
    /// Its word in `units.txt` (`travels`).
    pub fn word(self) -> &'static str {
        match self {
            Travel::Land => "land",
            Travel::Sea => "sea",
            Travel::Air => "air",
        }
    }

    /// The travel a word names.
    pub fn named(word: &str) -> Option<Travel> {
        [Travel::Land, Travel::Sea, Travel::Air].into_iter().find(|t| t.word() == word)
    }
}

/// Names a kind of unit in [`Units`].
#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct UnitKindId(pub u32);

/// A kind of unit a map has: what every unit of the kind starts with. A map
/// brings its own (`docs/map-format.md`, `units.txt`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UnitKind {
    /// The word files use for it: lower-case letters, digits and `_`.
    pub id: String,
    /// What the player reads.
    pub name: String,
    /// Its picture, relative to an asset directory.
    pub image: String,
    pub strength: u32,
    /// The movement points it has each turn.
    pub moves: u32,
    pub upkeep: u32,
    /// What it costs a city to build one.
    pub price: u32,
    /// What it pays to enter each terrain.
    pub cost: MoveCost,
    /// A commander: how much stronger it makes the others of its stack in a
    /// battle (0: it is not one). A stack has one, in a place of its own.
    pub commander: u32,
    /// How it goes: on land, by sea, through the air.
    pub travel: Travel,
    /// A ship: how many land units it carries (its stack's commander goes
    /// besides). 0 for the others.
    pub carries: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Unit {
    id: UnitId,
    pub name: String,
    pub upkeep: u32,
    pub strength: u32,
    /// Movement points left this turn.
    pub movement: u32,
    pub move_cost: MoveCostId,
    /// Movement points at the start of a turn, before any carry-over.
    pub base_move_points: u32,
    /// The kind it was made as, if it was made from one.
    pub kind: Option<UnitKindId>,
    pos: Hex,
    /// Its level, 1 to [`MAX_LEVEL`], from the battles it has won
    /// (`experience`) or its training; each above the first makes it 1
    /// stronger (`Unit::power`).
    pub level: u8,
    pub experience: u32,
    /// Its own name, if it has one (a captain).
    pub given_name: Option<String>,
    /// What a temple gave it, once.
    pub blessing: Option<Blessing>,
    /// A standing order: where it goes, a turn at a time (`Order::GoTo`);
    /// and whether its last step was stopped (it waits for its player).
    pub goto: Option<Hex>,
    pub stalled: bool,
    /// This turn: it has been given an order (it moved), or its player is
    /// done with it (`Order::Done`).
    pub moved: bool,
    pub done: bool,
}

/// The highest level, and the battles won a unit needs for each level above
/// the first: a veteran after 2, elite after 5.
pub const MAX_LEVEL: u8 = 3;
pub const LEVEL_EXPERIENCE: [u32; 2] = [2, 5];

/// How much stronger a commander makes the others of its stack: what its
/// kind says (`kind_bonus`, 0 for a unit that is not one), and a point more
/// at the highest level.
pub fn command_bonus(kind_bonus: u32, level: u8) -> u32 {
    if kind_bonus == 0 { 0 } else { kind_bonus + u32::from(level >= MAX_LEVEL) }
}

/// The level that so much experience makes.
pub fn level_for(experience: u32) -> u8 {
    1 + LEVEL_EXPERIENCE.iter().filter(|need| experience >= **need).count() as u8
}

/// What a temple gives a unit, once: a point of strength, or two of
/// movement a turn.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Blessing {
    Strength,
    Moves,
}

impl Blessing {
    pub const STRENGTH: u32 = 1;
    pub const MOVES: u32 = 2;

    /// What it gives, in words: "+1 strength".
    pub fn words(self) -> String {
        match self {
            Blessing::Strength => format!("+{} strength", Blessing::STRENGTH),
            Blessing::Moves => format!("+{} moves", Blessing::MOVES),
        }
    }
}

impl Unit {
    pub fn id(&self) -> UnitId {
        self.id
    }

    pub fn side(&self) -> Side {
        self.id.side
    }

    /// Where the unit is. Only meaningful once the map has placed it; only the
    /// map changes it (`Map::place_unit`, `Map::move_unit`).
    pub fn pos(&self) -> Hex {
        self.pos
    }

    pub(super) fn set_pos(&mut self, pos: Hex) {
        self.pos = pos;
    }

    /// The strength it fights with, before its stack and the ground and the
    /// walls: its own, a point for each level above the first, and a
    /// temple's blessing.
    pub fn power(&self) -> u32 {
        let blessed = if self.blessing == Some(Blessing::Strength) { Blessing::STRENGTH } else { 0 };
        self.strength.saturating_add(u32::from(self.level.saturating_sub(1))).saturating_add(blessed)
    }

    /// What it is called: its own name with its kind's ("Captain Aldric"),
    /// or its kind's.
    pub fn title(&self) -> String {
        match &self.given_name {
            Some(name) => format!("{} {name}", self.name),
            None => self.name.clone(),
        }
    }

    /// Experience from a battle won: the level it brings, if it is a new
    /// one.
    pub fn gain(&mut self, experience: u32) -> Option<u8> {
        self.experience = self.experience.saturating_add(experience);
        let level = level_for(self.experience).max(self.level);
        (level > self.level).then(|| {
            self.level = level;
            level
        })
    }

    /// Start at a level (a city that trains its units), with the experience
    /// it takes.
    pub fn train(&mut self, level: u8) {
        let level = level.clamp(1, MAX_LEVEL);
        if level > self.level {
            self.level = level;
            self.experience = self.experience.max(LEVEL_EXPERIENCE[usize::from(level) - 2]);
        }
    }

    /// Bless it (a temple): false if it has been blessed already.
    pub fn bless(&mut self, blessing: Blessing) -> bool {
        if self.blessing.is_some() {
            return false;
        }
        self.blessing = Some(blessing);
        if blessing == Blessing::Moves {
            self.base_move_points = self.base_move_points.saturating_add(Blessing::MOVES);
            self.movement = self.movement.saturating_add(Blessing::MOVES);
        }
        true
    }
}

/// A place in a side's list: the unit in it, if any, and how many units have
/// been in it before.
#[derive(Clone, Debug, Default)]
struct Slot {
    generation: u32,
    unit: Option<Unit>,
}

/// Every unit in the game, by side, and the movement cost tables they share.
///
/// The C++ kept lists for the eight player sides only, so a grey unit indexed
/// past the end of its array. Here grey has a list too.
#[derive(Clone, Debug, Default)]
pub struct Units {
    per_side: [Vec<Slot>; NUM_SIDES + 1],
    move_costs: Vec<MoveCost>,
    kinds: Vec<UnitKind>,
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
        let index = match list.iter().position(|slot| slot.unit.is_none()) {
            Some(i) => i,
            None => {
                list.push(Slot::default());
                list.len() - 1
            }
        };
        let slot = &mut list[index];
        let id = UnitId { side, index: index as u32, generation: slot.generation };
        slot.unit = Some(Unit {
            id,
            name: name.to_string(),
            upkeep,
            strength,
            movement,
            move_cost,
            base_move_points: movement,
            kind: None,
            pos: Hex::default(),
            level: 1,
            experience: 0,
            given_name: None,
            blessing: None,
            goto: None,
            stalled: false,
            moved: false,
            done: false,
        });
        id
    }

    /// Add a kind of unit. `None` if there is one with that id already.
    pub fn add_kind(&mut self, kind: UnitKind) -> Option<UnitKindId> {
        if self.kind_named(&kind.id).is_some() {
            return None;
        }
        self.kinds.push(kind);
        Some(UnitKindId(self.kinds.len() as u32 - 1))
    }

    pub fn kind(&self, id: UnitKindId) -> Option<&UnitKind> {
        self.kinds.get(id.0 as usize)
    }

    /// The kind files call `id`.
    pub fn kind_named(&self, id: &str) -> Option<UnitKindId> {
        self.kinds.iter().position(|k| k.id == id).map(|i| UnitKindId(i as u32))
    }

    /// Every kind, in the order they were added (a `UnitKindId` is the place).
    pub fn kinds(&self) -> &[UnitKind] {
        &self.kinds
    }

    /// The kind a unit was made as.
    pub fn kind_of(&self, unit: &Unit) -> Option<&UnitKind> {
        self.kind(unit.kind?)
    }

    /// Create a unit of a kind, with the kind's name and numbers. `None` if
    /// there is no such kind.
    pub fn new_unit_of(&mut self, side: Side, kind: UnitKindId) -> Option<UnitId> {
        let k = self.kind(kind)?.clone();
        let cost = self.acquire_move_cost(k.cost);
        let id = self.new_unit(side, &k.name, k.upkeep, k.strength, k.moves, cost);
        if let Some(unit) = self.get_mut(id) {
            unit.kind = Some(kind);
        }
        Some(id)
    }

    fn slot(&self, id: UnitId) -> Option<&Slot> {
        self.per_side[id.side.index()].get(id.index as usize).filter(|slot| slot.generation == id.generation)
    }

    fn slot_mut(&mut self, id: UnitId) -> Option<&mut Slot> {
        self.per_side[id.side.index()].get_mut(id.index as usize).filter(|slot| slot.generation == id.generation)
    }

    /// Remove a unit, freeing its slot for the next unit of its side (which gets
    /// a new id). Returns the unit if it existed. C++: UnitManager::deleteUnit
    pub fn delete(&mut self, id: UnitId) -> Option<Unit> {
        let slot = self.slot_mut(id)?;
        let unit = slot.unit.take()?;
        slot.generation = slot.generation.wrapping_add(1);
        Some(unit)
    }

    pub fn get(&self, id: UnitId) -> Option<&Unit> {
        self.slot(id)?.unit.as_ref()
    }

    pub fn get_mut(&mut self, id: UnitId) -> Option<&mut Unit> {
        self.slot_mut(id)?.unit.as_mut()
    }

    /// The units of one side.
    pub fn of_side(&self, side: Side) -> impl Iterator<Item = &Unit> {
        self.per_side[side.index()].iter().filter_map(|slot| slot.unit.as_ref())
    }

    /// Every unit.
    pub fn iter(&self) -> impl Iterator<Item = &Unit> {
        self.per_side.iter().flat_map(|list| list.iter().filter_map(|slot| slot.unit.as_ref()))
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
    /// slots)
    pub fn reset_move_points(&mut self, side: Side) {
        for u in self.per_side[side.index()].iter_mut().filter_map(|slot| slot.unit.as_mut()) {
            // A map may give a kind any number of moves: the most there is stays the most.
            u.movement = u.base_move_points.saturating_add(u.movement.min(2));
            // A new turn: nothing done yet.
            (u.moved, u.done, u.stalled) = (false, false, false);
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
        assert_eq!(a, UnitId { side: Side::Black, index: 0, generation: 0 });
        assert_eq!(b, UnitId { side: Side::Black, index: 1, generation: 0 });
        assert_eq!(c, UnitId { side: Side::Red, index: 0, generation: 0 });
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
        // The hole is filled before the list grows, by a unit with a new id: the
        // old id names nothing, however it is used.
        let d = units.new_unit(Side::Black, "D", 1, 2, 10, mc);
        assert_eq!((d.side, d.index, d.generation), (b.side, b.index, b.generation + 1));
        assert_ne!(d, b);
        assert_eq!(units.get(d).unwrap().name, "D");
        assert_eq!(units.get(d).unwrap().id(), d);
        assert!(units.get(b).is_none() && units.get_mut(b).is_none());
        assert!(units.delete(b).is_none(), "a stale id must not delete the slot's new unit");
        assert_eq!(units.get(d).unwrap().name, "D");
        let e = units.new_unit(Side::Black, "E", 1, 2, 10, mc);
        assert_eq!(e.index, 3);
        assert_eq!(units.get(a).unwrap().name, "A");
        assert_eq!(units.get(c).unwrap().name, "C");
    }

    #[test]
    fn unknown_ids_are_none() {
        let mut units = Units::new();
        let ghost = UnitId { side: Side::Blue, index: 7, generation: 0 };
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
        let gone = units.new_unit(Side::Black, "gone", 1, 1, 20, mc);
        units.delete(gone);

        units.reset_move_points(Side::Black);
        let got: Vec<u32> = ids.iter().map(|id| units.get(*id).unwrap().movement).collect();
        assert_eq!(got, [20, 21, 22, 22]);
        // Other sides are untouched.
        assert_eq!(units.get(other).unwrap().movement, 5);

        // The most movement there is does not wrap around with what is carried over.
        let tireless = units.new_unit(Side::Red, "Tireless", 1, 1, u32::MAX, mc);
        units.reset_move_points(Side::Red);
        assert_eq!(units.get(tireless).unwrap().movement, u32::MAX);
    }

    #[test]
    fn a_unit_of_a_kind_has_the_kinds_numbers() {
        let mut units = Units::new();
        let lancers = UnitKind {
            id: "lancers".to_string(),
            name: "Lancers".to_string(),
            image: "units/lancers.png".to_string(),
            strength: 4,
            moves: 18,
            upkeep: 2,
            price: 9,
            cost: cav_costs(),
            commander: 0,
            travel: Travel::Land,
            carries: 0,
        };
        let kind = units.add_kind(lancers.clone()).unwrap();
        assert_eq!(units.add_kind(lancers.clone()), None, "an id is used once");
        assert_eq!(units.kind_named("lancers"), Some(kind));
        assert_eq!(units.kind_named("pikes"), None);
        assert_eq!(units.kinds(), std::slice::from_ref(&lancers));

        let id = units.new_unit_of(Side::Blue, kind).unwrap();
        let unit = units.get(id).unwrap();
        assert_eq!((unit.name.as_str(), unit.side(), unit.kind), ("Lancers", Side::Blue, Some(kind)));
        assert_eq!((unit.strength, unit.movement, unit.base_move_points, unit.upkeep), (4, 18, 18, 2));
        assert_eq!(units.move_cost(unit.move_cost), Some(&cav_costs()));
        assert_eq!(units.kind_of(unit), Some(&lancers));
        assert_eq!(units.new_unit_of(Side::Blue, UnitKindId(7)), None);
        // A unit made by hand has no kind.
        let mc = units.acquire_move_cost(cav_costs());
        let plain = units.new_unit(Side::Red, "Plain", 1, 1, 5, mc);
        assert!(units.kind_of(units.get(plain).unwrap()).is_none());
    }
}
