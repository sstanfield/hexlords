//! The game: the map, whose turn it is, and the rules that carry out the
//! sides' orders (`orders`); and what the person at the screen has selected
//! and where it can go.
//!
//! C++: src/render/maprender.cpp MapRenderer::selectHex, moveUp, moveDown, and the
//! cursor keys of src/gui/mapwidget.cpp MapWidget::OnEvent

use std::path::{Path, PathBuf};

use crate::orders::{Event, Fallen, Order, Record};
use crate::setup::{Controller, Setup, TurnOrder, WhenOut};
use crate::state::Blessing;
use crate::state::combat::{self, Fighter};
use crate::state::scenario::{self, Scenario, side_name};
use crate::state::{
    CityId, Direction, Hex, MAX_STACK, Map, MoveRange, NUM_SIDES, Passage, Rng, Side, TileType, Unit, UnitId,
    compute_range, hostile, passage, reach_with, step_toward_with, way_over_land,
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
    /// The selected units moved to the hex (the first of them paid `cost`)
    /// and were deselected; what happened (the moves, a city taken).
    Moved { unit: UnitId, cost: u32, events: Vec<Event> },
    /// Nothing was selected, and the hex has none of the side's units.
    Nothing,
}

/// The hex after `from` in `hexes` (in reading order), or the one before it;
/// round from the end to the start. From nowhere: the first, or the last.
fn next_in(hexes: &[Hex], from: Option<Hex>, back: bool) -> Option<Hex> {
    let key = |h: &Hex| (h.row, h.col);
    let after = |h: &&Hex| from.is_some_and(|f| key(h) > key(&f));
    let before = |h: &&Hex| from.is_some_and(|f| key(h) < key(&f));
    if back {
        hexes.iter().rev().find(before).or(hexes.last()).copied()
    } else {
        hexes.iter().find(after).or(hexes.first()).copied()
    }
}

/// What the last order changed, kept so that it can be taken back
/// (`Order::Undo`). Only an order with no chance in it that put nobody out
/// is kept: a battle cannot be undone.
#[derive(Clone, Debug)]
enum Undo {
    /// Units went to `to`: where each was and the points it had; the order of
    /// the stacks they left; and the city they took, with who held it and
    /// what it was building, how far.
    Moved {
        units: Vec<(UnitId, Hex, u32)>,
        to: Hex,
        stacks: Vec<(Hex, Vec<UnitId>)>,
        taken: Option<(CityId, Side, Option<usize>, u32)>,
        /// Where each was going, given back: its standing order, whether it
        /// had stalled, whether it had moved this turn.
        goals: Vec<(UnitId, MarchState)>,
    },
    /// A city was set to build something else: what it was building, how far.
    Built { city: CityId, building: Option<usize>, progress: u32 },
}

/// Something a side has left to do, or to look at, this turn (`Game::tasks`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Task {
    /// A stack of the side's, on its hex: how many units, and how it stands.
    Stack {
        at: Hex,
        units: usize,
        state: StackState,
    },
    City {
        city: CityId,
        state: CityState,
    },
}

/// How a stack stands this turn, as far as orders go.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum StackState {
    /// It can go, and nobody has told it anything this turn.
    Waiting,
    /// Its standing order stopped short: something is in the way.
    Stalled,
    /// It went, and can go further.
    MovesLeft,
    /// Its player is done with it, though it could still go.
    Done,
    /// It is on its way somewhere, on a standing order.
    Marching,
    /// It can go nowhere more this turn.
    Spent,
}

/// How a city stands this turn.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CityState {
    /// It builds nothing.
    Idle,
    /// What it builds is done and waits: for the gold, or for room.
    Waiting,
    /// It is building.
    Building,
}

impl Task {
    /// How pressing it is: 1, what has had no orders (stacks that wait, or
    /// whose way is blocked, and cities that build nothing); 2, what could
    /// do more (stacks with moves left, or put aside with moves, and cities
    /// whose work waits); 3, the rest.
    pub fn tier(&self) -> u8 {
        match self {
            Task::Stack { state: StackState::Waiting | StackState::Stalled, .. } => 1,
            Task::City { state: CityState::Idle, .. } => 1,
            Task::Stack { state: StackState::MovesLeft | StackState::Done, .. } => 2,
            Task::City { state: CityState::Waiting, .. } => 2,
            _ => 3,
        }
    }
}

/// What a unit is doing about a standing order: its `goto`, whether it has
/// stalled, whether it has moved this turn.
type MarchState = (Option<Hex>, bool, bool);

/// How far a side sees, with fog of war: from its units, and from its cities.
pub const UNIT_SIGHT: u32 = 2;
pub const CITY_SIGHT: u32 = 3;

/// What a side sees: every hex (no fog of war), or those near its units and
/// cities (`Game::sight`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sight {
    cols: u16,
    /// Row-major; `None`: everything.
    seen: Option<Vec<bool>>,
}

impl Sight {
    /// Sight of everything.
    pub fn all() -> Sight {
        Sight::default()
    }

    /// True if the hex is seen: who stands there, who holds its city.
    pub fn sees(&self, hex: Hex) -> bool {
        match &self.seen {
            None => true,
            Some(seen) => seen.get(usize::from(hex.row) * usize::from(self.cols) + usize::from(hex.col)) == Some(&true),
        }
    }
}

/// How many rounds a battle can be watched again for (`Game::fight`).
pub const REPLAY_ROUNDS: u32 = 5;

/// A stack with no commander that wins a battle sees a captain rise from its
/// ranks with the chance, in percent, of half of how unlikely its win was:
/// `(100 - odds) / 2` (the owner's word, 2026-11-19).
pub fn emerge_chance(odds: u32) -> u32 {
    (100 - odds.min(100)) / 2
}
/// A battle won against odds below this (percent) is worth a point more of
/// experience to who lived through it.
pub const HARD_ODDS: u32 = 50;

/// The parts of the names captains are given, put together by chance: our
/// own, not anyone's in particular.
const NAME_STARTS: [&str; 24] = [
    "Al", "Bran", "Cor", "Dag", "Ed", "Fen", "Gar", "Hal", "Is", "Jor", "Kel", "Lor", "Mor", "Nor", "Os", "Per", "Ren",
    "Sel", "Tam", "Ul", "Ver", "Wil", "Yor", "Ash",
];
const NAME_ENDS: [&str; 16] =
    ["ric", "wen", "dan", "mund", "eth", "ra", "win", "bert", "ia", "mir", "ton", "sa", "na", "dred", "wyn", "ald"];

/// A unit in a battle, as the battle saw it: who it was, and what its
/// strength was made of, in points (the walls' in quarters of a point, as a
/// battle counts them: `combat::QUARTER`).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Combatant {
    pub unit: UnitId,
    pub side: Side,
    pub name: String,
    /// Its kind's picture.
    pub image: String,
    pub commander: bool,
    /// What it is called in the battle: its own name if it has one (a
    /// captain's), else its kind's; and its level.
    pub title: String,
    pub level: u8,
    /// Its kind's strength, what its level and a blessing add to it, a
    /// commander's bonus, the ground's (a defender's).
    pub strength: u32,
    pub growth: u32,
    pub led: u32,
    pub ground: u32,
    /// The walls' bonus, in quarters (a defender's in a city).
    pub walls: u32,
    /// All of it, in quarters: what it fought with.
    pub quarters: u32,
}

/// A battle, kept so that it can be watched again: where, who, under what
/// rules, everyone in it with their strengths, and each exchange.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Fight {
    pub at: Hex,
    pub attacker: Side,
    pub defender: Side,
    pub rules: crate::state::combat::Rules,
    /// The ground of the hex, and the walls of a city there.
    pub terrain: TileType,
    pub walls: u32,
    pub attackers: Vec<Combatant>,
    pub defenders: Vec<Combatant>,
    pub trace: Vec<combat::Exchange>,
    pub won: bool,
}

#[derive(Clone)]
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
    /// The last order, if it can be taken back, and how long the history
    /// was before it.
    undo: Option<Undo>,
    undo_mark: usize,
    /// What has happened: the battles, the cities that changed hands, the
    /// sides that went out (`Game::history`).
    history: Vec<Record>,
    /// Each side's gold, by `Side::index`.
    treasury: [u32; NUM_SIDES + 1],
    /// The battles of the last few rounds, to be watched again: the index of
    /// each one's record in the history, its round, the fight; and the one
    /// the order being carried out fought, until it is kept.
    fights: Vec<(usize, u32, Fight)>,
    fought: Option<Fight>,
    /// After a battle won by a stack with no commander against the odds:
    /// its side, and the hex its captain would rise on.
    emerging: Option<(Side, Hex)>,
    /// The levels the last battle brought, to be told after it.
    promoted: Vec<Event>,
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
        let mut game = Game {
            map,
            selected: Vec::new(),
            range: None,
            range_revision: 0,
            assets: None,
            order,
            current: 0,
            round: 1,
            rng,
            out: Vec::new(),
            winner: None,
            undo: None,
            undo_mark: 0,
            history: Vec::new(),
            treasury: [setup.gold; NUM_SIDES + 1],
            fights: Vec::new(),
            fought: None,
            emerging: None,
            promoted: Vec::new(),
            setup,
        };
        game.name_commanders();
        game
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

    /// Give every commander on the map that has no name one (the game's
    /// chance picks them).
    fn name_commanders(&mut self) {
        let nameless: Vec<UnitId> = self
            .map
            .units()
            .iter()
            .filter(|u| u.given_name.is_none() && self.map.is_commander(u.id()))
            .map(|u| u.id())
            .collect();
        for id in nameless {
            let name = self.captain_name();
            if let Some(unit) = self.map.unit_mut(id) {
                unit.given_name = Some(name);
            }
        }
    }

    /// A name for a captain, put together by chance; one no captain on the
    /// map has, if a few tries find one.
    fn captain_name(&mut self) -> String {
        let mut name = String::new();
        for _ in 0..8 {
            let start = NAME_STARTS[self.rng.below(NAME_STARTS.len() as u64) as usize];
            let end = NAME_ENDS[self.rng.below(NAME_ENDS.len() as u64) as usize];
            name = format!("{start}{end}");
            if !self.map.units().iter().any(|u| u.given_name.as_deref() == Some(name.as_str())) {
                break;
            }
        }
        name
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
        let last = self.undo.take();
        if order == Order::Undo {
            return self.take_back(side, last);
        }
        let (round, mark) = (self.round, self.history.len());
        // Being done with units changes nothing to take back: the order
        // before can still be.
        let keeps = matches!(order, Order::Done { .. });
        let done = self.carry_out(side, order);
        if keeps && done.is_ok() {
            self.undo = last;
            return done;
        }
        match &done {
            Ok(events) => {
                self.remember(round, side, events);
                self.undo_mark = mark;
            }
            Err(_) => self.undo = last, // nothing changed: it can still be taken back
        }
        done
    }

    /// Keep what an order of `turn`'s, in `round`, did that is worth keeping:
    /// not the moves (there are too many, and they say little afterwards), the
    /// turns' beginnings, or an undo.
    /// (What comes after a turn's beginning, in the same order, is that
    /// turn's: the marches, the units built, what they take.)
    fn remember(&mut self, round: u32, turn: Side, events: &[Event]) {
        let (mut round, mut turn) = (round, turn);
        for event in events {
            if let Event::Began { side, round: began } = event {
                (round, turn) = (*began, *side);
            }
            if matches!(event, Event::Moved { .. } | Event::Began { .. } | Event::Undone) {
                continue;
            }
            // A battle's fight is kept beside its record.
            if matches!(event, Event::Battle { .. }) {
                if let Some(fight) = self.fought.take() {
                    self.fights.push((self.history.len(), round, fight));
                }
            }
            self.history.push(Record { round, turn, event: event.clone() });
        }
    }

    /// The battle of the history's record `record`, to be watched again, if
    /// it was in the last [`REPLAY_ROUNDS`] rounds.
    pub fn fight(&self, record: usize) -> Option<&Fight> {
        self.fights.iter().find(|(i, _, _)| *i == record).map(|(_, _, fight)| fight)
    }

    /// What has happened in the game, oldest first: every battle, every city
    /// taken, every unit built, every side out, and the winner, with the
    /// round and whose turn it was. An order that was taken back is not in it.
    pub fn history(&self) -> &[Record] {
        &self.history
    }

    /// True if the last order can be taken back (`Order::Undo`).
    pub fn can_undo(&self) -> bool {
        self.undo.is_some() && self.winner.is_none()
    }

    fn carry_out(&mut self, side: Side, order: Order) -> Result<Vec<Event>, String> {
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
                // As far as there is room for all of them.
                let room = |hex: Hex| self.fits(hex, &units);
                let (hex, _) = step_toward_with(&self.map, lead, to, &room).ok_or("there is no way there")?;
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
                let (building, progress) = self.map.cities().get(city).map_or((None, 0), |c| (c.building, c.progress));
                if !self.map.set_production(city, what) {
                    return Err("it cannot build that".to_string());
                }
                self.undo = Some(Undo::Built { city, building, progress });
                Ok(Vec::new())
            }
            Order::EndTurn => Ok(self.next_turn()),
            Order::Undo => Err("there is nothing to undo".to_string()),
            Order::GoTo { units, to } => {
                if !self.map.contains(to) {
                    return Err(format!("{to} is not on the map"));
                }
                let before: Vec<(UnitId, MarchState)> = units
                    .iter()
                    .map(|id| self.own_unit(side, *id).map(|u| (*id, (u.goto, u.stalled, u.moved))))
                    .collect::<Result<_, _>>()?;
                if units.is_empty() {
                    return Err("no units".to_string());
                }
                // Each of them can get there over the land, in as many turns
                // as it takes (whoever is on the way now: units move).
                for id in &units {
                    let Some(unit) = self.map.units().get(*id) else { continue };
                    let costs = self.map.units().move_cost(unit.move_cost);
                    if !costs.is_some_and(|c| way_over_land(&self.map, c, unit.pos(), to)) {
                        return Err(format!("{} cannot get there", unit.title()));
                    }
                }
                for id in &units {
                    if let Some(unit) = self.map.unit_mut(*id) {
                        (unit.goto, unit.stalled) = (Some(to), false);
                    }
                }
                // This turn's step, as the march would, a stack (the units of a
                // hex) at a time.
                let mut groups: Vec<(Hex, Vec<UnitId>)> = Vec::new();
                for id in &units {
                    let Some(at) = self.map.units().get(*id).map(|u| u.pos()) else { continue };
                    match groups.iter_mut().find(|(hex, _)| *hex == at) {
                        Some((_, ids)) => ids.push(*id),
                        None => groups.push((at, vec![*id])),
                    }
                }
                let mut events = Vec::new();
                self.undo = None;
                for (_, ids) in &groups {
                    events.extend(self.march(side, ids, to));
                }
                // An undo gives the units back what they were doing before (a
                // stack from one hex: units from several cannot be taken back).
                match &mut self.undo {
                    Some(Undo::Moved { goals, .. }) if groups.len() == 1 => *goals = before,
                    _ => self.undo = None,
                }
                Ok(events)
            }
            Order::Done { units } => {
                for id in &units {
                    self.own_unit(side, *id)?;
                }
                for id in &units {
                    if let Some(unit) = self.map.unit_mut(*id) {
                        unit.done = true;
                    }
                }
                Ok(Vec::new())
            }
            Order::Rally { city, to } => {
                let holder = self.map.cities().get(city).map(|c| c.side).ok_or("there is no such city")?;
                if holder != side {
                    return Err(format!("only {} says where its units go", side_name(holder)));
                }
                if to.is_some_and(|hex| !self.map.contains(hex)) {
                    return Err("that is not on the map".to_string());
                }
                // Somewhere what it builds can get to.
                if let (Some(to), Some(c)) = (to, self.map.cities().get(city)) {
                    let units = self.map.units();
                    let reaches = c.units.iter().any(|prod| {
                        let kind = units.kind_named(&prod.basename).and_then(|k| units.kind(k));
                        kind.is_some_and(|k| way_over_land(&self.map, &k.cost, c.pos, to))
                    });
                    if !reaches {
                        return Err(format!("nothing {} builds can get there", c.name));
                    }
                }
                if let Some(c) = self.map.city_mut(city) {
                    c.rally = to;
                }
                Ok(Vec::new())
            }
        }
    }

    /// Units of `side` on a standing order to `to` take this turn's step: as
    /// far as the slowest of them gets, where there is room for all of them,
    /// and those that can get as far go. They do not attack: a step into
    /// another side's units, or no way at all, stalls them (they keep their
    /// order, and wait for their player). There, their order ends.
    fn march(&mut self, side: Side, ids: &[UnitId], to: Hex) -> Vec<Event> {
        let units: Vec<&Unit> = ids.iter().filter_map(|id| self.map.units().get(*id)).collect();
        let Some(lead) = units.iter().copied().min_by_key(|u| u.movement) else { return Vec::new() };
        let at = lead.pos();
        // (A stack: the units of one hex.)
        let ids: Vec<UnitId> = units.iter().filter(|u| u.pos() == at).map(|u| u.id()).collect();
        let units: Vec<&Unit> = units.into_iter().filter(|u| u.pos() == at).collect();
        if at == to {
            self.arrive(&ids);
            return Vec::new();
        }
        if lead.movement == 0 {
            return Vec::new();
        }
        let room = |hex: Hex| self.fits(hex, &ids);
        let step = step_toward_with(&self.map, lead, to, &room).filter(|(hex, _)| !hostile(&self.map, side, *hex));
        let coming: Vec<UnitId> = match step {
            Some((hex, _)) => {
                units.iter().filter(|u| compute_range(&self.map, u).cost(hex).is_some()).map(|u| u.id()).collect()
            }
            None => Vec::new(),
        };
        let moved = match step {
            Some((hex, _)) if !coming.is_empty() => self.move_units(side, &coming, hex).ok().map(|e| (hex, e)),
            _ => None,
        };
        let Some((hex, events)) = moved else {
            for id in &ids {
                if let Some(unit) = self.map.unit_mut(*id) {
                    unit.stalled = true;
                }
            }
            return vec![Event::Stalled { side, units: ids, at, to }];
        };
        // On, or there.
        for id in &coming {
            if let Some(unit) = self.map.unit_mut(*id) {
                unit.goto = (hex != to).then_some(to);
            }
        }
        events
    }

    /// Units on a standing order are where it sent them: it ends.
    fn arrive(&mut self, ids: &[UnitId]) {
        for id in ids {
            if let Some(unit) = self.map.unit_mut(*id) {
                (unit.goto, unit.stalled) = (None, false);
            }
        }
    }

    /// At the start of `side`'s turn, its stacks on standing orders march, a
    /// stack (the units of a hex going to the same hex) at a time.
    fn marches(&mut self, side: Side) -> Vec<Event> {
        let mut groups: Vec<(Hex, Hex, Vec<UnitId>)> = Vec::new();
        for unit in self.map.units().of_side(side).filter(|u| self.map.contains(u.pos())) {
            let Some(to) = unit.goto else { continue };
            match groups.iter_mut().find(|(at, goal, _)| *at == unit.pos() && *goal == to) {
                Some((_, _, ids)) => ids.push(unit.id()),
                None => groups.push((unit.pos(), to, vec![unit.id()])),
            }
        }
        let mut events = Vec::new();
        for (_, to, ids) in groups {
            events.extend(self.march(side, &ids, to));
        }
        // What marched on its own is not the player's to take back.
        self.undo = None;
        events
    }

    /// Take back the last order (`last`), which is `side`'s, this turn.
    fn take_back(&mut self, side: Side, last: Option<Undo>) -> Result<Vec<Event>, String> {
        let mark = self.undo_mark;
        match last.ok_or("there is nothing to undo")? {
            Undo::Moved { units, to, stacks, taken, goals } => {
                for (id, _, _) in &units {
                    if self.own_unit(side, *id)?.pos() != to {
                        return Err("the units are not where the order left them".to_string());
                    }
                }
                if let Some((city, holder, building, progress)) = taken {
                    if let Some(c) = self.map.city_mut(city) {
                        (c.side, c.building, c.progress) = (holder, building, progress);
                    }
                }
                for (id, from, movement) in &units {
                    self.map.move_unit(*id, *from).map_err(|_| "there is no room where they were".to_string())?;
                    if let Some(unit) = self.map.unit_mut(*id) {
                        unit.movement = *movement;
                    }
                }
                for (hex, order) in &stacks {
                    self.map.set_stack_order(*hex, order);
                }
                for (id, (goto, stalled, moved)) in goals {
                    if let Some(unit) = self.map.unit_mut(id) {
                        (unit.goto, unit.stalled, unit.moved) = (goto, stalled, moved);
                    }
                }
                // They are where they were, and selected again, to go
                // somewhere else.
                self.select(units.iter().map(|(id, _, _)| *id).collect());
            }
            Undo::Built { city, building, progress } => {
                let holder = self.map.cities().get(city).map(|c| c.side);
                if holder != Some(side) {
                    return Err("the city is not the side's".to_string());
                }
                if let Some(c) = self.map.city_mut(city) {
                    (c.building, c.progress) = (building, progress);
                }
            }
        }
        // What it did never happened.
        self.history.truncate(mark);
        Ok(vec![Event::Undone])
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
        // Who goes, from where, at what cost; and how things stood before,
        // for an undo.
        let mut going = Vec::with_capacity(ids.len());
        let mut before = Vec::with_capacity(ids.len());
        let mut goals = Vec::with_capacity(ids.len());
        for id in &ids {
            let unit = self.own_unit(side, *id)?;
            let cost = compute_range(&self.map, unit).cost(to);
            let cost = cost.ok_or(format!("{} cannot get there this turn", unit.name))?;
            going.push((*id, unit.pos(), cost));
            before.push((*id, unit.pos(), unit.movement));
            goals.push((*id, (unit.goto, unit.stalled, unit.moved)));
        }
        let city_before = self.map.city_at(to).map(|c| (c.id, c.side, c.building, c.progress));
        let mut stacks: Vec<(Hex, Vec<UnitId>)> = Vec::new();
        for (_, from, _) in &before {
            if !stacks.iter().any(|(hex, _)| hex == from) {
                stacks.push((*from, self.map.units_at(*from).to_vec()));
            }
        }
        let mut events = Vec::new();
        if hostile(&self.map, side, to) {
            // An attack is a stack: as many as a hex holds, from wherever.
            if !self.fits(Hex::new(u16::MAX, u16::MAX), &ids) {
                return Err(format!("at most {MAX_STACK} units and a commander attack together"));
            }
            let (won, battle) = self.battle(side, &ids, to);
            events.push(battle);
            events.append(&mut self.promoted);
            if !won {
                for (id, _, cost) in &going {
                    self.pay(*id, *cost);
                }
                if let Some((side, at)) = self.emerging.take() {
                    events.extend(self.emerge(side, at));
                }
                return Ok(events);
            }
            going.retain(|(id, _, _)| self.map.units().get(*id).is_some());
        } else if !self.fits(to, &ids) {
            return Err("there is no room there".to_string());
        }
        let mut arrived = Vec::new();
        for (id, from, cost) in going {
            // After a battle, those that do not fit stay where they were.
            if self.map.move_unit(id, to).is_ok() {
                events.push(Event::Moved { unit: id, from, to, cost });
                arrived.push(id);
            }
            self.pay(id, cost);
        }
        for id in &ids {
            if let Some(unit) = self.map.unit_mut(*id) {
                (unit.moved, unit.goto, unit.stalled) = (true, None, false);
            }
        }
        if let Some((side, at)) = self.emerging.take() {
            events.extend(self.emerge(side, at));
        }
        // A temple blesses who stops on it, once in a unit's life: chance
        // says how.
        let mut blessed = false;
        if self.map.tile(to).is_some_and(|t| t.kind == TileType::Temple) {
            for id in arrived {
                let blessing = if self.rng.below(2) == 0 { Blessing::Strength } else { Blessing::Moves };
                let Some(unit) = self.map.unit_mut(id).filter(|u| u.blessing.is_none()) else { continue };
                unit.bless(blessing);
                events.push(Event::Blessed { unit: id, side: unit.side(), name: unit.title(), blessing });
                blessed = true;
            }
        }
        self.selected.retain(|id| !ids.contains(id));
        if self.selected.is_empty() {
            self.deselect();
        }
        let mut taken = None;
        if let Some((city, holder)) = self.map.city_at(to).filter(|c| c.side != side).map(|c| (c.id, c.side)) {
            events.extend(self.take(city, holder, side));
            taken = city_before;
        }
        // A move can be taken back unless it was a battle, put a side out, or
        // a temple's blessing came of it (chance had a say).
        let final_ = events.iter().any(|e| matches!(e, Event::Battle { .. } | Event::Out { .. } | Event::Won { .. }));
        if !final_ && !blessed {
            self.undo = Some(Undo::Moved { units: before, to, stacks, taken, goals });
        }
        Ok(events)
    }

    /// A unit pays for a move.
    fn pay(&mut self, id: UnitId, cost: u32) {
        if let Some(unit) = self.map.unit_mut(id) {
            unit.movement = unit.movement.saturating_sub(cost);
        }
    }

    /// True if `ids` all have room on `to` together (with who is there; on a
    /// hex off the map, nobody is).
    fn fits(&self, to: Hex, ids: &[UnitId]) -> bool {
        let here: Vec<UnitId> = self.map.units_at(to).iter().copied().filter(|u| !ids.contains(u)).collect();
        let all = || here.iter().chain(ids);
        let commanders = all().filter(|u| self.map.is_commander(**u)).count();
        commanders <= 1 && all().count() - commanders <= MAX_STACK
    }

    /// How strong a unit fights in a battle on `at`, in quarters of a point
    /// (`combat::QUARTER`): its strength and the best bonus of a commander in
    /// its stack (`stack`, not itself), and, if it defends, the ground's
    /// bonus, and three quarters of a point for each level of the walls of a
    /// city there.
    pub fn battle_strength(&self, unit: &Unit, at: Hex, defending: bool, stack: &[UnitId]) -> u32 {
        let (own, led, ground, walls) = self.strength_parts(unit, at, defending, stack);
        own.saturating_add(led).saturating_add(ground).saturating_mul(combat::QUARTER).saturating_add(walls)
    }

    /// What `battle_strength` is made of: the unit's own strength (with its
    /// level and blessing, `Unit::power`), the best
    /// bonus of a commander in its stack, the ground's bonus (a defender's),
    /// in points; and the walls' (a defender's in a city), in quarters.
    pub fn strength_parts(&self, unit: &Unit, at: Hex, defending: bool, stack: &[UnitId]) -> (u32, u32, u32, u32) {
        let units = self.map.units();
        let led = stack
            .iter()
            .filter(|id| **id != unit.id())
            .filter_map(|id| units.get(*id))
            .map(|u| self.command(u))
            .max()
            .unwrap_or(0);
        if !defending {
            return (unit.power(), led, 0, 0);
        }
        let ground = self.map.tile(at).map_or(0, |t| combat::ground_bonus(t.kind));
        let walls = self.map.city_at(at).map_or(0, |c| c.walls).saturating_mul(combat::WALL_QUARTERS);
        (unit.power(), led, ground, walls)
    }

    /// How much stronger a commander makes the others of its stack: what its
    /// kind says, and a point more at the highest level. 0 for a unit that is
    /// not one.
    pub fn command(&self, unit: &Unit) -> u32 {
        crate::state::command_bonus(self.map.units().kind_of(unit).map_or(0, |k| k.commander), unit.level)
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
        // How likely the attackers were to win (asked with a chance of its
        // own: the game's is not touched).
        let odds = self.battle_odds(attackers, at).unwrap_or(100);
        let battle = combat::fight(&a, &d, &self.setup.combat, &mut self.rng);
        // As it was, to be watched again.
        let defending: Vec<UnitId> = d.iter().map(|f| f.unit).collect();
        self.fought = Some(Fight {
            at,
            attacker: side,
            defender,
            rules: self.setup.combat,
            terrain: self.map.tile(at).map_or(TileType::Grass, |t| t.kind),
            walls: self.map.city_at(at).map_or(0, |c| c.walls),
            attackers: self.combatants(attackers, at, false),
            defenders: self.combatants(&defending, at, true),
            trace: battle.trace.clone(),
            won: battle.attackers_won,
        });
        let lost = |s: Side| battle.dead.iter().filter(|u| u.side == s).count();
        let units = self.map.units();
        let dead = battle
            .dead
            .iter()
            .filter_map(|id| units.get(*id))
            .map(|u| Fallen { side: u.side(), name: u.title(), person: u.given_name.is_some() })
            .collect();
        let event = Event::Battle {
            at,
            attacker: side,
            defender,
            attackers: a.len(),
            defenders: d.len(),
            attackers_lost: lost(side),
            defenders_lost: lost(defender),
            won: battle.attackers_won,
            dead,
        };
        for unit in &battle.dead {
            self.map.kill_unit(*unit);
        }
        // Those who lived through it won it: experience, a point more if the
        // odds were against them; and, if no commander led them and it was a
        // hard fight, maybe a captain from their ranks.
        let (winners, winner_odds): (Vec<UnitId>, u32) = if battle.attackers_won {
            (attackers.to_vec(), odds)
        } else {
            (d.iter().map(|f| f.unit).collect(), 100 - odds.min(100))
        };
        let living: Vec<UnitId> = winners.into_iter().filter(|id| self.map.units().get(*id).is_some()).collect();
        let worth = 1 + u32::from(winner_odds < HARD_ODDS);
        let mut after = Vec::new();
        for id in &living {
            let commander = self.map.is_commander(*id);
            let Some(unit) = self.map.unit_mut(*id) else { continue };
            if let Some(level) = unit.gain(worth) {
                after.push(Event::Promoted { unit: *id, side: unit.side(), name: unit.title(), level, commander });
            }
        }
        let led = living.iter().any(|id| self.map.is_commander(*id));
        self.emerging = None;
        if !led && !living.is_empty() && self.rng.below(100) < u64::from(emerge_chance(winner_odds)) {
            let side = if battle.attackers_won { side } else { defender };
            self.emerging = Some((side, at));
        }
        self.promoted = after;
        (battle.attackers_won, event)
    }

    /// A captain rises on `at` for `side` (`emerging`), if the map has a kind
    /// of commander and there is room for one: named, with no moves left this
    /// turn.
    fn emerge(&mut self, side: Side, at: Hex) -> Option<Event> {
        let kind = self.map.units().kinds().iter().position(|k| k.commander > 0)?;
        let id = self.map.new_unit_of(side, crate::state::UnitKindId(kind as u32))?;
        if self.map.place_unit(id, at).is_err() {
            self.map.kill_unit(id);
            return None;
        }
        let name = self.captain_name();
        let unit = self.map.unit_mut(id)?;
        unit.given_name = Some(name);
        unit.movement = 0;
        Some(Event::Emerged { unit: id, side, at, name: unit.title() })
    }

    /// The units of `ids` in a battle on `at`, as the battle sees them (the
    /// order of `fighters`).
    fn combatants(&self, ids: &[UnitId], at: Hex, defending: bool) -> Vec<Combatant> {
        let units = self.map.units();
        ids.iter()
            .filter_map(|id| units.get(*id))
            .map(|unit| {
                let (power, led, ground, walls) = self.strength_parts(unit, at, defending, ids);
                Combatant {
                    unit: unit.id(),
                    side: unit.side(),
                    name: unit.name.clone(),
                    title: unit.given_name.clone().unwrap_or_else(|| unit.name.clone()),
                    level: unit.level,
                    image: units
                        .kind_of(unit)
                        .map_or(crate::state::DEFAULT_UNIT_IMAGE, |k| k.image.as_str())
                        .to_string(),
                    commander: self.map.is_commander(unit.id()),
                    strength: unit.strength,
                    growth: power.saturating_sub(unit.strength),
                    led,
                    ground,
                    walls,
                    quarters: self.battle_strength(unit, at, defending, ids),
                }
            })
            .collect()
    }

    /// `by` takes a city from `from`: it starts its work over, and a side
    /// that has lost its last city is out.
    fn take(&mut self, city: CityId, from: Side, by: Side) -> Vec<Event> {
        if let Some(c) = self.map.city_mut(city) {
            // Where its old holder sent its new units is not the new one's.
            (c.side, c.rally) = (by, None);
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
        let round = self.round;
        self.fights.retain(|(_, fought, _)| fought + REPLAY_ROUNDS > round);
        let mut events = vec![Event::Began { side, round: self.round }];
        if self.round > 1 {
            self.map.reset_move_points(side);
            events.extend(self.pay_day(side));
            events.extend(self.marches(side));
        }
        events
    }

    /// The start of `side`'s turn: its cities' taxes come in; its units are
    /// paid their upkeep, and those it cannot pay leave it (as few as make up
    /// what it is short, a commander last); and what its cities finish is
    /// paid for, or waits until it can be.
    fn pay_day(&mut self, side: Side) -> Vec<Event> {
        let mut gold = self.gold(side).saturating_add(self.income(side));
        let mut events = Vec::new();
        loop {
            let owed = self.upkeep(side);
            if owed <= gold {
                gold -= owed;
                break;
            }
            let Some(id) = self.deserter(side, owed - gold) else {
                gold = 0;
                break;
            };
            if let Some(unit) = self.map.kill_unit(id) {
                events.push(Event::Deserted { side, name: unit.name });
            }
        }
        for unit in self.map.produce(side, &mut gold) {
            let city = self.map.units().get(unit).and_then(|u| self.map.city_at(u.pos())).map(|c| (c.id, c.rally));
            if let Some((city, rally)) = city {
                events.push(Event::Built { city, unit });
                // Its city sends it on.
                if let Some(new) = self.map.unit_mut(unit) {
                    new.goto = rally;
                }
            }
        }
        self.name_commanders();
        self.treasury[side.index()] = gold;
        events
    }

    /// Who leaves `side` when it is `short` of its upkeep: the least dear
    /// unit that makes it up alone, or else the dearest; a commander only
    /// when there is nobody else.
    fn deserter(&self, side: Side, short: u32) -> Option<UnitId> {
        let map = &self.map;
        let paid: Vec<&Unit> = map.units().of_side(side).filter(|u| u.upkeep > 0 && map.contains(u.pos())).collect();
        let (leaders, soldiers): (Vec<&Unit>, Vec<&Unit>) = paid.into_iter().partition(|u| map.is_commander(u.id()));
        let pick = |units: &[&Unit]| {
            let enough = units.iter().filter(|u| u.upkeep >= short).min_by_key(|u| u.upkeep);
            enough.or_else(|| units.iter().max_by_key(|u| u.upkeep)).map(|u| u.id())
        };
        pick(&soldiers).or_else(|| pick(&leaders))
    }

    /// What `side` sees: everything without fog of war; with it, what is
    /// within [`UNIT_SIGHT`] of its units and [`CITY_SIGHT`] of its cities.
    /// (Nobody, `Side::Grey`, sees everything.)
    pub fn sight(&self, side: Side) -> Sight {
        if !self.setup.fog || side == Side::Grey {
            return Sight::all();
        }
        let (rows, cols) = (self.map.rows(), self.map.cols());
        let mut seen = vec![false; usize::from(rows) * usize::from(cols)];
        let units = self.map.units().of_side(side).map(|u| (u.pos(), UNIT_SIGHT));
        let cities = self.map.cities().iter().filter(|c| c.side == side).map(|c| (c.pos, CITY_SIGHT));
        for (at, reach) in units.chain(cities).filter(|(at, _)| self.map.contains(*at)) {
            let r = reach as i32;
            for row in (i32::from(at.row) - r).max(0)..=(i32::from(at.row) + r).min(i32::from(rows) - 1) {
                for col in (i32::from(at.col) - r).max(0)..=(i32::from(at.col) + r).min(i32::from(cols) - 1) {
                    let hex = Hex::new(row as u16, col as u16);
                    if at.distance(hex) <= reach {
                        seen[row as usize * usize::from(cols) + col as usize] = true;
                    }
                }
            }
        }
        Sight { cols, seen: Some(seen) }
    }

    /// The game as `side` knows it, with fog of war: what it does not see is
    /// not in it (the other sides' units out of its sight; who holds a city
    /// in the fog: nobody, as far as it knows), nor what it is not told (what
    /// the other sides' cities build and where they send it, the other
    /// sides' gold, the history). What the computer's players are given, so
    /// that they know what a person in their place would. `None` without fog
    /// of war: then the whole game is seen.
    pub fn view_for(&self, side: Side) -> Option<Game> {
        if !self.setup.fog || side == Side::Grey {
            return None;
        }
        let sight = self.sight(side);
        let mut view = self.clone();
        (view.history, view.fights, view.fought, view.undo) = (Vec::new(), Vec::new(), None, None);
        view.deselect();
        let hidden: Vec<UnitId> =
            view.map.units().iter().filter(|u| u.side() != side && !sight.sees(u.pos())).map(|u| u.id()).collect();
        for id in hidden {
            view.map.kill_unit(id);
        }
        let theirs: Vec<(CityId, bool)> =
            view.map.cities().iter().filter(|c| c.side != side).map(|c| (c.id, sight.sees(c.pos))).collect();
        for (id, seen) in theirs {
            if let Some(city) = view.map.city_mut(id) {
                (city.building, city.progress, city.rally) = (None, 0, None);
                if !seen {
                    city.side = Side::Grey;
                }
            }
        }
        for (i, gold) in view.treasury.iter_mut().enumerate() {
            if i != side.index() {
                *gold = 0;
            }
        }
        Some(view)
    }

    /// The gold in a side's treasury.
    pub fn gold(&self, side: Side) -> u32 {
        self.treasury.get(side.index()).copied().unwrap_or(0)
    }

    /// What a side's cities bring in a turn: its taxes.
    pub fn income(&self, side: Side) -> u32 {
        self.map.cities().iter().filter(|c| c.side == side).map(|c| c.income).sum()
    }

    /// What a side's units cost a turn.
    pub fn upkeep(&self, side: Side) -> u32 {
        let map = &self.map;
        map.units().of_side(side).filter(|u| map.contains(u.pos())).map(|u| u.upkeep).sum()
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

    /// The selected units' reach as a side that sees `sight` knows it: other
    /// sides' units stop them where they are seen, and nowhere else (in the
    /// fog only cities do). What the map marks and the panels say, so that
    /// the fog tells nothing; the rules go by what is there.
    pub fn range_seen(&self, sight: &Sight) -> Option<MoveRange> {
        let mut range: Option<MoveRange> = None;
        for id in &self.selected {
            let unit = self.map.units().get(*id)?;
            let costs = self.map.units().move_cost(unit.move_cost)?;
            let side = unit.side();
            let pass = |hex: Hex| {
                if sight.sees(hex) {
                    passage(&self.map, side, hex)
                } else if self.map.city_at(hex).is_some_and(|c| c.side != side) {
                    Passage::Stop
                } else {
                    Passage::Open
                }
            };
            let one = reach_with(&self.map, unit.pos(), costs, unit.movement, &pass);
            range = Some(match range {
                Some(all) => all.both(&one),
                None => one,
            });
        }
        range
    }

    /// The next stack of `side` that can still go somewhere after the one on
    /// `from` (see [`Game::next_stack`]).
    pub fn next_stack_of(&self, side: Side, from: Option<Hex>, back: bool) -> Option<Hex> {
        self.stack_after(side, from, back)
    }

    /// The next city of `side` after the one on `from` (see [`Game::next_city`]).
    pub fn next_city_of(&self, side: Side, from: Hex, back: bool) -> Option<Hex> {
        self.city_after(side, from, back)
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
    /// - Units are selected and they cannot go there: they are deselected, and
    ///   the hex's own units, if it has any, are selected instead.
    /// - Nothing is selected and the hex has units of the side whose turn it
    ///   is: select them all, the stack (those with points left, if any have).
    ///
    /// C++: MapRenderer::selectHex. There a selected unit could go to any hex that
    /// the flood fill had not reached, mountains included, for free; here that
    /// drops the selection.
    pub fn select_hex(&mut self, hex: Hex) -> SelectOutcome {
        self.refresh();
        let Some(first) = self.selected() else { return self.select_stack(hex) };
        if self.map.units().get(first).is_some_and(|u| u.pos() == hex) {
            self.deselect();
            return SelectOutcome::Deselected;
        }
        let Some(cost) = self.range.as_ref().and_then(|r| r.cost(hex)) else {
            self.deselect();
            return match self.select_stack(hex) {
                SelectOutcome::Nothing => SelectOutcome::Deselected,
                selected => selected,
            };
        };
        let units = self.selected.clone();
        let done = self.apply(Order::Move { units, to: hex });
        self.deselect();
        match done {
            Ok(events) if events.iter().any(|e| matches!(e, Event::Battle { .. })) => SelectOutcome::Fought(events),
            Ok(events) => SelectOutcome::Moved { unit: first, cost, events },
            Err(_) => SelectOutcome::Deselected, // a full stack, for one
        }
    }

    /// Select the side's stack on a hex: those of its units with points left,
    /// or all of them if none has any.
    fn select_stack(&mut self, hex: Hex) -> SelectOutcome {
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
        match stack.first().copied() {
            Some(first) if self.select(stack) => SelectOutcome::Selected(first),
            _ => SelectOutcome::Nothing,
        }
    }

    /// The next hex after `from` (or before it, `back`; from nowhere, the
    /// first or the last), in reading order, with a stack of the side whose
    /// turn it is that can still go somewhere; if none can, with any of its
    /// stacks. `None` if it has no units.
    pub fn next_stack(&self, from: Option<Hex>, back: bool) -> Option<Hex> {
        self.stack_after(self.player(), from, back)
    }

    fn stack_after(&self, side: Side, from: Option<Hex>, back: bool) -> Option<Hex> {
        let map = &self.map;
        let mut hexes: Vec<Hex> = map.units().of_side(side).map(|u| u.pos()).filter(|h| map.contains(*h)).collect();
        hexes.sort_by_key(|h| (h.row, h.col));
        hexes.dedup();
        // A stack waits for orders if one of its units can still go
        // somewhere, its player is not done with it, and it is on no
        // standing order (or one that stalled).
        let can_go = |hex: &Hex| {
            map.units_at(*hex).iter().filter_map(|id| map.units().get(*id)).any(|u| {
                u.side() == side
                    && u.movement > 0
                    && !u.done
                    && (u.goto.is_none() || u.stalled)
                    && compute_range(map, u).reachable().next().is_some()
            })
        };
        let able: Vec<Hex> = hexes.iter().copied().filter(can_go).collect();
        next_in(if able.is_empty() { &hexes } else { &able }, from, back)
    }

    /// What `side` has: each of its stacks and cities, as a task (what it
    /// has left to do with it this turn), the most pressing first
    /// ([`Task::tier`]), then stacks before cities, then by place.
    pub fn tasks(&self, side: Side) -> Vec<Task> {
        let map = &self.map;
        let mut hexes: Vec<Hex> = map.units().of_side(side).map(|u| u.pos()).filter(|h| map.contains(*h)).collect();
        hexes.sort_by_key(|h| (h.row, h.col));
        hexes.dedup();
        let mut tasks: Vec<Task> = hexes
            .into_iter()
            .map(|at| {
                let units: Vec<&Unit> = map
                    .units_at(at)
                    .iter()
                    .filter_map(|id| map.units().get(*id))
                    .filter(|u| u.side() == side)
                    .collect();
                let can_go = |u: &&Unit| u.movement > 0 && compute_range(map, u).reachable().next().is_some();
                let free: Vec<&Unit> = units.iter().copied().filter(can_go).collect();
                let state = if free.iter().any(|u| u.goto.is_some() && u.stalled) {
                    StackState::Stalled
                } else if free.iter().any(|u| !u.moved && !u.done && u.goto.is_none()) {
                    StackState::Waiting
                } else if free.iter().any(|u| !u.done && u.goto.is_none()) {
                    StackState::MovesLeft
                } else if free.iter().any(|u| u.done) {
                    StackState::Done
                } else if units.iter().any(|u| u.goto.is_some()) {
                    StackState::Marching
                } else {
                    StackState::Spent
                };
                Task::Stack { at, units: units.len(), state }
            })
            .collect();
        for city in map.cities().iter().filter(|c| c.side == side && side != Side::Grey) {
            let state = match city.production() {
                None => CityState::Idle,
                Some(prod) if city.progress >= prod.time.max(1) => CityState::Waiting,
                Some(_) => CityState::Building,
            };
            tasks.push(Task::City { city: city.id, state });
        }
        let place = |task: &Task| match task {
            Task::Stack { at, .. } => (0, at.row, at.col),
            Task::City { city, .. } => {
                (1, map.cities().get(*city).map_or(0, |c| c.pos.row), map.cities().get(*city).map_or(0, |c| c.pos.col))
            }
        };
        tasks.sort_by_key(|t| (t.tier(), place(t)));
        tasks
    }

    /// A side's capital: its first city in the map's order (on the maps here,
    /// the one it starts with), or, with no city, where its first unit
    /// stands. In hotseat a person's turn starts there.
    pub fn capital(&self, side: Side) -> Option<Hex> {
        let city = self.map.cities().iter().find(|c| c.side == side && side != Side::Grey).map(|c| c.pos);
        city.or_else(|| self.map.units().of_side(side).map(|u| u.pos()).find(|h| self.map.contains(*h)))
    }

    /// The next city of the side whose turn it is after the one on `from` (or
    /// before it), in the map's order of cities. `None` if it has none.
    pub fn next_city(&self, from: Hex, back: bool) -> Option<Hex> {
        self.city_after(self.player(), from, back)
    }

    fn city_after(&self, side: Side, from: Hex, back: bool) -> Option<Hex> {
        let own: Vec<Hex> = self.map.cities().iter().filter(|c| c.side == side).map(|c| c.pos).collect();
        let here = own.iter().position(|h| *h == from);
        let n = own.len();
        let i = match (here, back) {
            (_, _) if n == 0 => return None,
            (Some(i), false) => (i + 1) % n,
            (Some(i), true) => (i + n - 1) % n,
            (None, false) => 0,
            (None, true) => n - 1,
        };
        own.get(i).copied()
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
        let moved = Event::Moved { unit: id, from: START, to: target, cost: 6 };
        assert_eq!(game.select_hex(target), SelectOutcome::Moved { unit: id, cost: 6, events: vec![moved] });
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
        // The C++ teleported to all three of these for free (deviation D2);
        // here a click out of reach drops the selection.
        for hex in [Hex::new(0, 0), Hex::new(51, 51), Hex::new(50, 70), Hex::new(100, 100)] {
            assert_eq!(game.select_hex(START), SelectOutcome::Selected(id));
            assert_eq!(game.select_hex(hex), SelectOutcome::Deselected, "{hex}");
            assert_eq!(game.selected(), None, "no longer selected");
            assert_eq!(game.map.units().get(id).unwrap().pos(), START);
            assert_eq!(game.map.units().get(id).unwrap().movement, 20);
        }
        // With nothing selected, a hex without units is nothing.
        assert_eq!(game.select_hex(Hex::new(0, 0)), SelectOutcome::Nothing);
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
    fn a_full_stack_refuses_the_move_and_drops_the_selection() {
        let mut game = grassland();
        let &[cav] = game.map.units_at(START) else { panic!() };
        let cost = game.map.units().get(cav).unwrap().move_cost;
        let target = Hex::new(50, 51);
        for i in 0..MAX_STACK {
            let id = game.map.new_unit(Side::Black, &format!("Guard {i}"), 1, 1, 0, cost);
            game.map.place_unit(id, target).unwrap();
        }
        game.select_hex(START);
        assert_eq!(game.select_hex(target), SelectOutcome::Deselected);
        assert_eq!(game.selected(), None);
        assert!(!game.can_undo(), "nothing happened");
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
        assert_eq!(game.select_hex(next_door), SelectOutcome::Deselected, "select_hex refreshes by itself");
        assert!(!game.can_move_to(next_door));
        assert!(game.select_unit(cav));

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
