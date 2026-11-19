//! The game: the map, whose turn it is, and the rules that carry out the
//! sides' orders (`orders`); and what the person at the screen has selected
//! and where it can go.
//!
//! C++: src/render/maprender.cpp MapRenderer::selectHex, moveUp, moveDown, and the
//! cursor keys of src/gui/mapwidget.cpp MapWidget::OnEvent

use std::path::{Path, PathBuf};

use crate::orders::{Event, Order};
use crate::setup::{Controller, Setup, TurnOrder, WhenOut};
use crate::state::combat::{self, Fighter};
use crate::state::scenario::{self, Scenario, side_name};
use crate::state::{
    CityId, Direction, Hex, MAX_STACK, Map, MoveRange, Rng, Side, TileType, Unit, UnitId, compute_range, hostile,
    step_toward,
};

/// The agent that passes its turns: the computer's, when nothing else is said.
pub const PASSIVE: &str = "passive";

/// What `Game::select_hex` did.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectOutcome {
    /// Nothing was selected and the hex had a unit: it is selected now.
    Selected(UnitId),
    /// The selected unit's own hex was chosen: nothing is selected now.
    Deselected,
    /// The selected units attacked the hex: what happened (the battle, and
    /// the moves and the city taken if they won).
    Fought(Vec<Event>),
    /// The selected unit moved to the hex, paid `cost` and was deselected.
    Moved { unit: UnitId, cost: u32 },
    /// Nothing changed: no unit here to select, or the selected unit cannot go here.
    Nothing,
}

pub struct Game {
    pub map: Map,
    /// The units selected to move together (a stack, or some of one).
    selected: Vec<UnitId>,
    /// Where all of them can go.
    range: Option<MoveRange>,
    /// The map's revision when `range` was computed.
    range_revision: u64,
    /// The map's own asset directory, if it brought one: a picture found
    /// there is drawn instead of the game's of the same path.
    pub assets: Option<PathBuf>,
    /// How the game was set up: the mode, who plays which side.
    setup: Setup,
    /// This round's order of the sides' turns.
    order: Vec<Side>,
    /// Whose turn it is: an index into `order`.
    current: usize,
    /// The round: every side once. 1 at the start.
    round: u32,
    rng: Rng,
    /// The sides that are out, in the order they went.
    out: Vec<Side>,
    winner: Option<Side>,
}

impl Game {
    /// A game on `map` for one person (the first side), the computer passing
    /// the others' turns; see [`Game::start`].
    pub fn new(map: Map) -> Game {
        let setup = Setup::solo(&map, None, PASSIVE);
        Game::start(map, setup)
    }

    /// The game on `map`, set up as `setup` says: the first side's turn of the
    /// first round. (A side whose turn comes first may be the computer's: whoever
    /// runs the game plays it, `agents::play_computers`.)
    pub fn start(map: Map, setup: Setup) -> Game {
        let mut rng = Rng::new(setup.seed);
        let mut order: Vec<Side> = setup.seats.iter().map(|seat| seat.side).collect();
        if setup.order == TurnOrder::Drawn {
            rng.shuffle(&mut order);
        }
        Game {
            map,
            selected: Vec::new(),
            range: None,
            range_revision: 0,
            assets: None,
            setup,
            order,
            current: 0,
            round: 1,
            rng,
            out: Vec::new(),
            winner: None,
        }
    }

    /// The side whose turn it is (on a map without sides, nobody's).
    pub fn player(&self) -> Side {
        self.order.get(self.current).copied().unwrap_or(Side::Grey)
    }

    /// How the game was set up.
    pub fn setup(&self) -> &Setup {
        &self.setup
    }

    /// Who plays a side, if it is in the game.
    pub fn controller(&self, side: Side) -> Option<&Controller> {
        self.setup.controller(side)
    }

    /// This round's order of turns.
    pub fn order(&self) -> &[Side] {
        &self.order
    }

    /// The sides that are out of the game.
    pub fn out(&self) -> &[Side] {
        &self.out
    }

    /// The side that won, once only one is left.
    pub fn winner(&self) -> Option<Side> {
        self.winner
    }

    /// The game on a map that was read from its files, with its cities and
    /// its units. The cursor starts on the first city that has a garrison (or
    /// the first city, or the middle of the map); its holder is the one
    /// person's side, and the computer passes the others' turns.
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
        let person = start.and_then(|hex| map.city_at(hex)).map(|c| c.side).filter(|side| *side != Side::Grey);
        let setup = Setup::solo(&map, person, PASSIVE);
        Game { assets, ..Game::start(map, setup) }
    }

    /// Read a map (a `.map` file or a map directory, `docs/map-format.md`)
    /// and start the game on it. Also returns what the reader warns of.
    pub fn load(path: &Path) -> Result<(Game, Vec<String>), String> {
        let scenario = scenario::load(path).map_err(|e| e.to_string())?;
        let warnings = scenario.warnings.clone();
        Ok((Game::from_scenario(scenario), warnings))
    }

    /// The same game started over with another setup (the new game screen's).
    pub fn with_setup(self, setup: Setup) -> Game {
        let Game { map, assets, .. } = self;
        Game { assets, ..Game::start(map, setup) }
    }

    /// The round that is being played: 1 at the start.
    pub fn turn(&self) -> u32 {
        self.round
    }

    /// The side whose turn it is has done: the next side's turn (see
    /// [`Order::EndTurn`]). Nothing is selected.
    pub fn end_turn(&mut self) -> Vec<Event> {
        self.apply(Order::EndTurn).unwrap_or_default()
    }

    /// Carry out an order of the side whose turn it is, or say why not. The
    /// one way the game goes on: a person's clicks and the computer's agents
    /// both come here.
    pub fn apply(&mut self, order: Order) -> Result<Vec<Event>, String> {
        if let Some(side) = self.winner {
            return Err(format!("the game is over: {} won", side_name(side)));
        }
        let side = self.player();
        match order {
            Order::Move { units, to } => self.move_units(side, &units, to),
            Order::Toward { units, to } => {
                // The way of the slowest; whoever can get as far comes along.
                let mut lead: Option<&Unit> = None;
                for id in &units {
                    let unit = self.own_unit(side, *id)?;
                    if lead.is_none_or(|l| unit.movement < l.movement) {
                        lead = Some(unit);
                    }
                }
                let lead = lead.ok_or("no units")?;
                let (hex, _) = step_toward(&self.map, lead, to).ok_or("there is no way there")?;
                let map = &self.map;
                let coming: Vec<UnitId> = units
                    .iter()
                    .copied()
                    .filter(|id| map.units().get(*id).is_some_and(|u| compute_range(map, u).cost(hex).is_some()))
                    .collect();
                self.move_units(side, &coming, hex)
            }
            Order::Build { city, what } => {
                let holder = self.map.cities().get(city).map(|c| c.side).ok_or("there is no such city")?;
                if holder != side {
                    return Err(format!("only {} sets what it builds", side_name(holder)));
                }
                if !self.map.set_production(city, what) {
                    return Err("it cannot build that".to_string());
                }
                Ok(Vec::new())
            }
            Order::EndTurn => Ok(self.next_turn()),
        }
    }

    /// One of `side`'s units, or why not.
    fn own_unit(&self, side: Side, id: UnitId) -> Result<&Unit, String> {
        let unit = self.map.units().get(id).ok_or("there is no such unit")?;
        if unit.side() != side {
            return Err(format!("it is {}'s, and {}'s turn", side_name(unit.side()), side_name(side)));
        }
        Ok(unit)
    }

    /// Units of `side` go to a hex every one of them can reach, and each pays
    /// what it costs it. Into another side's units, they fight first, and go
    /// in only if they win; a city of another side with nobody left in it is
    /// taken.
    fn move_units(&mut self, side: Side, ids: &[UnitId], to: Hex) -> Result<Vec<Event>, String> {
        let mut ids = ids.to_vec();
        ids.dedup();
        if ids.is_empty() {
            return Err("no units".to_string());
        }
        // Who goes, from where, at what cost.
        let mut going = Vec::with_capacity(ids.len());
        for id in &ids {
            let unit = self.own_unit(side, *id)?;
            let cost = compute_range(&self.map, unit).cost(to);
            let cost = cost.ok_or(format!("{} cannot get there this turn", unit.name))?;
            going.push((*id, unit.pos(), cost));
        }
        let mut events = Vec::new();
        if hostile(&self.map, side, to) {
            let (won, battle) = self.battle(side, &ids, to);
            events.push(battle);
            if !won {
                for (id, _, cost) in &going {
                    self.pay(*id, *cost);
                }
                return Ok(events);
            }
            going.retain(|(id, _, _)| self.map.units().get(*id).is_some());
        } else if !self.fits(to, &ids) {
            return Err("there is no room there".to_string());
        }
        for (id, from, cost) in going {
            // After a battle, those that do not fit stay where they were.
            if self.map.move_unit(id, to).is_ok() {
                events.push(Event::Moved { unit: id, from, to, cost });
            }
            self.pay(id, cost);
        }
        self.selected.retain(|id| !ids.contains(id));
        if self.selected.is_empty() {
            self.deselect();
        }
        if let Some((city, holder)) = self.map.city_at(to).filter(|c| c.side != side).map(|c| (c.id, c.side)) {
            events.extend(self.take(city, holder, side));
        }
        Ok(events)
    }

    /// A unit pays for a move.
    fn pay(&mut self, id: UnitId, cost: u32) {
        if let Some(unit) = self.map.unit_mut(id) {
            unit.movement = unit.movement.saturating_sub(cost);
        }
    }

    /// True if `ids` all have room on `to` together (with who is there).
    fn fits(&self, to: Hex, ids: &[UnitId]) -> bool {
        let here: Vec<UnitId> = self.map.units_at(to).iter().copied().filter(|u| !ids.contains(u)).collect();
        let all = || here.iter().chain(ids);
        let commanders = all().filter(|u| self.map.is_commander(**u)).count();
        commanders <= 1 && all().count() - commanders <= MAX_STACK
    }

    /// How strong a unit fights in a battle on `at`: its strength, the best
    /// bonus of a commander in its stack (`stack`, not itself), and, if it
    /// defends, the ground's bonus and the walls of a city there.
    pub fn battle_strength(&self, unit: &Unit, at: Hex, defending: bool, stack: &[UnitId]) -> u32 {
        let units = self.map.units();
        let led = stack
            .iter()
            .filter(|id| **id != unit.id())
            .filter_map(|id| units.get(*id).and_then(|u| units.kind_of(u)))
            .map(|kind| kind.commander)
            .max()
            .unwrap_or(0);
        let mut strength = unit.strength.saturating_add(led);
        if defending {
            let ground = self.map.tile(at).map_or(0, |t| combat::ground_bonus(t.kind));
            let walls = self.map.city_at(at).map_or(0, |c| c.walls);
            strength = strength.saturating_add(ground).saturating_add(walls);
        }
        strength
    }

    /// The two sides of a battle of `attackers` against whoever is on `at`.
    fn fighters(&self, attackers: &[UnitId], at: Hex) -> (Vec<Fighter>, Vec<Fighter>) {
        let defenders: Vec<UnitId> = self.map.units_at(at).to_vec();
        let side = |ids: &[UnitId], defending: bool| -> Vec<Fighter> {
            ids.iter()
                .filter_map(|id| self.map.units().get(*id))
                .map(|unit| Fighter {
                    unit: unit.id(),
                    strength: self.battle_strength(unit, at, defending, ids),
                    commander: self.map.is_commander(unit.id()),
                })
                .collect()
        };
        (side(attackers, false), side(&defenders, true))
    }

    /// How likely `attackers` are to win against whoever is on `at`, in
    /// percent (`None` if nobody is there to fight): from battles fought with
    /// a chance of their own, so the game's is not touched, and the same
    /// question in the same turn gets the same answer.
    pub fn battle_odds(&self, attackers: &[UnitId], at: Hex) -> Option<u32> {
        let (a, d) = self.fighters(attackers, at);
        if d.is_empty() || a.is_empty() {
            return None;
        }
        let seed = (u64::from(self.round) << 32) ^ (u64::from(at.row) << 16) ^ u64::from(at.col);
        Some(combat::odds(&a, &d, &self.setup.combat, seed, 200))
    }

    /// `side`'s units attack whoever is on `at`: they fight with the game's
    /// chance; the dead go. Returns whether the attackers won, and the event.
    fn battle(&mut self, side: Side, attackers: &[UnitId], at: Hex) -> (bool, Event) {
        let (a, d) = self.fighters(attackers, at);
        let defender = d.first().map_or(Side::Grey, |f| f.unit.side);
        let battle = combat::fight(&a, &d, &self.setup.combat, &mut self.rng);
        let lost = |s: Side| battle.dead.iter().filter(|u| u.side == s).count();
        let event = Event::Battle {
            at,
            attacker: side,
            defender,
            attackers: a.len(),
            defenders: d.len(),
            attackers_lost: lost(side),
            defenders_lost: lost(defender),
            won: battle.attackers_won,
        };
        for unit in &battle.dead {
            self.map.kill_unit(*unit);
        }
        (battle.attackers_won, event)
    }

    /// `by` takes a city from `from`: it starts its work over, and a side
    /// that has lost its last city is out.
    fn take(&mut self, city: CityId, from: Side, by: Side) -> Vec<Event> {
        if let Some(c) = self.map.city_mut(city) {
            c.side = by;
        }
        self.map.set_production(city, None);
        let mut events = vec![Event::Taken { city, from, by }];
        if from != Side::Grey && !self.map.cities().iter().any(|c| c.side == from) {
            events.extend(self.put_out(from));
        }
        events
    }

    /// A side is out of the game, and its units with it (`WhenOut`); if one
    /// side is left, it has won.
    fn put_out(&mut self, side: Side) -> Vec<Event> {
        if self.out.contains(&side) {
            return Vec::new();
        }
        self.out.push(side);
        match self.setup.when_out {
            WhenOut::Disband => {
                let units: Vec<UnitId> = self.map.units().of_side(side).map(|u| u.id()).collect();
                for unit in units {
                    self.map.kill_unit(unit);
                }
            }
        }
        let mut events = vec![Event::Out { side }];
        let left: Vec<Side> = self.order.iter().copied().filter(|s| !self.out.contains(s)).collect();
        if let [last] = left[..] {
            self.winner = Some(last);
            events.push(Event::Won { side: last });
        }
        events
    }

    /// The next side's turn: after the last of the round, the first of the
    /// next (in a new order, if it is drawn). From the second round on, a
    /// side's turn begins with its units' points back (with up to 2 they kept)
    /// and its cities' work.
    fn next_turn(&mut self) -> Vec<Event> {
        self.deselect();
        if self.order.is_empty() {
            self.round += 1;
            return Vec::new();
        }
        // The sides still in; the game is over before none are.
        for _ in 0..self.order.len() * 2 {
            self.current += 1;
            if self.current >= self.order.len() {
                self.current = 0;
                self.round += 1;
                if self.setup.order == TurnOrder::Drawn {
                    self.rng.shuffle(&mut self.order);
                }
            }
            if !self.out.contains(&self.order[self.current]) {
                break;
            }
        }
        let side = self.player();
        let mut events = vec![Event::Began { side, round: self.round }];
        if self.round > 1 {
            self.map.reset_move_points(side);
            for unit in self.map.produce(side) {
                let city = self.map.units().get(unit).and_then(|u| self.map.city_at(u.pos())).map(|c| c.id);
                if let Some(city) = city {
                    events.push(Event::Built { city, unit });
                }
            }
        }
        events
    }

    /// The first of the selected units (the one the map shows on its hex).
    pub fn selected(&self) -> Option<UnitId> {
        self.selected.first().copied()
    }

    /// The units selected to move together.
    pub fn selection(&self) -> &[UnitId] {
        &self.selected
    }

    /// Where the selected units can all go.
    pub fn range(&self) -> Option<&MoveRange> {
        self.range.as_ref()
    }

    /// True if units are selected and all of them can move to `hex` with the
    /// movement they have left (into an enemy: an attack). These are the hexes
    /// the map marks.
    pub fn can_move_to(&self, hex: Hex) -> bool {
        self.range.as_ref().is_some_and(|r| r.cost(hex).is_some())
    }

    pub fn deselect(&mut self) {
        self.selected.clear();
        self.range = None;
    }

    /// Select one unit by name (a slot of the stack was chosen), and no other.
    /// False, and nothing changes, if there is no such unit any more, or it is
    /// not the side's whose turn it is.
    pub fn select_unit(&mut self, id: UnitId) -> bool {
        self.own(id) && self.select(vec![id])
    }

    /// Take a unit into the selection, or out of it (a slot of the stack was
    /// chosen): a unit of another hex than the selection's starts a new one.
    /// False if it is not the side's whose turn it is.
    pub fn toggle_unit(&mut self, id: UnitId) -> bool {
        if !self.own(id) {
            return false;
        }
        let pos = |game: &Game, u: UnitId| game.map.units().get(u).map(|u| u.pos());
        let mut units = self.selected.clone();
        if units.first().is_some_and(|first| pos(self, *first) != pos(self, id)) {
            units.clear();
        }
        if let Some(i) = units.iter().position(|u| *u == id) {
            units.remove(i);
        } else {
            units.push(id);
        }
        if units.is_empty() {
            self.deselect();
            return true;
        }
        self.select(units)
    }

    fn own(&self, id: UnitId) -> bool {
        self.map.units().get(id).is_some_and(|u| u.side() == self.player())
    }

    /// Select these units (they are there), with where all of them can go.
    fn select(&mut self, units: Vec<UnitId>) -> bool {
        let map = &self.map;
        let mut range: Option<MoveRange> = None;
        for id in &units {
            let Some(unit) = map.units().get(*id) else { return false };
            let one = compute_range(map, unit);
            range = Some(match range {
                Some(all) => all.both(&one),
                None => one,
            });
        }
        self.range = range;
        self.range_revision = self.map.revision();
        self.selected = units;
        true
    }

    /// Bring the selection up to date with the map, which is public and may have
    /// changed since the units were selected: units that are gone are no longer
    /// selected, and a range computed from an older map is computed again.
    /// `select_hex` does this itself; whoever draws the range calls it first.
    /// Returns true if anything changed.
    pub fn refresh(&mut self) -> bool {
        if self.selected.is_empty() {
            return false;
        }
        let before = self.selected.len();
        let units = self.map.units();
        self.selected.retain(|id| units.get(*id).is_some());
        if self.selected.is_empty() {
            self.deselect();
            return true;
        }
        if before == self.selected.len() && self.range.is_some() && self.range_revision == self.map.revision() {
            return false;
        }
        self.select(self.selected.clone())
    }

    /// The player chose a hex (a click, or Enter at the cursor).
    ///
    /// - Units are selected and this is their hex: deselect them.
    /// - Units are selected and all of them can reach the hex: they move there
    ///   (`Order::Move`: each pays its cost; into another side's units it is a
    ///   battle; a city may be taken) and are deselected.
    /// - Nothing is selected and the hex has units of the side whose turn it
    ///   is: select them all, the stack (those with points left, if any have).
    ///
    /// C++: MapRenderer::selectHex. There a selected unit could go to any hex that
    /// the flood fill had not reached, mountains included, for free; here it stays
    /// selected and nothing happens.
    pub fn select_hex(&mut self, hex: Hex) -> SelectOutcome {
        self.refresh();
        let Some(first) = self.selected() else {
            let side = self.player();
            let units = self.map.units();
            let own: Vec<UnitId> = self
                .map
                .units_at(hex)
                .iter()
                .copied()
                .filter(|id| units.get(*id).is_some_and(|u| u.side() == side))
                .collect();
            let able: Vec<UnitId> =
                own.iter().copied().filter(|id| units.get(*id).is_some_and(|u| u.movement > 0)).collect();
            let stack = if able.is_empty() { own } else { able };
            return match stack.first().copied() {
                Some(first) if self.select(stack) => SelectOutcome::Selected(first),
                _ => SelectOutcome::Nothing,
            };
        };
        if self.map.units().get(first).is_some_and(|u| u.pos() == hex) {
            self.deselect();
            return SelectOutcome::Deselected;
        }
        let Some(cost) = self.range.as_ref().and_then(|r| r.cost(hex)) else { return SelectOutcome::Nothing };
        let units = self.selected.clone();
        match self.apply(Order::Move { units, to: hex }) {
            Ok(events) => {
                self.deselect();
                if events.iter().any(|e| matches!(e, Event::Battle { .. })) {
                    SelectOutcome::Fought(events)
                } else {
                    SelectOutcome::Moved { unit: first, cost }
                }
            }
            Err(_) => SelectOutcome::Nothing, // a full stack, for one
        }
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

/// A game for tests (it was the C++ demo's): 100 x 100 grass with a mountain
/// diagonal, and one black rider in the middle, which stands on the diagonal
/// (a mountain it could not enter).
#[cfg(test)]
pub(crate) fn grassland() -> Game {
    let mut map = Map::new(100, 100, TileType::Grass).expect("100 x 100 is a valid map size");
    for i in 0..100 {
        if let Some(tile) = map.tile_mut(Hex::new(i, i)) {
            tile.kind = TileType::Mountain;
        }
    }
    let costs = crate::state::MoveCost {
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
    };
    let cost = map.acquire_move_cost(costs);
    let rider = map.new_unit(Side::Black, "Test Cav", 3, 5, 20, cost);
    map.place_unit(rider, Hex::new(50, 50)).expect("a hex of a new map is free");
    map.cursor = Hex::new(50, 50);
    Game::new(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::MAX_STACK;

    const START: Hex = Hex::new(50, 50);

    #[test]
    fn the_grassland_state() {
        let game = grassland();
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
        let mut game = grassland();
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
        let mut game = grassland();
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
        let mut game = grassland();
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
        let mut game = grassland();
        assert_eq!(game.select_hex(Hex::new(10, 10)), SelectOutcome::Nothing);
        assert!(game.selected().is_none());
    }

    #[test]
    fn a_full_stack_refuses_the_move_and_keeps_the_selection() {
        let mut game = grassland();
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
        let mut game = grassland();
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
        let mut game = grassland();
        let &[cav] = game.map.units_at(START) else { panic!() };
        game.select_hex(START);
        game.map.kill_unit(cav);
        assert_eq!(game.select_hex(Hex::new(50, 51)), SelectOutcome::Nothing);
        assert!(game.selected().is_none() && game.range().is_none());
    }

    #[test]
    fn a_new_unit_in_a_dead_units_slot_does_not_inherit_the_selection() {
        let mut game = grassland();
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
        let mut game = grassland();
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
        let mut game = grassland();
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

        // The city's stack goes down the road and over the bridge, the two
        // riders together: 1 a step.
        assert!(matches!(game.select_hex(city), SelectOutcome::Selected(_)));
        assert_eq!(game.selection().len(), 2);
        assert!(game.can_move_to(Hex::new(5, 8)));
        assert_eq!(game.range().unwrap().cost(Hex::new(5, 8)), Some(3));
        assert_eq!(game.range().unwrap().cost(Hex::new(5, 9)), Some(4), "the pass");
        assert!(matches!(game.select_hex(Hex::new(5, 9)), SelectOutcome::Moved { cost: 4, .. }));
        assert!(game.map.units_at(city).is_empty());
        let riders = game.map.units_at(Hex::new(5, 9)).to_vec();
        assert_eq!(riders.len(), 2);
        assert!(riders.iter().all(|r| game.map.units().get(*r).unwrap().movement == 6));

        // One of them goes on alone; the other stays.
        assert!(game.select_unit(riders[1]));
        assert!(matches!(game.select_hex(Hex::new(5, 10)), SelectOutcome::Moved { cost: 2, .. }));
        assert_eq!(game.map.units_at(Hex::new(5, 9)), [riders[0]]);

        // The turn ends: they have their points back (and the two they may
        // carry over).
        game.end_turn();
        assert_eq!((game.turn(), game.selected()), (2, None));
        assert_eq!(game.map.units().get(riders[0]).unwrap().movement, 12);
        assert_eq!(game.map.units().get(riders[1]).unwrap().movement, 12);
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
    fn a_map_is_loaded_as_it_is() {
        let dir = std::env::temp_dir().join(format!("hexgame-game-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("bare/assets")).unwrap();
        let terrain = format!("20\n20\n{}", "gggggggggggggggggggg\n".repeat(20));
        std::fs::write(dir.join("bare/terrain.map"), &terrain).unwrap();
        std::fs::write(dir.join("plain.map"), terrain.replacen("gg", "cg", 1)).unwrap();

        // Terrain alone: no units, and no sides to play.
        let (game, warnings) = Game::load(&dir.join("bare")).unwrap();
        assert_eq!(game.map.units().iter().count(), 0);
        assert!(game.setup().seats.is_empty());
        assert_eq!(game.assets, Some(dir.join("bare/assets")));
        assert!(warnings.is_empty());
        let (game, warnings) = Game::load(&dir.join("plain.map")).unwrap();
        assert_eq!(game.map.units().iter().count(), 0);
        assert_eq!(warnings, ["the city tile at (0, 0) has no entry in cities.txt".to_string()]);
        assert!(game.assets.is_none());
        let e = Game::load(&dir.join("none.map")).err().unwrap();
        assert!(e.contains("none.map"), "{e}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_player_is_the_side_whose_city_the_game_starts_in() {
        let mut rows = vec!["g".repeat(20); 20];
        rows[5] = "gggggcgggggcgggggggg".to_string();
        let terrain = format!("20\n20\n{}\n", rows.join("\n"));
        let units = "unit militia\nname Militia\nmoves 6\nenter grass 1, city 1\n";
        let cities = "city 5 5\nname Alder\nside blue\ncity 5 11\nname Birch\nside green\ngarrison militia\n";
        let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
        let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
        assert_eq!(game.player(), Side::Green, "Birch has the garrison: the game starts there");
        assert_eq!(game.controller(Side::Green), Some(&Controller::Person));
        assert_eq!(game.controller(Side::Blue), Some(&Controller::Computer(PASSIVE.to_string())));
        // Then blue's turn.
        game.end_turn();
        assert_eq!(game.player(), Side::Blue);
        // A map without cities: the sides with units.
        assert_eq!(grassland().player(), Side::Black);
        // A map without sides: nobody's turn, and the turns go by.
        let mut empty = Game::new(Map::new(20, 20, TileType::Grass).unwrap());
        assert_eq!(empty.player(), Side::Grey);
        empty.end_turn();
        assert_eq!((empty.player(), empty.turn()), (Side::Grey, 2));
    }

    #[test]
    fn the_end_of_a_turn_is_a_turn_of_work_in_the_cities() {
        let mut rows = vec!["g".repeat(20); 20];
        rows[5] = "gggggcgggggggggggggg".to_string();
        let terrain = format!("20\n20\n{}\n", rows.join("\n"));
        let units = "unit militia\nname Militia\nmoves 6\nenter grass 1, city 1\n";
        let cities = "city 5 5\nname Alder\nside blue\nproduce militia 2\n";
        let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
        let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
        let id = game.map.city_at(Hex::new(5, 5)).unwrap().id;
        game.map.set_production(id, Some(0));
        game.end_turn();
        assert!(game.map.units_at(Hex::new(5, 5)).is_empty());
        game.end_turn();
        let &[unit] = game.map.units_at(Hex::new(5, 5)) else { panic!("one militia") };
        // With its moves, and no more: it did not get the turn's extra.
        assert_eq!(game.map.units().get(unit).unwrap().movement, 6);
    }
}
