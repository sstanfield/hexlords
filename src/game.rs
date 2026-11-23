//! The game: the map, whose turn it is, and the rules that carry out the
//! sides' orders (`orders`); and what the person at the screen has selected
//! and where it can go.
//!
//! C++: src/render/maprender.cpp MapRenderer::selectHex, moveUp, moveDown, and the
//! cursor keys of src/gui/mapwidget.cpp MapWidget::OnEvent

use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::orders::{Event, Fallen, Order, RALLY_POINTS, RallyPoint, Record, Step};
use crate::setup::{Controller, Fog, Setup, TurnOrder, WhenOut};
use crate::state::combat::{self, Fighter};
use crate::state::scenario::{self, MapSource, Scenario, side_name};
use crate::state::{ATTACK_COST, Blessing};
use crate::state::{
    City, CityId, Direction, Hex, MAX_QUEUE, MAX_STACK, Map, MoveCost, MoveCostId, MoveRange, NUM_SIDES, Passage,
    Queue, QueueEnd, QueueItem, Rng, Sends, Side, TileType, Travel, Unit, UnitId, compute_range, hostile, passage,
    path_with, reach_with, step_cost, step_toward_by, way_over_land,
};

mod patrol;
mod rounds;

pub use rounds::{BEAT, ROUND};

/// The agent that passes its turns: the computer's, when nothing else is said.
pub const PASSIVE: &str = "passive";

/// Why an order to land the side has never seen is refused (total fog of
/// war).
pub const UNSEEN: &str = "nobody of the side has seen that land yet";

/// What `Game::select_hex` did.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectOutcome {
    /// Nothing was selected and the hex had a unit: it is selected now.
    Selected(UnitId),
    /// The selected units are not the side's whose turn it is (selected in
    /// another's turn) and were sent nowhere: nothing is selected now.
    Deselected,
    /// The selected units attacked the hex: what happened (the battle, and
    /// the moves and the city taken if they won).
    Fought(Vec<Event>),
    /// The selected units moved to the hex (the first of them paid `cost`)
    /// and were deselected; what happened (the moves, a city taken).
    Moved { unit: UnitId, cost: u32, events: Vec<Event> },
    /// Nothing was selected, and the hex has none of the side's units.
    Nothing,
    /// The selected units could not reach the hex this turn, and were sent
    /// there (`Order::GoTo`, over as many turns as it takes): what came of
    /// it this turn. They are no longer selected.
    Sent(Vec<Event>),
    /// The selected units could not go there, now (a move refused: no room
    /// there, say) or over turns (they could not be sent there): why. They
    /// stay selected.
    Refused(String),
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
        taken: Option<CityBefore>,
        /// Where each was going, given back: its standing order, whether it
        /// had stalled, whether it had moved this turn.
        goals: Vec<(UnitId, MarchState)>,
    },
    /// A city was set to build something else: what it was building, how
    /// far, its queue and whether it rested.
    Built { city: CityId, building: Option<usize>, progress: u32, queue: Box<Queue>, resting: bool },
    /// A city's walls were raised a level, for `price`.
    Walls { city: CityId, price: u32 },
    /// Units were given a standing order and have not gone anywhere yet:
    /// what each was doing before.
    Ordered { goals: Vec<(UnitId, MarchState)> },
}

/// A city as it was before a move took it, for an undo: its id, holder,
/// work and how far it had got, walls and where it sent its units.
#[derive(Clone, Debug)]
struct CityBefore {
    id: CityId,
    side: Side,
    building: Option<usize>,
    progress: u32,
    walls: u32,
    sends: Sends,
    queue: Queue,
    resting: bool,
}

impl CityBefore {
    fn of(c: &City) -> CityBefore {
        CityBefore {
            id: c.id,
            side: c.side,
            building: c.building,
            progress: c.progress,
            walls: c.walls,
            sends: c.sends.clone(),
            queue: c.queue.clone(),
            resting: c.resting,
        }
    }
}

/// Something a side has left to do, or to look at, this turn (`Game::tasks`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Task {
    /// A stack of the side's, on its hex: how it stands, and how many of
    /// its units that is about (of five, three of which defend while two
    /// wait for orders: two).
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
    /// Its player moved it this turn, and it can go further: its player is
    /// taken to be done with it, as with Done.
    MovesLeft,
    /// Its player is done with it, though it could still go.
    Done,
    /// It is on its way somewhere, on a standing order.
    Marching,
    /// It defends where it stands (`Order::Defend`), until it is sent
    /// somewhere.
    Defending,
    /// It can go nowhere more this turn.
    Spent,
    /// It goes round waypoints (`Order::Patrol`), turn after turn.
    Patrolling,
    /// The computer leads it (`Order::Maraud`).
    Marauding,
    /// It fought on patrol, or marauding: it waits for its player's word
    /// (Done: it goes on, at once if it can; left alone, the next turn).
    Fought,
}

/// How a city stands this turn.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum CityState {
    /// It builds nothing, and its holder has not said that is what it wants
    /// (a city taken, a queue that ran out).
    Idle,
    /// It builds nothing, as its holder said (`Order::Build` with nothing).
    Resting,
    /// What it builds is done and waits: for the gold, or for room.
    Waiting,
    /// It is building.
    Building,
}

impl Task {
    /// How pressing it is: 1, what has had no orders (stacks that wait, or
    /// whose way is blocked, or that fought on patrol or marauding and wait
    /// for their player's word, and cities that build nothing); 2, what is held
    /// up (cities whose work waits for the gold or for room); 3, the rest,
    /// with the stacks its player is done with this turn: put aside, or
    /// moved (a stack moved and stopped has had its orders, moves left or
    /// not).
    pub fn tier(&self) -> u8 {
        match self {
            Task::Stack { state: StackState::Waiting | StackState::Stalled | StackState::Fought, .. } => 1,
            Task::City { state: CityState::Idle, .. } => 1,
            Task::City { state: CityState::Waiting, .. } => 2,
            _ => 3,
        }
    }
}

/// What a side remembers of another side's stack it saw, once it is out of
/// its sight (fog of war): where, whose, how many, and in which round it was
/// seen there last. It stays until that hex is in sight again.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Ghost {
    pub at: Hex,
    pub side: Side,
    pub count: usize,
    pub round: u32,
}

/// What a unit is doing about a standing order, for an undo: its route,
/// whether it has stalled, has moved this turn, defends, was put aside; its
/// patrol, whether it marauds, whether it fought on either; whether its
/// route is a round's plan.
#[derive(Clone, Debug)]
struct MarchState {
    route: Vec<Hex>,
    bound: Option<u8>,
    stalled: bool,
    moved: bool,
    defending: bool,
    done: bool,
    patrol: Option<crate::state::Patrol>,
    marauding: bool,
    fought: bool,
    planned: bool,
    party: u32,
    way: Vec<Hex>,
}

impl MarchState {
    fn of(unit: &Unit) -> MarchState {
        MarchState {
            route: unit.route.clone(),
            bound: unit.bound,
            stalled: unit.stalled,
            moved: unit.moved,
            defending: unit.defending,
            done: unit.done,
            patrol: unit.patrol.clone(),
            marauding: unit.marauding,
            fought: unit.fought,
            planned: unit.planned,
            party: unit.party,
            way: unit.way.clone(),
        }
    }

    fn restore(self, unit: &mut Unit) {
        (unit.route, unit.bound, unit.stalled, unit.moved, unit.defending, unit.done) =
            (self.route, self.bound, self.stalled, self.moved, self.defending, self.done);
        (unit.patrol, unit.marauding, unit.fought) = (self.patrol, self.marauding, self.fought);
        (unit.planned, unit.party, unit.way) = (self.planned, self.party, self.way);
    }
}

/// Units that go together (`Game::group`): those that move, each at the
/// prices of a table (its own, or a commander's with flyers, theirs), and
/// those carried by the ships among them, who pay nothing.
struct Group {
    movers: Vec<(UnitId, MoveCostId)>,
    carried: Vec<UnitId>,
}

impl Group {
    /// The one that moves with the fewest points left: the way is its, if
    /// they all go at one table's prices (`mixed`).
    fn lead<'a>(&self, map: &'a Map) -> Option<(&'a Unit, MoveCostId)> {
        let movers = self.movers.iter().filter_map(|(id, table)| map.units().get(*id).map(|u| (u, *table)));
        movers.min_by_key(|(u, _)| u.movement)
    }

    /// True if those that move go at different prices (flyers with units
    /// that walk, riders with rangers): the way of one is not the others',
    /// and they go a way all of them can (`Game::ground_together`).
    fn mixed(&self) -> bool {
        self.movers.windows(2).any(|pair| pair[0].1 != pair[1].1)
    }
}

/// What a hex is to one who goes, and what a step costs it, as closures a
/// way is found with (`Game::ground`, `Game::ground_together`).
type Ground<'a> = (Box<dyn Fn(Hex) -> Passage + 'a>, Box<dyn Fn(Hex, Hex) -> Option<u16> + 'a>);

/// How the steps of units that go together are weighed against each other,
/// to find the way of them all (`Game::ground_together`).
enum Weigh {
    /// This turn, with what each has left (in the movers' order): a step
    /// weighs the largest share it takes of what one of them has left, of
    /// [`SHARE`]; a way that weighs no more than [`SHARE`] all of them can
    /// pay for.
    Now(Vec<u32>),
    /// However far: a step weighs the time it takes the slowest of them
    /// (its cost over its movement a turn; not a commander's among others,
    /// who keeps up).
    Far,
}

/// All of what a unit has left, as `Weigh::Now` counts (every number up to
/// 12 divides it, and most up to 36).
const SHARE: u32 = 55_440;

/// A turn's movement, as `Weigh::Far` counts the time a step takes.
const PACE: u32 = 720;

/// How far a side sees, with fog of war: from its units, and from its cities.
pub const UNIT_SIGHT: u32 = 2;
pub const CITY_SIGHT: u32 = 3;

/// What a side knows of the map (`Game::sight`): what it sees now (every
/// hex, with no fog of war; else those near its units and cities); with total
/// fog, the land it has ever seen; and what it remembers of the rest: the
/// other sides' stacks (`Ghost`), and who held each city when it saw it
/// last.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Sight {
    cols: u16,
    /// Row-major; `None`: everything.
    seen: Option<Vec<bool>>,
    /// Row-major; `None`: everything (no total fog).
    known: Option<Vec<bool>>,
    ghosts: Vec<Ghost>,
    /// Where each city it has seen is, who held it then, the round, and its
    /// walls then.
    held: Vec<(Hex, Side, u32, u32)>,
}

/// Sight of everything, to lend (`Sight::ALL`).
pub static SEE_ALL: Sight = Sight::ALL;

impl Sight {
    /// Sight of everything.
    pub const ALL: Sight = Sight { cols: 0, seen: None, known: None, ghosts: Vec::new(), held: Vec::new() };

    /// Sight of everything.
    pub fn all() -> Sight {
        Sight::ALL
    }

    /// A sight made by hand (a test's, say): of a map of `map`'s size, the
    /// hexes `seen` says are seen, those `known` says are known (`None`: all
    /// of them), and the stacks remembered.
    pub fn by_hand(
        map: &Map,
        seen: &dyn Fn(Hex) -> bool,
        known: Option<&dyn Fn(Hex) -> bool>,
        ghosts: Vec<Ghost>,
    ) -> Sight {
        let hexes = || (0..map.rows()).flat_map(|row| (0..map.cols()).map(move |col| Hex::new(row, col)));
        Sight {
            cols: map.cols(),
            seen: Some(hexes().map(seen).collect()),
            known: known.map(|known| hexes().map(known).collect()),
            ghosts,
            held: Vec::new(),
        }
    }

    /// What a screen shows while a move is shown going, from `self` (what
    /// was known before it) to `after` (what is known now): `after`, but a
    /// hex it sees or knows that `self` did not only once `open` says so
    /// (the stack has come near enough); and what `near` says is seen (it
    /// is in sight of a stack on its way, where that is shown now: what a
    /// stack sees goes with it). A stack remembered now that was not
    /// before shows once its hex has been seen.
    pub fn opening(&self, after: &Sight, open: &dyn Fn(Hex) -> bool, near: &dyn Fn(Hex) -> bool) -> Sight {
        let cols = usize::from(after.cols.max(1));
        let hex = |i: usize| Hex::new((i / cols) as u16, (i % cols) as u16);
        let was = |then: &Option<Vec<bool>>, i: usize| then.as_ref().is_none_or(|t| t.get(i).copied().unwrap_or(false));
        let seen = after.seen.as_ref().map(|now| {
            let shown = |(i, seen): (usize, &bool)| near(hex(i)) || *seen && (was(&self.seen, i) || open(hex(i)));
            now.iter().enumerate().map(shown).collect()
        });
        let known = after.known.as_ref().map(|now| {
            let shown = |(i, known): (usize, &bool)| *known && (was(&self.known, i) || open(hex(i)));
            now.iter().enumerate().map(shown).collect()
        });
        let noted = |g: &&Ghost| self.ghost_at(g.at).is_some() || self.sees(g.at) || open(g.at) || near(g.at);
        let ghosts = after.ghosts.iter().filter(noted).copied().collect();
        Sight { seen, known, ghosts, ..after.clone() }
    }

    fn at(&self, hex: Hex) -> usize {
        usize::from(hex.row) * usize::from(self.cols) + usize::from(hex.col)
    }

    /// True if the hex is seen: who stands there, who holds its city.
    pub fn sees(&self, hex: Hex) -> bool {
        match &self.seen {
            None => true,
            Some(seen) => hex.col < self.cols && seen.get(self.at(hex)) == Some(&true),
        }
    }

    /// True if the side knows the hex: its terrain, its road, the city on it.
    /// Every hex, but with total fog of war: then those it has ever seen.
    pub fn knows(&self, hex: Hex) -> bool {
        match &self.known {
            None => true,
            Some(known) => hex.col < self.cols && known.get(self.at(hex)) == Some(&true),
        }
    }

    /// How many hexes it knows (`None`: all of them): it only grows.
    pub fn known_count(&self) -> Option<usize> {
        self.known.as_ref().map(|known| known.iter().filter(|k| **k).count())
    }

    /// Who holds `city` as far as the side knows: in sight, who does; in the
    /// fog, who did when it saw the city last (nobody, if it never did).
    pub fn holder(&self, city: &City) -> Side {
        if self.sees(city.pos) { city.side } else { self.held(city.pos).map_or(Side::Grey, |(side, _)| side) }
    }

    /// Who held the city on `hex` when the side saw it last, and in which
    /// round.
    pub fn held(&self, hex: Hex) -> Option<(Side, u32)> {
        self.held.iter().find(|(at, ..)| *at == hex).map(|(_, side, round, _)| (*side, *round))
    }

    /// The walls of the city on `hex` when the side saw it last (they fall
    /// when it is taken, and rise when its holder raises them).
    pub fn walls_seen(&self, hex: Hex) -> Option<u32> {
        self.held.iter().find(|(at, ..)| *at == hex).map(|(.., walls)| *walls)
    }

    /// The other sides' stacks it remembers where it saw them last: those in
    /// its sight as they are now, the others as they were.
    pub fn ghosts(&self) -> &[Ghost] {
        &self.ghosts
    }

    /// The stack it remembers on a hex out of its sight, if any: what shows
    /// there in the fog.
    pub fn ghost_at(&self, hex: Hex) -> Option<&Ghost> {
        if self.sees(hex) {
            return None;
        }
        self.ghosts.iter().find(|g| g.at == hex)
    }

    /// The hexes it knows next to land it has never seen (total fog of
    /// war): where it goes to see more. None without total fog.
    pub fn frontier(&self, map: &Map) -> Vec<Hex> {
        if self.known.is_none() {
            return Vec::new();
        }
        let hexes = (0..map.rows()).flat_map(|row| (0..map.cols()).map(move |col| Hex::new(row, col)));
        hexes.filter(|h| self.knows(*h) && map.neighbours(*h).into_iter().any(|n| !self.knows(n))).collect()
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
    /// Its kind's bonus on this ground (`UnitKind::bonus`).
    pub home: u32,
    /// The walls' bonus, in quarters (a defender's in a city), less what
    /// the attackers' siege engines broke.
    pub walls: u32,
    /// All of it, in quarters: what it fought with (no more than the
    /// game's cap, `combat::Rules::cap`).
    pub quarters: u32,
    /// The hits it came with (wounds that last, `combat::Rules::wounds`),
    /// and what it fights with once wounded, in quarters (as `quarters`
    /// unless wounds last).
    pub wounds: u32,
    pub hurt: u32,
}

/// What a unit's strength in a battle is made of (`Game::strength_parts`):
/// its own (`Unit::power`: its kind's, its level, its blessings), the best
/// bonus of a commander in its stack, its kind's bonus on the battle's
/// ground (`UnitKind::bonus`), and, a defender's, the ground's bonus, in
/// points; and the walls' (a defender's in a city, less what the
/// attackers' siege engines break), in quarters.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct StrengthParts {
    pub own: u32,
    pub led: u32,
    pub home: u32,
    pub ground: u32,
    pub walls: u32,
}

impl StrengthParts {
    /// All of it, in quarters of a point (`combat::QUARTER`).
    pub fn quarters(&self) -> u32 {
        let points = self.own.saturating_add(self.led).saturating_add(self.home).saturating_add(self.ground);
        points.saturating_mul(combat::QUARTER).saturating_add(self.walls)
    }
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
    /// How many points of the walls the attackers' siege engines broke.
    pub siege: u32,
    pub attackers: Vec<Combatant>,
    pub defenders: Vec<Combatant>,
    pub trace: Vec<combat::Exchange>,
    pub won: bool,
}

/// True if `unit`'s orders have stopped and it waits for its player's
/// word: a march stopped short, a patrol or marauders that fought, a patrol
/// that could not go on (it waits for orders: `Game::waiting_stacks`).
pub fn waits_for_word(unit: &Unit) -> bool {
    patrol::needs_word(unit) || !unit.route.is_empty() && unit.stalled
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
    /// In rounds: the selected units go on from where their plan for the
    /// round ends (`select_onward`): `range` is from there, and the hex
    /// chosen next is a waypoint more (`Order::Onward`), not a new plan.
    onward: bool,
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
    fought: Vec<Fight>,
    /// What a screen shows happening (`Game::watch`): every event of the
    /// orders carried out since it last took them, each with its record in
    /// the history if it has one; `None` while nobody watches (without a
    /// window, nothing is kept).
    watched: Option<Vec<(Option<usize>, Event)>>,
    /// After a battle won by a stack with no commander against the odds:
    /// its side, and the hex its captain would rise on.
    emerging: Option<(Side, Hex)>,
    /// The levels the last battle brought, to be told after it.
    promoted: Vec<Event>,
    /// With fog of war, each side's (by `Side::index`): the hexes its units
    /// saw on their way in the order being carried out (seen until it has
    /// been: what was there is noted, `note_sightings`, and they are seen
    /// no longer), and the other sides' stacks it saw last where they were
    /// (`Ghost`).
    trail: Vec<Vec<bool>>,
    memory: Vec<Vec<Ghost>>,
    /// With fog of war, each side's: who held each city it has seen, when it
    /// saw it last (`Sight::held`); with total fog, the hexes it has ever
    /// seen.
    held: Vec<Vec<(Hex, Side, u32, u32)>>,
    explored: Vec<Vec<bool>>,
    /// The cities whose walls were raised this turn (once a turn each).
    raised: Vec<CityId>,
    /// The turn (round, side) whose units left alone after a battle were
    /// last let go on (`stand_easy`: once a turn).
    eased: Option<(u32, Side)>,
    /// Each side's rally points (by `Side::index`), 0 to 9.
    rally_points: Vec<[Option<RallyPoint>; RALLY_POINTS]>,
    /// The sides that are out, in the order they went.
    out: Vec<Side>,
    winner: Option<Side>,
    /// In rounds, while a round plays out (`play_out`): how far it has got;
    /// and how many plans have been made this round (`Unit::party`).
    playing: Option<rounds::PlayOut>,
    parties: u32,
    /// What was done to the game since it was last taken (`take_steps`),
    /// while it is kept (`record`): the orders carried out, the beats of a
    /// round playing out. With the game as it was before them, they are
    /// what happened since (a seed and a set of orders always make the
    /// same game). `None`: not kept.
    pub(crate) journal: Option<Vec<Step>>,
    /// The texts the map was read from, if it was (`with_source`): what a
    /// saved game keeps of it.
    source: Option<Rc<MapSource>>,
    /// It is a copy of the game as it was (`freeze`): it takes no orders.
    frozen: bool,
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
            onward: false,
            assets: None,
            order,
            current: 0,
            round: 1,
            rng,
            raised: Vec::new(),
            eased: None,
            rally_points: vec![[None; RALLY_POINTS]; NUM_SIDES + 1],
            out: Vec::new(),
            trail: vec![Vec::new(); NUM_SIDES + 1],
            memory: vec![Vec::new(); NUM_SIDES + 1],
            held: vec![Vec::new(); NUM_SIDES + 1],
            explored: vec![Vec::new(); NUM_SIDES + 1],
            winner: None,
            playing: None,
            parties: 0,
            journal: None,
            source: None,
            frozen: false,
            undo: None,
            undo_mark: 0,
            history: Vec::new(),
            treasury: [setup.gold; NUM_SIDES + 1],
            fights: Vec::new(),
            fought: Vec::new(),
            watched: None,
            emerging: None,
            promoted: Vec::new(),
            setup,
        };
        game.name_commanders();
        // What each side sees at the start, it knows.
        game.note_sightings();
        game
    }

    /// The side whose turn it is (on a map without sides, nobody's): in
    /// rounds, the side that gives its orders now, and nobody while the
    /// round plays out (`playing_out`).
    pub fn player(&self) -> Side {
        if self.playing.is_some() {
            return Side::Grey;
        }
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
        let (scenario, source) = scenario::load_with_source(path).map_err(|e| e.to_string())?;
        let warnings = scenario.warnings.clone();
        Ok((Game::from_scenario(scenario).with_source(source), warnings))
    }

    /// The same game started over with another setup (the new game screen's).
    pub fn with_setup(self, setup: Setup) -> Game {
        let Game { map, assets, source, .. } = self;
        Game { assets, source, ..Game::start(map, setup) }
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
    /// both come here. (In rounds, an order to move is a plan, carried out
    /// when the round plays out: `game::rounds`.)
    pub fn apply(&mut self, order: Order) -> Result<Vec<Event>, String> {
        if self.frozen {
            return Err("that is how it was: orders are given in the game as it is now".to_string());
        }
        // (Kept, if it is carried out: `record`.)
        let kept = self.journal.is_some().then(|| order.clone());
        let done = self.obey(order);
        if let (Ok(_), Some(order), Some(journal)) = (&done, kept, &mut self.journal) {
            journal.push(Step::Order(order));
        }
        done
    }

    /// Do again what was done (`record`): an order is carried out, a round
    /// goes on a beat. To the game as it was then, it does the same.
    pub fn redo(&mut self, step: &Step) -> Result<Vec<Event>, String> {
        match step {
            Step::Order(order) => self.apply(order.clone()),
            Step::Beat if self.playing_out() => Ok(self.play_out()),
            Step::Beat => Err("no round is playing out".to_string()),
            Step::Ease(side) => {
                self.stand_easy(*side);
                Ok(Vec::new())
            }
        }
    }

    fn obey(&mut self, order: Order) -> Result<Vec<Event>, String> {
        if let Some(side) = self.winner {
            return Err(format!("the game is over: {} won", side_name(side)));
        }
        if self.playing.is_some() {
            return Err("the round is playing out".to_string());
        }
        let side = self.player();
        let last = self.undo.take();
        if order == Order::Undo {
            return self.take_back(side, last);
        }
        // In rounds, a side that has done gives way to the next, or the
        // round plays out: it keeps what happens itself, side by side.
        if order == Order::EndTurn && self.rounds() {
            self.fought.clear();
            return Ok(self.end_planning());
        }
        let (round, mark) = (self.round, self.history.len());
        // Being done with units changes nothing to take back: the order
        // before can still be (unless it sent a patrol on: `Order::Done`);
        // nor does Defend, unless it ends a standing order (a march, a
        // patrol, marauding), which an undo of the order before would not
        // give back. Marauding is a standing order.
        let ends_orders = |units: &[UnitId]| {
            units.iter().filter_map(|id| self.map.units().get(*id)).any(|u| !u.route.is_empty() || u.on_duty())
        };
        let keeps = match &order {
            Order::Done { .. } => true,
            Order::Defend { units, .. } => !ends_orders(units),
            _ => false,
        };
        // (The fights an order's battles leave, in their order.)
        self.fought.clear();
        let done = self.carry_out(side, order);
        if keeps && done.as_ref().is_ok_and(|events| events.is_empty()) {
            self.undo = last;
            return done;
        }
        match &done {
            Ok(events) => {
                self.remember(round, side, events);
                self.undo_mark = mark;
                self.note_sightings();
            }
            Err(_) => self.undo = last, // nothing changed: it can still be taken back
        }
        done
    }

    /// Keep what an order of `turn`'s, in `round`, did that is worth keeping:
    /// not the moves (there are too many, and they say little afterwards), the
    /// turns' beginnings, or an undo.
    /// (What comes before a turn's beginning, in the same order, is the
    /// turn that ends: its marches and patrols, and what they take; what
    /// comes after it is the new turn's: the units built, the deserters.)
    fn remember(&mut self, round: u32, turn: Side, events: &[Event]) {
        let (mut round, mut turn) = (round, turn);
        for event in events {
            if let Event::Began { side, round: began } = event {
                (round, turn) = (*began, *side);
            }
            let kept = !matches!(event, Event::Moved { .. } | Event::Began { .. } | Event::Undone);
            let record = self.history.len();
            if let Some(watched) = &mut self.watched {
                watched.push((kept.then_some(record), event.clone()));
            }
            if !kept {
                continue;
            }
            // A battle's fight is kept beside its record.
            if matches!(event, Event::Battle { .. }) && !self.fought.is_empty() {
                let fight = self.fought.remove(0);
                self.fights.push((self.history.len(), round, fight));
            }
            self.history.push(Record { round, turn, event: event.clone() });
        }
    }

    /// Keep what happens, for a screen to show it as it happens
    /// (`take_happened`), or no longer. (It changes nothing in the game.)
    pub fn watch(&mut self, on: bool) {
        if on != self.watched.is_some() {
            self.watched = on.then(Vec::new);
        }
    }

    /// Keep what is done to the game from now on (`take_steps`), or no
    /// longer: the orders carried out, and in rounds each beat of a round
    /// playing out. With a copy of the game as it is now, they are a record
    /// of what happens next: done again on the copy in their order
    /// (`redo`), they make the same game again. For a screen that shows a
    /// turn again, and for a saved game (`save`). (While they are kept, a
    /// round does not play out within the last side's `EndTurn`: whoever
    /// keeps them plays it, `play_out`, so that each beat is one of them.)
    pub fn record(&mut self, on: bool) {
        if on != self.journal.is_some() {
            self.journal = on.then(Vec::new);
        }
    }

    /// What was done since this was last asked, while it is kept
    /// (`record`), in its order.
    pub fn take_steps(&mut self) -> Vec<Step> {
        self.journal.as_mut().map(std::mem::take).unwrap_or_default()
    }

    /// The same game, which knows the texts its map was read from.
    pub fn with_source(mut self, source: MapSource) -> Game {
        self.source = Some(Rc::new(source));
        self
    }

    /// The texts the map was read from, if the game knows them.
    pub fn source(&self) -> Option<&MapSource> {
        self.source.as_deref()
    }

    /// The game on the map these texts are (and the map's own pictures, if
    /// it has some): as `load` starts one.
    pub fn from_source(source: MapSource, assets: Option<PathBuf>) -> Result<Game, String> {
        let (map, warnings) = scenario::parse(&source.terrain, source.units.as_deref(), source.cities.as_deref())
            .map_err(|e| e.to_string())?;
        Ok(Game::from_scenario(Scenario { map, assets, warnings }).with_source(source))
    }

    /// A number that says how the game stands: whose turn in which round,
    /// the game's chance, every unit and city, the treasuries. Two games
    /// that went the same way have the same; a saved game keeps it, to know
    /// that it was made again as it was (`save`).
    pub fn digest(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        let mut add = |n: u64| {
            for byte in n.to_le_bytes() {
                hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
            }
        };
        add(u64::from(self.round));
        add(self.player().index() as u64);
        add(u64::from(self.playing.is_some()));
        add(self.rng.state());
        add(self.winner.map_or(99, |side| side.index() as u64));
        for gold in self.treasury {
            add(u64::from(gold));
        }
        for unit in self.map.units().iter() {
            let id = unit.id();
            for n in [id.side.index() as u32, id.index, id.generation, unit.movement, unit.wounds] {
                add(u64::from(n));
            }
            add(u64::from(unit.pos().row) << 16 | u64::from(unit.pos().col));
            add(u64::from(unit.level));
            // (What it is about: a march, a patrol, marauding, waiting.)
            let flags = [unit.fought, unit.marauding, unit.done, unit.stalled, unit.patrol.is_some()];
            add(flags.iter().fold(0, |bits, on| bits << 1 | u64::from(*on)));
            add(unit.route.len() as u64);
        }
        for city in self.map.cities().iter() {
            add(city.side.index() as u64);
            add(u64::from(city.walls));
            add(u64::from(city.progress));
        }
        hash
    }

    /// The game is a copy of how it was (or no longer): it takes no orders
    /// (`apply` refuses them), though it can still be looked at, selections
    /// and all. A round that was playing out in it can still be played on
    /// (`play_out`): that is no order.
    pub fn freeze(&mut self, on: bool) {
        self.frozen = on;
    }

    /// True if the game is a copy of how it was (`freeze`).
    pub fn frozen(&self) -> bool {
        self.frozen
    }

    /// What has happened since this was last asked, while watched (`watch`):
    /// the events of each order carried out, in their order, each with its
    /// record in the history (`history`) if it has one.
    pub fn take_happened(&mut self) -> Vec<(Option<usize>, Event)> {
        self.watched.as_mut().map(std::mem::take).unwrap_or_default()
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
            Order::Move { units, to } if self.rounds() => self.plan(side, &units, to, true, false),
            Order::Toward { units, to } if self.rounds() => self.plan(side, &units, to, false, false),
            Order::Roam { units, to } if self.rounds() => self.plan(side, &units, to, true, true),
            Order::Onward { units, to } if self.rounds() => self.plan_onward(side, &units, to),
            Order::Move { units, to } | Order::Onward { units, to } => self.move_units(side, &units, to),
            Order::Toward { units, to } => {
                // The way of the slowest; whoever can get as far comes along
                // (all or none, of those that ships carry).
                for id in &units {
                    self.own_unit(side, *id)?;
                }
                let group = self.group(&units)?;
                let (lead, table) = group.lead(&self.map).ok_or("no units")?;
                // As far as there is room for all of them, on the way the side
                // knows.
                let sight = self.sight(side);
                if !sight.knows(to) {
                    return Err(UNSEEN.to_string());
                }
                let room = |hex: Hex| self.fits(hex, &units);
                let hex = if group.mixed() {
                    // (A way all of them can go, as far as all can pay.)
                    let way = self.way_together(&group, to, &sight).ok_or("there is no way there")?;
                    let near = |(hex, cost): &(Hex, u32)| {
                        *cost <= lead.movement && (hostile(&self.map, side, *hex) || room(*hex))
                    };
                    way.into_iter().rev().find(near).map(|(hex, _)| hex)
                } else {
                    self.known_step_toward_by(lead, table, (to, &room), &sight).map(|(hex, _)| hex)
                };
                let hex = hex.ok_or("there is no way there")?;
                let coming = self.coming(&group, hex, &sight);
                if coming.is_empty() {
                    return Err("they cannot all get that far".to_string());
                }
                self.move_units(side, &coming, hex)
            }
            Order::Build { city, what } => {
                let holder = self.map.cities().get(city).map(|c| c.side).ok_or("there is no such city")?;
                if holder != side {
                    return Err(format!("only {} sets what it builds", side_name(holder)));
                }
                let c = self.map.cities().get(city).ok_or("there is no such city")?;
                let (building, progress, queue, resting) = (c.building, c.progress, c.queue.clone(), c.resting);
                if !self.map.set_production(city, what) {
                    return Err("it cannot build that".to_string());
                }
                if let Some(c) = self.map.city_mut(city) {
                    (c.queue, c.resting) = (Queue::default(), what.is_none());
                }
                self.undo = Some(Undo::Built { city, building, progress, queue: Box::new(queue), resting });
                Ok(Vec::new())
            }
            Order::EndTurn => Ok(self.next_turn()),
            Order::Bless { units } => self.bless(side, &units),
            Order::Walls { city } => {
                let price = self.walls_price(side, city)?;
                let walls = match self.map.city_mut(city) {
                    Some(c) => {
                        c.walls += 1;
                        c.walls
                    }
                    None => return Err("there is no such city".to_string()),
                };
                self.treasury[side.index()] -= price;
                self.raised.push(city);
                self.undo = Some(Undo::Walls { city, price });
                Ok(vec![Event::WallsRaised { city, side, walls, price }])
            }
            Order::Disband { units } => self.disband(side, &units),
            Order::Undo => Err("there is nothing to undo".to_string()),
            Order::GoTo { units, to, via } => {
                if !self.map.contains(to) {
                    return Err(format!("{to} is not on the map"));
                }
                let before: Vec<(UnitId, MarchState)> = units
                    .iter()
                    .map(|id| self.own_unit(side, *id).map(|u| (*id, MarchState::of(u))))
                    .collect::<Result<_, _>>()?;
                if units.is_empty() {
                    return Err("no units".to_string());
                }
                let route: Vec<Hex> = via.into_iter().chain([to]).collect();
                self.check_route(side, &units, &route)?;
                // To a rally point: bound to it.
                // (Not with another side's units on it: a march stops short
                // of them, and waits for orders.)
                let bound = self.rally_target(side, to);
                for id in &units {
                    if let Some(unit) = self.map.unit_mut(*id) {
                        (unit.route, unit.bound, unit.stalled, unit.defending) = (route.clone(), bound, false, false);
                        unit.stand_down();
                    }
                }
                // In rounds they go as the round plays out.
                if self.rounds() {
                    self.undo = Some(Undo::Ordered { goals: before });
                    return Ok(Vec::new());
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
                    events.extend(self.march(side, ids));
                }
                // An undo gives the units back what they were doing before (a
                // stack from one hex: units from several cannot be taken back);
                // an order that has moved nobody yet is taken back whole.
                let still = events.iter().all(|e| matches!(e, Event::Stalled { .. }));
                match &mut self.undo {
                    Some(Undo::Moved { goals, .. }) if groups.len() == 1 => *goals = before,
                    None if still => self.undo = Some(Undo::Ordered { goals: before }),
                    _ => self.undo = None,
                }
                Ok(events)
            }
            Order::Defend { units, on } => {
                for id in &units {
                    self.own_unit(side, *id)?;
                }
                if units.is_empty() {
                    return Err("no units".to_string());
                }
                for id in &units {
                    if let Some(unit) = self.map.unit_mut(*id) {
                        unit.defending = on;
                        // (A round's plan ends with it: the unit has no
                        // orders again.)
                        if unit.planned && !on {
                            unit.moved = false;
                        }
                        if on || unit.planned {
                            (unit.route, unit.bound, unit.stalled) = (Vec::new(), None, false);
                            unit.stand_down();
                        }
                    }
                }
                Ok(Vec::new())
            }
            Order::Done { units } => {
                for id in &units {
                    self.own_unit(side, *id)?;
                }
                // (A patrol that fought and waited for its player's word goes
                // on at once.)
                let events = self.carry_on(side, &units);
                for id in &units {
                    if let Some(unit) = self.map.unit_mut(*id) {
                        unit.done = true;
                    }
                }
                Ok(events)
            }
            Order::Rally { city, to } => {
                match &to {
                    Sends::Stay | Sends::Defend => self.check_rally(side, city, &[])?,
                    Sends::Route(route) if route.is_empty() => return Err("a route to nowhere".to_string()),
                    Sends::Route(route) => self.check_rally(side, city, route)?,
                    Sends::Point { n, .. } => {
                        let point = self.rally_point(side, *n).ok_or(format!("rally point {n} is not set"))?;
                        self.check_rally(side, city, &[point.at])?;
                    }
                }
                if let Some(c) = self.map.city_mut(city) {
                    // (With a queue, the item it builds now.)
                    let at = c.queue.at;
                    if let Some(item) = c.queue.items.get_mut(at) {
                        item.sends = to.clone();
                    }
                    c.sends = to;
                }
                Ok(Vec::new())
            }
            Order::Queue { city, items, end } => self.set_queue(side, city, items, end),
            Order::RallyPoint { n, point } => self.set_rally_point(side, n, point),
            Order::Patrol { units, waypoints, attack } => self.patrol(side, &units, &waypoints, attack),
            Order::Maraud { units, on } => self.maraud(side, &units, on),
            Order::Roam { units, to } => self.roam(side, &units, to),
        }
    }

    /// Units of `side` on a standing order (a stack: those of them on the
    /// first one's hex) go along their route as far as they can this turn,
    /// a leg at a time: each step as far as the slowest of them gets, where
    /// there is room for all of them, and those that can get as far go. They
    /// do not attack: a step into another side's units, or no way at all,
    /// stalls them (they keep their order, and wait for their player). At a
    /// stop they go on to the next with what movement they have left; at a
    /// stop on a temple, those it has not blessed seek its blessing and stop
    /// there for the turn. At the last stop their order ends. More than one
    /// leg, or a blessing, is not the player's to take back.
    fn march(&mut self, side: Side, ids: &[UnitId]) -> Vec<Event> {
        let mut events = Vec::new();
        let mut ids = ids.to_vec();
        let mut legs = 0;
        loop {
            let units: Vec<&Unit> = ids.iter().filter_map(|id| self.map.units().get(*id)).collect();
            let Some(first) = units.first() else { break };
            let (at, mut route, bound) = (first.pos(), first.route.clone(), first.bound);
            ids = units.iter().filter(|u| u.pos() == at).map(|u| u.id()).collect();
            // On the last leg to a rally point: to the point, or beside it
            // once it is full.
            if let (Some(n), [end]) = (bound, route.as_slice()) {
                let spot = self.rally_spot(side, n, &ids, at).unwrap_or(*end);
                if spot != *end {
                    route = vec![spot];
                    for id in &ids {
                        if let Some(unit) = self.map.unit_mut(*id) {
                            unit.route = route.clone();
                        }
                    }
                }
            }
            let Some(to) = route.first().copied() else { break };
            if at == to {
                // A stop: on to the next (none: there, and the order ends;
                // at a rally point that says so, they defend there).
                let defend = bound.and_then(|n| self.rally_point(side, n)).is_some_and(|p| p.defend);
                let last = route.len() == 1;
                for id in &ids {
                    if let Some(unit) = self.map.unit_mut(*id) {
                        (unit.route, unit.stalled) = (route[1..].to_vec(), false);
                        if last {
                            unit.bound = None;
                            unit.defending |= defend && bound.is_some();
                        }
                    }
                }
                if self.map.tile(at).is_some_and(|t| t.kind == TileType::Temple) {
                    let blessed = self.blessings(at, &ids);
                    if !blessed.is_empty() {
                        events.extend(blessed);
                        self.undo = None;
                        break;
                    }
                }
                continue;
            }
            let group = self.group(&ids);
            let Some((lead, _)) = group.as_ref().ok().and_then(|g| g.lead(&self.map)) else {
                events.extend(self.stall(side, ids, at, to));
                break;
            };
            if lead.movement == 0 {
                break;
            }
            // This turn's step: the farthest along the way, as the side
            // knows it, that all of them can pay for and that has room for
            // all of them; never into another side's units it sees (one it
            // does not is met).
            let sight = self.sight(side);
            let room = |hex: Hex| self.fits(hex, &ids);
            let way = group.as_ref().ok().and_then(|group| self.way_together(group, to, &sight));
            // Too few points left for the first step: that is no stop, it
            // goes on next time.
            if way.as_ref().and_then(|w| w.first()).is_some_and(|(_, cost)| *cost > lead.movement) {
                break;
            }
            let step = way
                .and_then(|way| way.into_iter().rev().find(|(hex, cost)| *cost <= lead.movement && room(*hex)))
                .filter(|(hex, _)| !(sight.sees(*hex) && hostile(&self.map, side, *hex)));
            let coming: Vec<UnitId> = match (step, &group) {
                (Some((hex, _)), Ok(group)) => self.coming(group, hex, &sight),
                _ => Vec::new(),
            };
            let moved = match step {
                Some((hex, _)) if !coming.is_empty() => self.move_units(side, &coming, hex).ok().map(|e| (hex, e)),
                _ => None,
            };
            let Some((hex, moved)) = moved else {
                events.extend(self.stall(side, ids, at, to));
                break;
            };
            events.extend(moved);
            legs += 1;
            // The move ended their order: they keep it.
            for id in &coming {
                if let Some(unit) = self.map.unit_mut(*id) {
                    (unit.route, unit.bound) = (route.clone(), bound);
                }
            }
            ids = coming;
            if hex != to {
                // On the way: that is this turn's.
                break;
            }
        }
        if legs > 1 {
            self.undo = None;
        }
        events
    }

    /// Units on a standing order cannot go on this turn: they wait for their
    /// player, and keep the order.
    fn stall(&mut self, side: Side, ids: Vec<UnitId>, at: Hex, to: Hex) -> Vec<Event> {
        for id in &ids {
            if let Some(unit) = self.map.unit_mut(*id) {
                unit.stalled = true;
            }
        }
        vec![Event::Stalled { side, units: ids, at, to }]
    }

    /// Who of `group` goes to `hex` this turn: all of them, if every one
    /// that moves can get there; else none (a stack stays together: the
    /// owner's word, 2026-11-22).
    fn coming(&self, group: &Group, hex: Hex, sight: &Sight) -> Vec<UnitId> {
        let units = self.map.units();
        let reach = |(id, table): &(UnitId, MoveCostId)| {
            units.get(*id).is_some_and(|u| self.known_range_by(u, *table, sight).cost(hex).is_some())
        };
        if !group.movers.iter().all(reach) {
            return Vec::new();
        }
        group.movers.iter().map(|(id, _)| *id).chain(group.carried.iter().copied()).collect()
    }

    /// At the end of `side`'s turn, its stacks on standing orders march, a
    /// stack (the units of a hex with the same route) at a time, with the
    /// movement they have left.
    fn marches(&mut self, side: Side) -> Vec<Event> {
        let mut groups: Vec<(Hex, &[Hex], Vec<UnitId>)> = Vec::new();
        for unit in self.map.units().of_side(side).filter(|u| self.map.contains(u.pos()) && !u.route.is_empty()) {
            match groups.iter_mut().find(|(at, route, _)| *at == unit.pos() && *route == unit.route.as_slice()) {
                Some((_, _, ids)) => ids.push(unit.id()),
                None => groups.push((unit.pos(), &unit.route, vec![unit.id()])),
            }
        }
        let groups: Vec<Vec<UnitId>> = groups.into_iter().map(|(_, _, ids)| ids).collect();
        let mut events = Vec::new();
        for ids in groups {
            events.extend(self.march(side, &ids));
            // Nor has the player moved them this turn: a stack that got where
            // it was sent waits for orders.
            for id in &ids {
                if let Some(unit) = self.map.unit_mut(*id) {
                    unit.moved = false;
                }
            }
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
                if let Some(before) = taken {
                    if let Some(c) = self.map.city_mut(before.id) {
                        (c.side, c.building, c.progress, c.walls) =
                            (before.side, before.building, before.progress, before.walls);
                        (c.sends, c.queue, c.resting) = (before.sends, before.queue, before.resting);
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
                for (id, march) in goals {
                    if let Some(unit) = self.map.unit_mut(id) {
                        march.restore(unit);
                    }
                }
                // They are where they were, and selected again, to go
                // somewhere else.
                self.select(units.iter().map(|(id, _, _)| *id).collect());
            }
            Undo::Ordered { goals } => {
                for (id, _) in &goals {
                    self.own_unit(side, *id)?;
                }
                let ids: Vec<UnitId> = goals.iter().map(|(id, _)| *id).collect();
                for (id, march) in goals {
                    if let Some(unit) = self.map.unit_mut(id) {
                        march.restore(unit);
                    }
                }
                // (In rounds, with a plan still: to be sent on from its end,
                // as they were before the leg taken back.)
                if !self.select_onward(&ids) {
                    self.select(ids);
                }
            }
            Undo::Built { city, building, progress, queue, resting } => {
                let holder = self.map.cities().get(city).map(|c| c.side);
                if holder != Some(side) {
                    return Err("the city is not the side's".to_string());
                }
                if let Some(c) = self.map.city_mut(city) {
                    (c.building, c.progress, c.queue, c.resting) = (building, progress, *queue, resting);
                }
            }
            Undo::Walls { city, price } => {
                let holder = self.map.cities().get(city).map(|c| c.side);
                if holder != Some(side) {
                    return Err("the city is not the side's".to_string());
                }
                if let Some(c) = self.map.city_mut(city) {
                    c.walls = c.walls.saturating_sub(1);
                }
                self.raised.retain(|c| *c != city);
                let gold = &mut self.treasury[side.index()];
                *gold = gold.saturating_add(price);
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
        // Where they can go is what the side knows of the map (fog of war).
        let sight = self.sight(side);
        if !sight.knows(to) {
            return Err(UNSEEN.to_string());
        }
        let mut ways: Vec<Vec<Hex>> = Vec::new();
        for id in &ids {
            self.own_unit(side, *id)?;
        }
        // Each that moves pays what it costs it; those a ship carries, nothing.
        let group = self.group(&ids)?;
        for (id, table) in &group.movers {
            let unit = self.own_unit(side, *id)?;
            let cost = self.known_range_by(unit, *table, &sight).cost(to);
            let cost = cost.ok_or(format!("{} cannot get there this turn", unit.name))?;
            going.push((*id, unit.pos(), cost));
            before.push((*id, unit.pos(), unit.movement));
            goals.push((*id, MarchState::of(unit)));
            let way = self.known_path_by(unit, *table, to, unit.movement, &sight).unwrap_or_default();
            ways.push(std::iter::once(unit.pos()).chain(way.into_iter().map(|(h, _)| h)).collect());
        }
        for id in &group.carried {
            let unit = self.own_unit(side, *id)?;
            going.push((*id, unit.pos(), 0));
            before.push((*id, unit.pos(), unit.movement));
            goals.push((*id, MarchState::of(unit)));
        }
        // Nobody is left in the water with no ship, and nobody goes onto the
        // water with none to carry them.
        let ids: Vec<UnitId> = going.iter().map(|(id, _, _)| *id).collect();
        for (_, from, _) in &before {
            if !self.afloat(*from, &self.there_after(side, *from, &ids, &[])) {
                return Err("that would leave units in the water with no ship to carry them".to_string());
            }
        }
        if !hostile(&self.map, side, to) && !self.afloat(to, &self.there_after(side, to, &[], &ids)) {
            return Err("there is no room aboard for them there".to_string());
        }
        // Another side's units it did not see on the way: the move ends
        // there, in a battle nobody looked for.
        let mut events = Vec::new();
        let met = ways.first().and_then(|way| {
            way.iter().skip(1).copied().find(|h| *h != to && hostile(&self.map, side, *h) && !sight.sees(*h))
        });
        let to = match met {
            Some(at) => {
                for (i, (id, _, cost)) in going.iter_mut().enumerate().take(group.movers.len()) {
                    let unit = self.map.units().get(*id);
                    let there = unit.and_then(|u| self.known_range_by(u, group.movers[i].1, &sight).cost(at));
                    *cost = there.unwrap_or(*cost);
                    if let Some(way) = ways.get_mut(i) {
                        if let Some(stop) = way.iter().position(|h| *h == at) {
                            way.truncate(stop + 1);
                        }
                    }
                }
                events.push(Event::Surprise { side, at });
                at
            }
            None => to,
        };
        let revealing = |game: &Game| -> Vec<Hex> {
            let now = game.sight(side);
            let units = game.map.units().iter().filter(|u| u.side() != side).map(|u| u.pos());
            let cities = game.map.cities().iter().filter(|c| c.side != side).map(|c| c.pos);
            units.chain(cities).filter(|h| now.sees(*h) && !sight.sees(*h)).collect()
        };
        let city_before = self.map.city_at(to).map(CityBefore::of);
        let mut stacks: Vec<(Hex, Vec<UnitId>)> = Vec::new();
        for (_, from, _) in &before {
            if !stacks.iter().any(|(hex, _)| hex == from) {
                stacks.push((*from, self.map.units_at(*from).to_vec()));
            }
        }
        // What they see on the way stays seen this turn.
        for way in &ways {
            self.see_along(side, way);
        }
        let battled = hostile(&self.map, side, to);
        if battled {
            // An attack is a stack: as many as a hex holds, from wherever.
            if !self.fits(Hex::new(u16::MAX, u16::MAX), &ids) {
                return Err(format!("at most {MAX_STACK} units and a commander attack together"));
            }
            let (won, battle) = self.battle(side, &ids, to);
            events.push(battle);
            events.append(&mut self.promoted);
            if !won {
                self.undo = None;
                // An attack uses up the attackers' movement for the turn.
                for (id, _, _) in &going {
                    self.pay(*id, u32::MAX);
                }
                // Who rode the ships that sank goes down with them.
                let origins: Vec<Hex> = before.iter().map(|(_, from, _)| *from).collect();
                events.extend(self.drown(side, &origins));
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
        // The way they went, as the first of them went it (straight from
        // where each was, if that is not it).
        let way_of = |from: Hex| match ways.first() {
            Some(way) if way.first() == Some(&from) && way.last() == Some(&to) => way.clone(),
            _ => vec![from, to],
        };
        for (id, from, cost) in going {
            // After a battle, those that do not fit stay where they were.
            if self.map.move_unit(id, to).is_ok() {
                events.push(Event::Moved { unit: id, from, to, cost, way: way_of(from) });
                arrived.push(id);
            }
            // (An attack uses up the attackers' movement for the turn.)
            self.pay(id, if battled { u32::MAX } else { cost });
        }
        // After a battle at sea, who has no ship left to ride goes down.
        if battled {
            let hexes: Vec<Hex> = before.iter().map(|(_, from, _)| *from).chain([to]).collect();
            events.extend(self.drown(side, &hexes));
        }
        // Those that watched them go out of their sight remember where.
        if let Some(way) = ways.first() {
            self.seen_leaving(side, way, arrived.len());
        }
        for id in &ids {
            if let Some(unit) = self.map.unit_mut(*id) {
                (unit.moved, unit.stalled, unit.defending) = (true, false, false);
                (unit.route, unit.bound) = (Vec::new(), None);
                unit.stand_down();
            }
        }
        if let Some((side, at)) = self.emerging.take() {
            events.extend(self.emerge(side, at));
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
        // A move can be taken back unless it was a battle or put a side out;
        // nor, with fog of war, if it showed what was not seen before
        // (another side's units, who holds a city, land never seen): what
        // was seen cannot be unseen.
        let final_ = events.iter().any(|e| matches!(e, Event::Battle { .. } | Event::Out { .. } | Event::Won { .. }));
        let explored = self.sight(side).known_count() != sight.known_count();
        let showed = self.setup.fog.on() && (explored || !revealing(self).is_empty());
        if !final_ && !showed {
            self.undo = Some(Undo::Moved { units: before, to, stacks, taken, goals });
        }
        Ok(events)
    }

    /// Units of `side` on a temple seek its blessing: each it has not blessed
    /// is blessed (chance says how: `Blessing`) and stops for the turn; one
    /// it has blessed keeps its movement. Chance had a say: no undo.
    fn bless(&mut self, side: Side, ids: &[UnitId]) -> Result<Vec<Event>, String> {
        let first = self.own_unit(side, *ids.first().ok_or("no units")?)?.pos();
        for id in ids {
            if self.own_unit(side, *id)?.pos() != first {
                return Err("the units are not all on one hex".to_string());
            }
        }
        if !self.map.tile(first).is_some_and(|t| t.kind == TileType::Temple) {
            return Err("they are not on a temple".to_string());
        }
        let events = self.blessings(first, ids);
        if events.is_empty() {
            return Err("this temple has blessed them all already".to_string());
        }
        // An order of their player's, for all of them (those blessed before
        // too, so that the stack stays one): any standing order ends.
        for id in ids {
            if let Some(unit) = self.map.unit_mut(*id) {
                unit.moved = true;
                (unit.route, unit.bound, unit.stalled, unit.planned) = (Vec::new(), None, false, false);
                unit.stand_down();
            }
        }
        self.undo = None;
        self.selected.retain(|id| !ids.contains(id));
        if self.selected.is_empty() {
            self.deselect();
        }
        Ok(events)
    }

    /// Those of `ids` (on `at`, a temple) it has not blessed are blessed
    /// (chance says how) and stop there for the turn: their movement is
    /// spent.
    fn blessings(&mut self, at: Hex, ids: &[UnitId]) -> Vec<Event> {
        let mut events = Vec::new();
        for id in ids {
            if self.map.units().get(*id).is_none_or(|u| u.blessed_at(at)) {
                continue;
            }
            let blessing = if self.rng.below(2) == 0 { Blessing::Strength } else { Blessing::Moves };
            if let Some(unit) = self.map.unit_mut(*id) {
                unit.bless(at, blessing);
                unit.movement = 0;
                events.push(Event::Blessed { unit: *id, side: unit.side(), name: unit.title(), blessing });
            }
        }
        events
    }

    /// Why `units` of `side` cannot be sent along `route` (its stops in
    /// order; the last is where they end), if they cannot: a stop off the
    /// map, or on land nobody of the side has seen, or a leg one of them
    /// cannot make over the land the side knows, in as many turns as it
    /// takes (whoever is on the way now: units move). Those a ship carries
    /// go where it goes. `Order::GoTo` asks it, and so can a screen, a stop
    /// at a time.
    pub fn check_route(&self, side: Side, units: &[UnitId], route: &[Hex]) -> Result<(), String> {
        let sight = self.sight(side);
        for to in route {
            if !self.map.contains(*to) {
                return Err(format!("{to} is not on the map"));
            }
            if !sight.knows(*to) {
                return Err(UNSEEN.to_string());
            }
        }
        let known = |hex: Hex| sight.knows(hex);
        let group = self.group(units)?;
        for (id, table) in &group.movers {
            let Some(unit) = self.map.units().get(*id) else { continue };
            let costs = self.map.units().move_cost(*table);
            let mut from = unit.pos();
            for to in route {
                if !costs.is_some_and(|c| way_over_land(&self.map, c, (from, *to), &known)) {
                    return Err(format!("{} cannot get there", unit.title()));
                }
                from = *to;
            }
        }
        Ok(())
    }

    /// Why `city` cannot send what it builds along `route` for `side` (its
    /// stops in order; empty, nowhere), if it cannot: it is not the side's,
    /// or a stop is off the map or on land nobody of the side has seen, or
    /// nothing it builds can make every leg over the land the side knows.
    /// `Order::Rally` asks it, and so can a screen, a stop at a time.
    pub fn check_rally(&self, side: Side, city: CityId, route: &[Hex]) -> Result<(), String> {
        let c = self.map.cities().get(city).ok_or("there is no such city")?;
        if c.side != side {
            return Err(format!("only {} says where its units go", side_name(c.side)));
        }
        let sight = self.sight(side);
        for to in route {
            if !self.map.contains(*to) {
                return Err("that is not on the map".to_string());
            }
            if !sight.knows(*to) {
                return Err(UNSEEN.to_string());
            }
        }
        let units = self.map.units();
        let known = |hex: Hex| sight.knows(hex);
        let reaches = c.units.iter().any(|prod| {
            let kind = units.kind_named(&prod.basename).and_then(|k| units.kind(k));
            kind.is_some_and(|k| {
                let mut from = c.pos;
                route
                    .iter()
                    .all(|to| way_over_land(&self.map, &k.cost, (std::mem::replace(&mut from, *to), *to), &known))
            })
        });
        if !reaches && !route.is_empty() {
            return Err(format!("nothing {} builds can get there", c.name));
        }
        Ok(())
    }

    /// Set `city`'s queue for `side` (`Order::Queue`): every item one it
    /// can build, at least one of it, sent somewhere it can send its units;
    /// it goes on from the item it was building if that is still there.
    fn set_queue(
        &mut self,
        side: Side,
        city: CityId,
        items: Vec<QueueItem>,
        end: QueueEnd,
    ) -> Result<Vec<Event>, String> {
        self.check_rally(side, city, &[])?;
        let kinds = self.map.cities().get(city).map_or(0, |c| c.units.len());
        if items.len() > MAX_QUEUE {
            return Err(format!("a queue holds {MAX_QUEUE} items at most"));
        }
        for item in &items {
            if item.what >= kinds {
                return Err("it cannot build that".to_string());
            }
            if item.count == 0 {
                return Err("an item is one unit at least".to_string());
            }
            match &item.sends {
                Sends::Stay | Sends::Defend => {}
                Sends::Route(route) if route.is_empty() => return Err("a route to nowhere".to_string()),
                Sends::Route(route) => self.check_rally(side, city, route)?,
                Sends::Point { n, .. } => {
                    let point = self.rally_point(side, *n).ok_or(format!("rally point {n} is not set"))?;
                    self.check_rally(side, city, &[point.at])?;
                }
            }
        }
        if let QueueEnd::Repeat { from, .. } = end {
            if from >= items.len() {
                return Err("it cannot repeat from an item it does not have".to_string());
            }
        }
        let Some(c) = self.map.city_mut(city) else { return Err("there is no such city".to_string()) };
        if items.is_empty() {
            c.queue = Queue::default();
            return Ok(Vec::new());
        }
        // On from the item it was building, if that is still there: where it
        // was (changed in place, or the same kind there); else the nearest of
        // its kind (an item before it taken out); else from the start.
        let old = &c.queue;
        let at = old.current().and_then(|current| {
            let nearest = |found: &dyn Fn(&QueueItem) -> bool| {
                (0..items.len()).filter(|j| found(&items[*j])).min_by_key(|j| (j.abs_diff(old.at), *j))
            };
            // The same item (another taken out or put in before it); an
            // item changed in place; one of its kind.
            nearest(&|new| new == current)
                .or_else(|| (items.len() == old.items.len() && old.at < items.len()).then_some(old.at))
                .or_else(|| nearest(&|new| new.what == current.what))
        });
        let (at, made, rounds) = match (at, old.current()) {
            (Some(at), Some(current)) if items[at].what == current.what => (at, old.made, old.rounds),
            (Some(at), _) => (at, 0, old.rounds),
            (None, _) => (0, 0, 0),
        };
        let made = made.min(items[at].count - 1);
        c.queue = Queue { items, end, at, made, rounds };
        c.follow_queue();
        Ok(Vec::new())
    }

    /// `side`'s rally point `n`, if it is set.
    pub fn rally_point(&self, side: Side, n: u8) -> Option<RallyPoint> {
        self.rally_points.get(side.index())?.get(usize::from(n)).copied().flatten()
    }

    /// `side`'s rally points, 0 to 9 (`None`: not set).
    pub fn rally_points(&self, side: Side) -> [Option<RallyPoint>; RALLY_POINTS] {
        self.rally_points.get(side.index()).copied().unwrap_or([None; RALLY_POINTS])
    }

    /// Which of `side`'s rally points is on `hex`, if one is (the first).
    pub fn rally_point_at(&self, side: Side, hex: Hex) -> Option<u8> {
        let points = self.rally_points(side);
        (0..RALLY_POINTS as u8).find(|n| points[usize::from(*n)].is_some_and(|p| p.at == hex))
    }

    /// Which of `side`'s rally points units sent to `hex` would be bound
    /// to: the one there, unless another side's units stand there that
    /// `side` sees (onto those a click is an attack, as anywhere; the
    /// owner's report, 2026-11-21: "I don't seem to be able to attack an
    /// enemy stack at one of my rally points"). One it does not see (fog of
    /// war) changes nothing: the click says nothing of it.
    pub fn rally_target(&self, side: Side, hex: Hex) -> Option<u8> {
        let point = self.rally_point_at(side, hex)?;
        let seen_foe = hostile(&self.map, side, hex) && self.sight(side).sees(hex);
        (!seen_foe).then_some(point)
    }

    /// Put `side`'s rally point `n` somewhere, move it, or take it away
    /// (`Order::RallyPoint`).
    fn set_rally_point(&mut self, side: Side, n: u8, point: Option<RallyPoint>) -> Result<Vec<Event>, String> {
        if usize::from(n) >= RALLY_POINTS {
            return Err(format!("the rally points are 0 to {}", RALLY_POINTS - 1));
        }
        if side == Side::Grey {
            return Err("nobody has rally points".to_string());
        }
        if let Some(p) = point {
            if !self.map.contains(p.at) {
                return Err(format!("{} is not on the map", p.at));
            }
            let sight = self.sight(side);
            if !sight.knows(p.at) {
                return Err(UNSEEN.to_string());
            }
            if self.map.tile(p.at).is_some_and(|t| t.kind == TileType::Water) && self.map.city_at(p.at).is_none() {
                return Err("a rally point goes on land".to_string());
            }
            if self.map.city_at(p.at).is_some_and(|c| sight.holder(c) != side) {
                return Err("a rally point goes in no city but its side's".to_string());
            }
        }
        let units: Vec<UnitId> =
            self.map.units().of_side(side).filter(|u| u.bound == Some(n)).map(|u| u.id()).collect();
        for id in units {
            let Some(unit) = self.map.unit_mut(id) else { continue };
            match point {
                // Moved: they go to its new place.
                Some(p) => {
                    if let Some(end) = unit.route.last_mut() {
                        *end = p.at;
                    }
                    unit.stalled = false;
                }
                // Gone: they stop, and wait for orders.
                None => (unit.route, unit.bound, unit.stalled) = (Vec::new(), None, false),
            }
        }
        if point.is_none() {
            let cities: Vec<CityId> = self
                .map
                .cities()
                .iter()
                .filter(|c| c.side == side && matches!(c.sends, Sends::Point { n: m, .. } if m == n))
                .map(|c| c.id)
                .collect();
            for city in cities {
                if let Some(c) = self.map.city_mut(city) {
                    c.sends = Sends::Stay;
                }
            }
            // And the items of queues that sent theirs there.
            let queued: Vec<CityId> = self
                .map
                .cities()
                .iter()
                .filter(|c| {
                    c.side == side
                        && c.queue.items.iter().any(|i| matches!(i.sends, Sends::Point { n: m, .. } if m == n))
                })
                .map(|c| c.id)
                .collect();
            for city in queued {
                if let Some(c) = self.map.city_mut(city) {
                    for item in &mut c.queue.items {
                        if matches!(item.sends, Sends::Point { n: m, .. } if m == n) {
                            item.sends = Sends::Stay;
                        }
                    }
                    c.follow_queue();
                }
            }
        }
        if let Some(slot) = self.rally_points.get_mut(side.index()).and_then(|p| p.get_mut(usize::from(n))) {
            *slot = point;
        }
        Ok(Vec::new())
    }

    /// Where `ids` of `side` (on `at`) bound to rally point `n` end: on it if
    /// there is room for them there, else on the nearest hex around it
    /// (out to three hexes, in a fixed order, so that one fills before the
    /// next) that has room, that they can stand on and get to, and that has
    /// nobody else's units or city. `None` if the point is not set, or none
    /// has room.
    fn rally_spot(&self, side: Side, n: u8, ids: &[UnitId], at: Hex) -> Option<Hex> {
        let point = self.rally_point(side, n)?;
        if self.fits(point.at, ids) || at == point.at {
            return Some(point.at);
        }
        let group = self.group(ids).ok()?;
        let (lead, table) = group.lead(&self.map)?;
        let sight = self.sight(side);
        let (rows, cols) = (self.map.rows(), self.map.cols());
        let mut ring = vec![point.at];
        let mut seen = vec![point.at];
        for _ in 0..3 {
            let mut next = Vec::new();
            for hex in &ring {
                for near in hex.neighbours(rows, cols) {
                    if !seen.contains(&near) {
                        seen.push(near);
                        next.push(near);
                    }
                }
            }
            for hex in &next {
                let theirs = self.map.city_at(*hex).is_some_and(|c| c.side != side);
                let usable = sight.knows(*hex) && !theirs && !hostile(&self.map, side, *hex) && self.fits(*hex, ids);
                if usable && (*hex == at || self.known_path_by(lead, table, *hex, u32::MAX, &sight).is_some()) {
                    return Some(*hex);
                }
            }
            ring = next;
        }
        None
    }

    /// A unit just built goes where its city sends it (`Sends`).
    fn send_new(&mut self, side: Side, id: UnitId, sends: &Sends) {
        let (route, bound, defend) = match sends {
            Sends::Stay => (Vec::new(), None, false),
            Sends::Defend => (Vec::new(), None, true),
            Sends::Route(route) => (route.clone(), route.last().and_then(|h| self.rally_point_at(side, *h)), false),
            Sends::Point { n, bless } => match self.rally_point(side, *n) {
                Some(point) => {
                    let temple = if *bless { self.temple_on_the_way(side, id, point.at) } else { None };
                    (temple.into_iter().chain([point.at]).collect(), Some(*n), false)
                }
                // (A point taken away: they stay.)
                None => (Vec::new(), None, false),
            },
        };
        if let Some(unit) = self.map.unit_mut(id) {
            (unit.route, unit.bound, unit.defending) = (route, bound, defend);
        }
    }

    /// A temple near `id`'s way to `to` that has not blessed it, as its side
    /// knows the map: the one that adds the least to the way, if that is no
    /// more than a turn's march (its full movement). None if there is none.
    fn temple_on_the_way(&self, side: Side, id: UnitId, to: Hex) -> Option<Hex> {
        let unit = self.map.units().get(id)?;
        let from = unit.pos();
        let costs = self.map.units().move_cost(unit.move_cost)?;
        let sight = self.sight(side);
        let (pass, steps) = self.ground(unit, costs, &sight);
        let cost = |a: Hex, b: Hex| -> Option<u32> {
            if a == b {
                return Some(0);
            }
            path_with(&self.map, (a, b), &steps, u32::MAX, &pass).and_then(|way| way.last().map(|(_, c)| *c))
        };
        let direct = cost(from, to)?;
        let budget = unit.base_move_points;
        // In hexes, a temple farther off the way than a turn's march could go
        // (a step costs a point at least) is not worth asking the way to.
        let straight = from.distance(to);
        let (rows, cols) = (self.map.rows(), self.map.cols());
        let mut best: Option<(u32, Hex)> = None;
        for row in 0..rows {
            for col in 0..cols {
                let hex = Hex::new(row, col);
                if self.map.tile(hex).is_none_or(|t| t.kind != TileType::Temple) || unit.blessed_at(hex) {
                    continue;
                }
                let off_the_way = from.distance(hex).saturating_add(hex.distance(to)) > straight.saturating_add(budget);
                if !sight.knows(hex) || off_the_way {
                    continue;
                }
                let Some(by) = cost(from, hex).zip(cost(hex, to)).map(|(a, b)| a.saturating_add(b)) else { continue };
                let extra = by.saturating_sub(direct);
                if extra <= budget && best.is_none_or(|(e, _)| extra < e) {
                    best = Some((extra, hex));
                }
            }
        }
        best.map(|(_, hex)| hex)
    }

    /// What raising the walls of `city` a level costs `side` now, or why it
    /// cannot (`Order::Walls`): it is not its city, they are as high as walls
    /// go, it raised them this turn, or it has not the gold.
    pub fn walls_price(&self, side: Side, city: CityId) -> Result<u32, String> {
        if let Some(winner) = self.winner {
            return Err(format!("the game is over: {} won", side_name(winner)));
        }
        let c = self.map.cities().get(city).ok_or("there is no such city")?;
        if c.side != side {
            return Err(format!("only {} raises its walls", side_name(c.side)));
        }
        if c.walls >= crate::state::WALLS_CAP {
            return Err("its walls are as high as walls go".to_string());
        }
        if self.raised.contains(&city) {
            return Err("its walls were raised this turn".to_string());
        }
        let price = crate::state::wall_price(c.walls);
        if self.gold(side) < price {
            return Err(format!("that takes {price} gold"));
        }
        Ok(price)
    }

    /// Units of `side`'s go home for good (`Order::Disband`). A ship on the
    /// water goes only if what is left there can still carry the land units
    /// aboard.
    fn disband(&mut self, side: Side, ids: &[UnitId]) -> Result<Vec<Event>, String> {
        if ids.is_empty() {
            return Err("no units".to_string());
        }
        let mut hexes: Vec<Hex> = Vec::new();
        for id in ids {
            let at = self.own_unit(side, *id)?.pos();
            if !hexes.contains(&at) {
                hexes.push(at);
            }
        }
        // Whoever stays where they were must still be afloat (the rule a
        // move keeps: riders within the ships' room, a commander who rides
        // with a ship or a flyer).
        for at in &hexes {
            if !self.afloat(*at, &self.there_after(side, *at, ids, &[])) {
                return Err("those aboard would drown: the ship cannot go without them".to_string());
            }
        }
        let mut names = Vec::new();
        for id in ids {
            if let Some(unit) = self.map.kill_unit(*id) {
                names.push(unit.title());
            }
        }
        self.undo = None;
        self.selected.retain(|id| !ids.contains(id));
        if self.selected.is_empty() {
            self.deselect();
        }
        Ok(vec![Event::Disbanded { side, names }])
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
    /// (`combat::QUARTER`): its strength, the best bonus of a commander in its
    /// stack (`stack`, not itself), its kind's bonus on that ground, and, if
    /// it defends, the ground's bonus and the walls' of a city there
    /// (`combat::walls_bonus`); see `strength_parts`, which also counts what
    /// siege engines break.
    /// No more than the game's cap, if it has one (`combat::Rules::cap`).
    pub fn battle_strength(&self, unit: &Unit, at: Hex, defending: bool, stack: &[UnitId]) -> u32 {
        self.setup.combat.capped(self.strength_parts(unit, at, defending, stack, 0).quarters())
    }

    /// What `battle_strength` is made of (`StrengthParts`), against
    /// attackers whose siege engines break `siege` points of walls (0 for
    /// an attacker).
    pub fn strength_parts(&self, unit: &Unit, at: Hex, defending: bool, stack: &[UnitId], siege: u32) -> StrengthParts {
        let units = self.map.units();
        let led = stack
            .iter()
            .filter(|id| **id != unit.id())
            .filter_map(|id| units.get(*id))
            .map(|u| self.command(u))
            .max()
            .unwrap_or(0);
        let ground_kind = self.map.tile(at).map_or(TileType::Grass, |t| t.kind);
        let home = units.kind_of(unit).map_or(0, |k| k.bonus_on(ground_kind));
        // (Wounded, half its own: `Unit::fighting_power`.)
        let mut parts = StrengthParts { own: unit.fighting_power(), led, home, ground: 0, walls: 0 };
        if defending {
            parts.ground = combat::ground_bonus(ground_kind);
            parts.walls = self
                .map
                .city_at(at)
                .map_or(0, |c| combat::walls_bonus(c.walls).saturating_sub(siege) * combat::QUARTER);
        }
        parts
    }

    /// How many points of walls `attackers` break: their best siege engine's
    /// (`UnitKind::siege`).
    pub fn siege_of(&self, attackers: &[UnitId]) -> u32 {
        let units = self.map.units();
        attackers
            .iter()
            .filter_map(|id| units.get(*id))
            .filter_map(|u| units.kind_of(u))
            .map(|k| k.siege)
            .max()
            .unwrap_or(0)
    }

    /// How much stronger a commander makes the others of its stack: what its
    /// kind says, and a point more at the highest level. 0 for a unit that is
    /// not one.
    pub fn command(&self, unit: &Unit) -> u32 {
        crate::state::command_bonus(self.map.units().kind_of(unit).map_or(0, |k| k.commander), unit.level)
    }

    /// True if `unit` fights in a battle on `at`: at sea, the ships and the
    /// flyers (who rides aboard does not; a commander still leads); on land,
    /// all but the ships (in a port they do not).
    fn fights_at(&self, unit: &Unit, at: Hex) -> bool {
        match self.map.travel(unit.id()) {
            Travel::Sea => self.map.city_at(at).is_none(),
            Travel::Air => true,
            Travel::Land => !(self.map.open_water(at) && self.rides(unit.id())),
        }
    }

    /// The two sides of a battle of `attackers` against whoever is on `at`:
    /// those of them that fight there (`fights_at`).
    fn fighters(&self, attackers: &[UnitId], at: Hex) -> (Vec<Fighter>, Vec<Fighter>) {
        let defenders: Vec<UnitId> = self.map.units_at(at).to_vec();
        let siege = self.siege_of(attackers);
        let side = |ids: &[UnitId], defending: bool| -> Vec<Fighter> {
            ids.iter()
                .filter_map(|id| self.map.units().get(*id))
                .filter(|unit| self.fights_at(unit, at))
                .map(|unit| {
                    let parts = self.strength_parts(unit, at, defending, ids, siege);
                    let rules = self.setup.combat;
                    let strength = rules.capped(parts.quarters());
                    // As strong wounded, unless wounds last.
                    let hurt = if rules.wounds {
                        rules.capped(StrengthParts { own: combat::wounded(unit.power()), ..parts }.quarters())
                    } else {
                        strength
                    };
                    Fighter {
                        unit: unit.id(),
                        strength,
                        commander: self.map.is_commander(unit.id()),
                        hits: combat::HITS.saturating_sub(unit.wounds).max(1),
                        hurt,
                    }
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
        self.battle_forecast(attackers, at).map(|f| f.odds)
    }

    /// What an attack by `attackers` on `at` is likely to come to: the odds
    /// (as `battle_odds`), and how many die on each side on the whole.
    pub fn battle_forecast(&self, attackers: &[UnitId], at: Hex) -> Option<combat::Forecast> {
        let (a, d) = self.fighters(attackers, at);
        if d.is_empty() || a.is_empty() {
            return None;
        }
        let seed = (u64::from(self.round) << 32) ^ (u64::from(at.row) << 16) ^ u64::from(at.col);
        Some(combat::forecast(&a, &d, &self.setup.combat, seed, 200))
    }

    /// `side`'s units attack whoever is on `at`: they fight with the game's
    /// chance; the dead go. Returns whether the attackers won, and the event.
    fn battle(&mut self, side: Side, attackers: &[UnitId], at: Hex) -> (bool, Event) {
        let (a, d) = self.fighters(attackers, at);
        let defender = self.map.units_at(at).first().map_or(Side::Grey, |id| id.side);
        let attacking: Vec<UnitId> = a.iter().map(|f| f.unit).collect();
        // How likely the attackers were to win (asked with a chance of its
        // own: the game's is not touched).
        let odds = self.battle_odds(attackers, at).unwrap_or(100);
        let battle = combat::fight(&a, &d, &self.setup.combat, &mut self.rng);
        // As it was, to be watched again.
        let defending: Vec<UnitId> = d.iter().map(|f| f.unit).collect();
        // (A commander leads its whole stack, fighting or not, as in the
        // battle; the siege broke what walls there were, no more.)
        let (all_attackers, all_defenders) = (attackers.to_vec(), self.map.units_at(at).to_vec());
        let siege = self.siege_of(attackers);
        let walls = self.map.city_at(at).map(|c| c.walls);
        self.fought.push(Fight {
            at,
            attacker: side,
            defender,
            rules: self.setup.combat,
            terrain: self.map.tile(at).map_or(TileType::Grass, |t| t.kind),
            walls: walls.unwrap_or(0),
            siege: walls.map_or(0, |w| siege.min(combat::walls_bonus(w))),
            attackers: self.combatants((&attacking, &all_attackers), at, false, 0),
            defenders: self.combatants((&defending, &all_defenders), at, true, siege),
            trace: battle.trace.clone(),
            won: battle.attackers_won,
        });
        // Won: who was there and did not fight (aboard the ships that sank,
        // the ships in a port that fell) is lost with them.
        let mut fallen = battle.dead.clone();
        if battle.attackers_won {
            fallen.extend(self.map.units_at(at).iter().filter(|id| !battle.dead.contains(id)));
        }
        let lost = |s: Side| fallen.iter().filter(|u| u.side == s).count();
        let units = self.map.units();
        let dead = fallen
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
        for unit in &fallen {
            self.map.kill_unit(*unit);
        }
        // Wounds that last: who lived keeps the hits it took (they heal as
        // its side's turn ends, `heal`).
        if self.setup.combat.wounds {
            for blow in &battle.trace {
                let fighter = if blow.hit_attacker { a.get(blow.unit) } else { d.get(blow.unit) };
                if let Some(unit) = fighter.and_then(|f| self.map.unit_mut(f.unit)) {
                    unit.wounds = (unit.wounds + 1).min(combat::HITS - 1);
                }
            }
        }
        // Those who held, on patrol or marauding, need their player's word.
        if !battle.attackers_won {
            self.alarm(at);
        }
        // Those who lived through it won it: experience, a point more if the
        // odds were against them; and, if no commander led them and it was a
        // hard fight, maybe a captain from their ranks.
        let (winners, winner_odds): (Vec<UnitId>, u32) = if battle.attackers_won {
            (attacking, odds)
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
    /// order of `fighters`): led by a commander of their whole `stack`, who
    /// leads though it may not fight (aboard a ship at sea).
    fn combatants(&self, (ids, stack): (&[UnitId], &[UnitId]), at: Hex, defending: bool, siege: u32) -> Vec<Combatant> {
        let units = self.map.units();
        ids.iter()
            .filter_map(|id| units.get(*id))
            .map(|unit| {
                let parts = self.strength_parts(unit, at, defending, stack, siege);
                let rules = self.setup.combat;
                let hurt = if rules.wounds {
                    rules.capped(StrengthParts { own: combat::wounded(unit.power()), ..parts }.quarters())
                } else {
                    rules.capped(parts.quarters())
                };
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
                    // (Wounded, its own as its wounds leave it, all of it.)
                    strength: if unit.wounds > 0 { parts.own } else { unit.strength },
                    growth: if unit.wounds > 0 { 0 } else { parts.own.saturating_sub(unit.strength) },
                    led: parts.led,
                    ground: parts.ground,
                    home: parts.home,
                    walls: parts.walls,
                    quarters: rules.capped(parts.quarters()),
                    wounds: unit.wounds,
                    hurt,
                }
            })
            .collect()
    }

    /// `by` takes a city from `from`: it starts its work over, and a side
    /// that has lost its last city is out.
    fn take(&mut self, city: CityId, from: Side, by: Side) -> Vec<Event> {
        let mut walls = None;
        if let Some(c) = self.map.city_mut(city) {
            // Where its old holder sent its new units, and its queue, are not
            // the new one's.
            c.side = by;
            (c.sends, c.queue, c.resting) = (Sends::Stay, Queue::default(), false);
            // Its walls fall a level (the owner's rule, 2026-11-20).
            if c.walls > 0 {
                c.walls -= 1;
                walls = Some(c.walls);
            }
        }
        self.map.set_production(city, None);
        let mut events = vec![Event::Taken { city, from, by, walls }];
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

    /// The wounds of the units of the sides `heals` says heal.
    fn heal(&mut self, heals: impl Fn(Side) -> bool) {
        let hurt: Vec<UnitId> =
            self.map.units().iter().filter(|u| u.wounds > 0 && heals(u.side())).map(|u| u.id()).collect();
        for id in hurt {
            if let Some(unit) = self.map.unit_mut(id) {
                unit.wounds = 0;
            }
        }
    }

    /// The next side's turn: after the last of the round, the first of the
    /// next (in a new order, if it is drawn). From the second round on, a
    /// side's turn begins with its units' points back (with up to 2 they kept)
    /// and its cities' work.
    fn next_turn(&mut self) -> Vec<Event> {
        self.deselect();
        let ending = self.player();
        if self.order.is_empty() {
            self.round += 1;
            return Vec::new();
        }
        // The ending side's standing orders go now, at the end of its turn,
        // with the movement its units have left (the owner's word,
        // 2026-11-21: its player could still give them other orders in the
        // turn, in a pinch): the marches, then the patrols (a patrol that
        // fought and was left alone goes on). What they see on their way is
        // noted as they pass (the other sides' stacks stay as ghosts where
        // they were seen), and seen no longer once they have passed.
        self.ease(ending);
        let mut events = self.marches(ending);
        events.extend(self.patrols(ending));
        // Its units' wounds heal (`combat::Rules::wounds`; the owner's word,
        // 2026-11-21: attacked in waves in the others' turns, a stack stays
        // battered through its own, and is whole for the next waves).
        self.heal(|side| side == ending);
        self.note_sightings();
        // The sides still in; the game is over before none are.
        for _ in 0..self.order.len() * 2 {
            self.current += 1;
            if self.current >= self.order.len() {
                self.current = 0;
                self.round += 1;
                if self.setup.order == TurnOrder::Drawn {
                    self.rng.shuffle(&mut self.order);
                }
                // The units of the sides that have no turn (nobody's, a
                // side out or with no seat) heal as a round ends.
                let (order, out) = (self.order.clone(), self.out.clone());
                self.heal(|side| !order.contains(&side) || out.contains(&side));
            }
            if !self.out.contains(&self.order[self.current]) {
                break;
            }
        }
        let side = self.player();
        let round = self.round;
        self.raised.clear();
        self.fights.retain(|(_, fought, _)| fought + REPLAY_ROUNDS > round);
        events.push(Event::Began { side, round: self.round });
        if self.round > 1 {
            self.map.reset_move_points(side);
            events.extend(self.pay_day(side));
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
            let city =
                self.map.units().get(unit).and_then(|u| self.map.city_at(u.pos())).map(|c| (c.id, c.sends.clone()));
            if let Some((city, sends)) = city {
                events.push(Event::Built { city, unit });
                // Its city sends it on, and its queue goes on.
                self.send_new(side, unit, &sends);
                if let Some(c) = self.map.city_mut(city) {
                    c.built_one();
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
    /// within [`UNIT_SIGHT`] of its units and [`CITY_SIGHT`] of its cities,
    /// now (the owner's word, 2026-11-22: "you should only see what the
    /// unit can see at the time and leave ghosts vs leave a lingering
    /// visibility trail"; and, while an order is carried out, what its units
    /// see on their way: what is there is noted as they pass). (Nobody,
    /// `Side::Grey`, sees everything.)
    pub fn sight(&self, side: Side) -> Sight {
        if !self.setup.fog.on() || side == Side::Grey {
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
        // And what its units see on their way, in the order being carried
        // out.
        let size = seen.len();
        for trail in self.trail.get(side.index()).into_iter().filter(|t| t.len() == size) {
            for (s, t) in seen.iter_mut().zip(trail) {
                *s |= *t;
            }
        }
        // With total fog, it knows what it has ever seen.
        let known = (self.setup.fog == Fog::Total).then(|| {
            let mut known = seen.clone();
            if let Some(explored) = self.explored.get(side.index()).filter(|e| e.len() == known.len()) {
                for (k, e) in known.iter_mut().zip(explored) {
                    *k |= *e;
                }
            }
            known
        });
        let held = self.held.get(side.index()).cloned().unwrap_or_default();
        Sight { cols, seen: Some(seen), known, ghosts: self.ghosts(side).to_vec(), held }
    }

    /// The other sides' stacks `side` remembers where it saw them last (fog
    /// of war): those in its sight as they are now, the others as they were.
    /// What shows of them in the fog is their ghost.
    pub fn ghosts(&self, side: Side) -> &[Ghost] {
        self.memory.get(side.index()).map_or(&[], Vec::as_slice)
    }

    /// The stack `side` remembers on `hex`, if it remembers one there.
    pub fn ghost_at(&self, side: Side, hex: Hex) -> Option<&Ghost> {
        self.ghosts(side).iter().find(|g| g.at == hex)
    }

    /// Where `unit` can go this turn as its side knows the map (`sight`, the
    /// side's): with fog of war, the other sides' units stop it only where its
    /// side sees them; in the fog only cities do; with total fog, land the
    /// side has never seen is shut to it. The rules go by what is known: a
    /// unit seen nowhere is met on the way (`move_units`). A ship goes into
    /// its side's cities only; a unit that goes on land steps aboard a ship of
    /// its side with room for it (`Game::ground`).
    pub fn known_range(&self, unit: &Unit, sight: &Sight) -> MoveRange {
        self.known_range_by(unit, unit.move_cost, sight)
    }

    /// [`Game::known_range`] at the prices of the table `table` (a commander
    /// that flies with flyers goes at theirs: `Game::group`).
    fn known_range_by(&self, unit: &Unit, table: MoveCostId, sight: &Sight) -> MoveRange {
        let Some(costs) = self.map.units().move_cost(table) else { return compute_range(&self.map, unit) };
        let (pass, steps) = self.ground(unit, costs, sight);
        reach_with(&self.map, unit.pos(), &steps, unit.movement, &pass)
    }

    /// What each hex is to `unit` going at the prices `costs`, as its side
    /// knows the map (`sight`), and what a step costs it: a ship goes into its
    /// side's cities only (its ports); a unit that goes on land and cannot
    /// swim steps aboard a ship of its side that has room for it, on open
    /// water, for what a step into a city costs it, and its move ends there.
    fn ground<'a>(
        &'a self,
        unit: &'a Unit,
        costs: &'a MoveCost,
        sight: &'a Sight,
    ) -> (impl Fn(Hex) -> Passage + 'a, impl Fn(Hex, Hex) -> Option<u16> + 'a) {
        let side = unit.side();
        let travel = self.map.travel(unit.id());
        let boards = travel == Travel::Land && costs.water == 0;
        let leads = self.map.is_commander(unit.id());
        let aboard = move |hex: Hex| {
            boards && self.map.open_water(hex) && sight.knows(hex) && self.room_aboard(side, hex, leads)
        };
        let pass = move |hex: Hex| {
            if aboard(hex) {
                return Passage::Stop;
            }
            let port = self.map.city_at(hex).is_none_or(|c| c.side == side);
            if travel == Travel::Sea && !port { Passage::Blocked } else { self.known_passage(side, hex, sight) }
        };
        // Into another side's units it sees: an attack, which costs what it
        // costs (`ATTACK_COST`) on any ground it could go onto.
        let steps = move |from: Hex, to: Hex| {
            if aboard(to) {
                Some(costs.city.max(1))
            } else if hostile(&self.map, side, to) && sight.sees(to) {
                step_cost(&self.map, costs, from, to).map(|_| ATTACK_COST)
            } else {
                step_cost(&self.map, costs, from, to)
            }
        };
        (pass, steps)
    }

    /// Those of `group` that move, each with its table's prices, in the
    /// group's order. `None` if one is gone.
    fn walkers(&self, group: &Group) -> Option<Vec<(&Unit, &MoveCost)>> {
        let units = self.map.units();
        group.movers.iter().map(|(id, table)| Some((units.get(*id)?, units.move_cost(*table)?))).collect()
    }

    /// What each hex is to the units of `group` going as one, as their side
    /// knows the map, and what a step weighs for them (`weigh`): a hex shut
    /// to one of them is shut to all, and a step one of them cannot make
    /// none makes (the owner's word, 2026-11-22: "a mixed stack should
    /// always stay and move together even with flyers and non-flyers": the
    /// flyers do not fly where the others cannot walk). Each still pays its
    /// own price for the way.
    fn ground_together<'a>(&'a self, group: &Group, sight: &'a Sight, weigh: Weigh) -> Option<Ground<'a>> {
        let walkers = self.walkers(group)?;
        let led = walkers.iter().any(|(u, _)| !self.map.is_commander(u.id()));
        let mut passes = Vec::new();
        let mut steps = Vec::new();
        for (i, (unit, costs)) in walkers.iter().enumerate() {
            let (pass, step) = self.ground(unit, costs, sight);
            passes.push(pass);
            // What its cost is multiplied by and divided by, to weigh it.
            let scale = match &weigh {
                Weigh::Now(left) => (SHARE, left.get(i).copied().unwrap_or(0), true),
                Weigh::Far => (PACE, unit.speed(), !(led && self.map.is_commander(unit.id()))),
            };
            steps.push((step, scale));
        }
        let now = matches!(weigh, Weigh::Now(_));
        let pass = move |hex: Hex| {
            let mut worst = Passage::Open;
            for pass in &passes {
                match pass(hex) {
                    Passage::Blocked => return Passage::Blocked,
                    Passage::Stop => worst = Passage::Stop,
                    Passage::Open => {}
                }
            }
            worst
        };
        let step = move |from: Hex, to: Hex| {
            let mut worst = 0u32;
            for (step, (times, over, counts)) in &steps {
                let cost = u32::from(step(from, to)?);
                if now && (*over == 0 || cost > *over) {
                    return None;
                }
                if *counts {
                    let weight = cost.saturating_mul(*times);
                    worst = worst.max(if now { weight / over } else { weight.div_ceil((*over).max(1)) });
                }
            }
            Some(worst.clamp(1, u32::from(u16::MAX)) as u16)
        };
        Some((Box::new(pass), Box::new(step)))
    }

    /// The ground of `group` for a way however far: its lead's, if all of
    /// it goes at one table's prices; else the ground of them all
    /// (`ground_together`), where a step weighs what it takes the slowest.
    fn ground_far<'a>(&'a self, group: &Group, sight: &'a Sight) -> Option<Ground<'a>> {
        if group.mixed() {
            return self.ground_together(group, sight, Weigh::Far);
        }
        let (lead, table) = group.lead(&self.map)?;
        let costs = self.map.units().move_cost(table)?;
        let (pass, steps) = self.ground(lead, costs, sight);
        Some((Box::new(pass), Box::new(steps)))
    }

    /// The way of `group` (a stack on a hex) to `to`, however far, as its
    /// side knows the map: each hex after its own, with what it costs so
    /// far, to be held against the movement of the group's lead (the one
    /// with the fewest points): no more than that, and every one of them
    /// can pay to get there this turn. For a group that goes at one table's
    /// prices that is the lead's cost on its cheapest way; for a mixed one
    /// (`Group::mixed`) the way is one all of them can go, the quickest for
    /// the slowest, and the cost says how much the one worst off is short.
    fn way_together(&self, group: &Group, to: Hex, sight: &Sight) -> Option<Vec<(Hex, u32)>> {
        let (lead, table) = group.lead(&self.map)?;
        if !group.mixed() {
            return self.known_path_by(lead, table, to, u32::MAX, sight);
        }
        let (pass, steps) = self.ground_together(group, sight, Weigh::Far)?;
        let way = path_with(&self.map, (lead.pos(), to), &*steps, u32::MAX, &*pass)?;
        let hexes: Vec<Hex> = way.into_iter().map(|(hex, _)| hex).collect();
        let costs = self.costs_each(group, lead.pos(), &hexes, sight)?;
        let walkers = self.walkers(group)?;
        let short = |spent: &Vec<u32>| {
            let over = walkers.iter().zip(spent).map(|((u, _), c)| i64::from(*c) - i64::from(u.movement)).max();
            (i64::from(lead.movement) + over.unwrap_or(0)).max(0) as u32
        };
        Some(hexes.into_iter().zip(&costs).map(|(hex, spent)| (hex, short(spent))).collect())
    }

    /// What going from `from` over `hexes`, one after the other, has cost
    /// each of `group` that moves (in its order) by each hex, as their side
    /// knows the map. `None` if one of them cannot go that way.
    fn costs_each(&self, group: &Group, from: Hex, hexes: &[Hex], sight: &Sight) -> Option<Vec<Vec<u32>>> {
        let walkers = self.walkers(group)?;
        let grounds: Vec<_> = walkers.iter().map(|(unit, costs)| self.ground(unit, costs, sight)).collect();
        let mut spent = vec![0u32; walkers.len()];
        let mut at = from;
        let mut all = Vec::with_capacity(hexes.len());
        for hex in hexes {
            for (i, (_, steps)) in grounds.iter().enumerate() {
                spent[i] += u32::from(steps(at, *hex)?);
            }
            all.push(spent.clone());
            at = *hex;
        }
        Some(all)
    }

    /// True if a ship of `side` on `hex` has room for one more who rides
    /// (`Game::rides`); for a commander, if a ship is there and no commander.
    fn room_aboard(&self, side: Side, hex: Hex, leads: bool) -> bool {
        let own: Vec<UnitId> = self.map.units_at(hex).iter().copied().filter(|id| id.side == side).collect();
        if !own.iter().any(|id| self.map.travel(*id) == Travel::Sea) {
            return false;
        }
        if leads {
            return !own.iter().any(|id| self.map.is_commander(*id));
        }
        let capacity: u32 = own.iter().map(|id| self.map.carries(*id)).sum();
        let riders = own.iter().filter(|id| self.rides(**id) && !self.map.is_commander(**id)).count();
        (riders as u32) < capacity
    }

    /// True if a unit is on open water only aboard a ship (a commander: or
    /// with flyers): it goes on land, and cannot swim.
    fn rides(&self, id: UnitId) -> bool {
        let units = self.map.units();
        let swims = units.get(id).and_then(|u| units.move_cost(u.move_cost)).is_some_and(|c| c.water > 0);
        self.map.travel(id) == Travel::Land && !swims
    }

    /// True if `units`, a side's, can be on `hex` together: on open water,
    /// those who ride fit in its ships, and a commander who rides has a ship
    /// or flyers with it.
    fn afloat(&self, hex: Hex, units: &[UnitId]) -> bool {
        if !self.map.open_water(hex) {
            return true;
        }
        let (mut capacity, mut keels, mut riders, mut leaders) = (0, 0, 0, 0);
        for id in units {
            match self.map.travel(*id) {
                Travel::Sea => (keels, capacity) = (keels + 1, capacity + self.map.carries(*id)),
                Travel::Air => keels += 1,
                Travel::Land if self.rides(*id) && self.map.is_commander(*id) => leaders += 1,
                Travel::Land if self.rides(*id) => riders += 1,
                Travel::Land => {}
            }
        }
        riders <= capacity && (leaders == 0 || keels > 0)
    }

    /// `side`'s units on `hex` once `leaving` have gone and `arriving` come.
    fn there_after(&self, side: Side, hex: Hex, leaving: &[UnitId], arriving: &[UnitId]) -> Vec<UnitId> {
        let here = self.map.units_at(hex).iter().copied().filter(|id| id.side == side && !leaving.contains(id));
        here.chain(arriving.iter().copied().filter(|id| !self.map.units_at(hex).contains(id))).collect()
    }

    /// After a battle, `side`'s units on each of `hexes` that cannot stay
    /// there go down with their ships: those who ride beyond what the ships
    /// left carry (the last aboard first), and a commander who rides with no
    /// ship or flyer left.
    fn drown(&mut self, side: Side, hexes: &[Hex]) -> Vec<Event> {
        let mut events = Vec::new();
        let mut done: Vec<Hex> = Vec::new();
        for hex in hexes.iter().copied() {
            if done.contains(&hex) {
                continue;
            }
            done.push(hex);
            let own = self.there_after(side, hex, &[], &[]);
            if self.afloat(hex, &own) {
                continue;
            }
            let capacity: u32 = own.iter().map(|id| self.map.carries(*id)).sum();
            let keels = own.iter().any(|id| self.map.travel(*id) != Travel::Land);
            let riding: Vec<UnitId> = own.iter().copied().filter(|id| self.rides(*id)).collect();
            let (leaders, riders): (Vec<UnitId>, Vec<UnitId>) =
                riding.into_iter().partition(|id| self.map.is_commander(*id));
            let mut gone: Vec<UnitId> = riders.into_iter().skip(capacity as usize).collect();
            if !keels {
                gone.extend(leaders);
            }
            let units = self.map.units();
            let dead: Vec<Fallen> = gone
                .iter()
                .filter_map(|id| units.get(*id))
                .map(|u| Fallen { side: u.side(), name: u.title(), person: u.given_name.is_some() })
                .collect();
            for id in gone {
                self.map.kill_unit(id);
            }
            if !dead.is_empty() {
                events.push(Event::Drowned { side, at: hex, dead });
            }
        }
        events
    }

    /// How `ids`, a side's, go together: with ships among them, the ships
    /// carry those who ride (on a hex where one of the ships is, as many as
    /// the ships carry; a commander besides) and the flyers fly; flyers and a
    /// commander with no one else: the commander flies with them, at the
    /// prices of the first of them; else each goes its own way. Why not, if
    /// they cannot go together.
    fn group(&self, ids: &[UnitId]) -> Result<Group, String> {
        let units = self.map.units();
        let list: Vec<&Unit> = ids.iter().filter_map(|id| units.get(*id)).collect();
        let by = |travel: Travel| list.iter().copied().filter(move |u| self.map.travel(u.id()) == travel);
        let (ships, flyers, others): (Vec<&Unit>, Vec<&Unit>, Vec<&Unit>) =
            (by(Travel::Sea).collect(), by(Travel::Air).collect(), by(Travel::Land).collect());
        let own = |u: &&Unit| (u.id(), u.move_cost);
        if !ships.is_empty() {
            let riders = others.iter().filter(|u| !self.map.is_commander(u.id())).count() as u32;
            let capacity: u32 = ships.iter().map(|u| self.map.carries(u.id())).sum();
            if riders > capacity {
                return Err(format!("the ships carry {capacity}, not {riders}"));
            }
            if let Some(u) = others.iter().find(|u| !ships.iter().any(|s| s.pos() == u.pos())) {
                return Err(format!("{} is not aboard: a ship takes those on its own hex", u.title()));
            }
            let movers = ships.iter().chain(&flyers).map(own).collect();
            return Ok(Group { movers, carried: others.iter().map(|u| u.id()).collect() });
        }
        let leaders_only = !others.is_empty() && others.iter().all(|u| self.map.is_commander(u.id()));
        if let (Some(wing), true) = (flyers.first(), leaders_only) {
            let movers = flyers.iter().map(own).chain(others.iter().map(|u| (u.id(), wing.move_cost))).collect();
            return Ok(Group { movers, carried: Vec::new() });
        }
        Ok(Group { movers: list.iter().map(own).collect(), carried: Vec::new() })
    }

    /// Where `ids` can go together this turn, as their side knows the map:
    /// where every one of them that moves can get (those carried go with
    /// their ships). `None` if they cannot go together.
    fn group_range(&self, ids: &[UnitId], sight: &Sight) -> Option<MoveRange> {
        let group = self.group(ids).ok()?;
        let mut range: Option<MoveRange> = None;
        for (id, table) in &group.movers {
            let one = self.known_range_by(self.map.units().get(*id)?, *table, sight);
            range = Some(match range {
                Some(all) => all.both(&one),
                None => one,
            });
        }
        range
    }

    /// Why the selected units cannot go anywhere together, if they cannot
    /// (too many for the ships, one not aboard).
    pub fn selection_problem(&self) -> Option<String> {
        (!self.selected.is_empty()).then(|| self.group(&self.selected).err()).flatten()
    }

    /// What `hex` is to a unit of `side` that sees as `sight` (`passage`, as
    /// far as it knows).
    fn known_passage(&self, side: Side, hex: Hex, sight: &Sight) -> Passage {
        if !sight.knows(hex) {
            Passage::Blocked
        } else if sight.sees(hex) {
            passage(&self.map, side, hex)
        } else if self.map.city_at(hex).is_some_and(|c| c.side != side) {
            Passage::Stop
        } else {
            Passage::Open
        }
    }

    /// `unit`'s way to `to` at the prices of `table`, as its side knows the
    /// map, at most `limit` points: each hex after its own, with the cost so
    /// far.
    fn known_path_by(
        &self,
        unit: &Unit,
        table: MoveCostId,
        to: Hex,
        limit: u32,
        sight: &Sight,
    ) -> Option<Vec<(Hex, u32)>> {
        let costs = self.map.units().move_cost(table)?;
        let (pass, steps) = self.ground(unit, costs, sight);
        path_with(&self.map, (unit.pos(), to), &steps, limit, &pass)
    }

    /// The way `unit` would take to `to` this turn, as its side knows the
    /// map (`sight`): each hex after its own, with the cost so far. `None`
    /// if it cannot get there with the movement it has. (In rounds: the way
    /// a plan to go there gives it, `Unit::way`.)
    pub fn known_way(&self, unit: &Unit, to: Hex, sight: &Sight) -> Option<Vec<(Hex, u32)>> {
        self.known_path_by(unit, unit.move_cost, to, unit.movement, sight)
    }

    /// The edge of the land `unit`'s side knows (`Sight::frontier`) that it
    /// can get to over that land, in as many turns as it takes, and how much
    /// that costs it, the cheapest first: where it goes to see more (total
    /// fog of war). Nothing without total fog.
    pub fn frontier_for(&self, unit: &Unit, sight: &Sight) -> Vec<(Hex, u32)> {
        let edge = sight.frontier(&self.map);
        if edge.is_empty() {
            return Vec::new();
        }
        let mut far = unit.clone();
        far.movement = u32::MAX / 2;
        let range = self.known_range(&far, sight);
        let mut ways: Vec<(Hex, u32)> = edge.into_iter().filter_map(|h| range.cost(h).map(|c| (h, c))).collect();
        ways.sort_by_key(|(h, cost)| (*cost, h.row, h.col));
        ways
    }

    /// Where `unit` gets to this turn on its way toward `target` (see
    /// `step_toward_with`), as its side knows the map (`sight`).
    pub fn known_step_toward(
        &self,
        unit: &Unit,
        target: Hex,
        room: &dyn Fn(Hex) -> bool,
        sight: &Sight,
    ) -> Option<(Hex, u32)> {
        self.known_step_toward_by(unit, unit.move_cost, (target, room), sight)
    }

    /// [`Game::known_step_toward`] at the prices of `table`.
    fn known_step_toward_by(
        &self,
        unit: &Unit,
        table: MoveCostId,
        (target, room): (Hex, &dyn Fn(Hex) -> bool),
        sight: &Sight,
    ) -> Option<(Hex, u32)> {
        let costs = self.map.units().move_cost(table)?;
        let (pass, steps) = self.ground(unit, costs, sight);
        step_toward_by(&self.map, (unit, target), room, (&pass, &steps))
    }

    /// What each side sees now goes into what it remembers: of the hexes it
    /// sees, the other sides' stacks there now (or none), and who holds the
    /// cities (and of its own, who took them); of those it does not, what it
    /// saw last; with total fog, all it has seen. Called after every order
    /// (in rounds, after every step as the round plays out): what the
    /// order's units saw on their way is noted with it, and from then on a
    /// side sees only what is in its sight now.
    fn note_sightings(&mut self) {
        if !self.setup.fog.on() {
            return;
        }
        self.note_seen();
        for trail in &mut self.trail {
            trail.clear();
        }
    }

    fn note_seen(&mut self) {
        let round = self.round;
        let sides: Vec<Side> = self.setup.seats.iter().map(|s| s.side).collect();
        for side in sides {
            let sight = self.sight(side);
            let mut stacks: Vec<Ghost> = Vec::new();
            for unit in self.map.units().iter().filter(|u| u.side() != side && self.map.contains(u.pos())) {
                if !sight.sees(unit.pos()) {
                    continue;
                }
                match stacks.iter_mut().find(|g| g.at == unit.pos()) {
                    Some(g) => g.count += 1,
                    None => stacks.push(Ghost { at: unit.pos(), side: unit.side(), count: 1, round }),
                }
            }
            // A side is told when it loses a city: it knows who took it.
            let holders: Vec<(Hex, Side, u32)> = self
                .map
                .cities()
                .iter()
                .filter(|c| sight.sees(c.pos) || c.side == side || sight.held(c.pos).is_some_and(|(s, _)| s == side))
                .map(|c| (c.pos, c.side, c.walls))
                .collect();
            if let Some(memory) = self.memory.get_mut(side.index()) {
                memory.retain(|g| !sight.sees(g.at));
                memory.extend(stacks);
            }
            if let Some(held) = self.held.get_mut(side.index()) {
                held.retain(|(at, ..)| !holders.iter().any(|(h, ..)| h == at));
                held.extend(holders.into_iter().map(|(at, holder, walls)| (at, holder, round, walls)));
            }
            if let (Some(known), Some(explored)) = (sight.known, self.explored.get_mut(side.index())) {
                *explored = known;
            }
        }
    }

    /// `side`'s units went along `way` (the hexes after where they stood):
    /// with fog of war, what they saw on the way is seen until the order
    /// has been carried out and what was there has been noted
    /// (`note_sightings`).
    fn see_along(&mut self, side: Side, way: &[Hex]) {
        if !self.setup.fog.on() || way.is_empty() {
            return;
        }
        let (rows, cols) = (self.map.rows(), self.map.cols());
        let size = usize::from(rows) * usize::from(cols);
        let Some(trail) = self.trail.get_mut(side.index()) else { return };
        if trail.len() != size {
            *trail = vec![false; size];
        }
        let r = UNIT_SIGHT as i32;
        for at in way {
            for row in (i32::from(at.row) - r).max(0)..=(i32::from(at.row) + r).min(i32::from(rows) - 1) {
                for col in (i32::from(at.col) - r).max(0)..=(i32::from(at.col) + r).min(i32::from(cols) - 1) {
                    let hex = Hex::new(row as u16, col as u16);
                    if at.distance(hex) <= UNIT_SIGHT {
                        trail[row as usize * usize::from(cols) + col as usize] = true;
                    }
                }
            }
        }
    }

    /// The other sides that saw `side`'s stack of `count` go along `way` (its
    /// hex first) and out of their sight remember it where it went into the
    /// fog.
    fn seen_leaving(&mut self, side: Side, way: &[Hex], count: usize) {
        if !self.setup.fog.on() || way.len() < 2 {
            return;
        }
        let round = self.round;
        let others: Vec<Side> = self.setup.seats.iter().map(|s| s.side).filter(|s| *s != side).collect();
        for other in others {
            let sight = self.sight(other);
            let Some(last) = way.iter().rposition(|h| sight.sees(*h)) else { continue };
            let Some(&into) = way.get(last + 1) else { continue };
            if let Some(memory) = self.memory.get_mut(other.index()) {
                memory.retain(|g| g.at != into);
                memory.push(Ghost { at: into, side, count, round });
            }
        }
    }

    /// The game as `side` knows it: what the computer's players are given
    /// (`agents::play_turn`), so that they know what a person in their place
    /// is told. With fog of war, the other sides' units out of its sight are
    /// not in it (what it remembers of them is: `Sight::ghosts`), and a city
    /// in the fog is held by who held it when the side saw it last (nobody,
    /// if it never did). With total fog, land it has never seen is grass with
    /// no road, and a city there is nobody's, with no name, walls or taxes:
    /// `Sight::knows` says which (a program is sent none of it). Fog
    /// or not, it is not told what the other sides' cities build (nor what
    /// they can) and where they send it, the other sides' gold, their units'
    /// standing orders and the moves they have left; nor the history. Its
    /// chance is a generator of its own, seeded from the round and the side:
    /// a battle tried in the view says nothing of how the game's will go.
    /// (What it does keep of the other sides' units in sight, their kinds
    /// and strengths, is what the odds before an attack tell a person.)
    pub fn view_for(&self, side: Side) -> Game {
        let mut view = self.clone();
        (view.history, view.fights, view.fought, view.undo) = (Vec::new(), Vec::new(), Vec::new(), None);
        view.journal = None;
        (view.watched, view.journal, view.frozen) = (None, None, false);
        for (i, ((memory, trail), (held, explored))) in view
            .memory
            .iter_mut()
            .zip(view.trail.iter_mut())
            .zip(view.held.iter_mut().zip(view.explored.iter_mut()))
            .enumerate()
        {
            if i != side.index() {
                memory.clear();
                trail.clear();
                held.clear();
                explored.clear();
            }
        }
        view.rng = Rng::new((u64::from(self.round) << 8) ^ (side.index() as u64) ^ 0x005e_ed0f_1e55);
        for (i, points) in view.rally_points.iter_mut().enumerate() {
            if i != side.index() {
                *points = [None; RALLY_POINTS];
            }
        }
        view.deselect();
        let sight = self.sight(side);
        let theirs: Vec<(UnitId, bool)> =
            view.map.units().iter().filter(|u| u.side() != side).map(|u| (u.id(), sight.sees(u.pos()))).collect();
        for (id, seen) in theirs {
            if !seen {
                view.map.kill_unit(id);
            } else if let Some(unit) = view.map.unit_mut(id) {
                (unit.stalled, unit.moved, unit.done, unit.defending) = (false, false, false, false);
                (unit.route, unit.bound) = (Vec::new(), None);
                unit.stand_down();
                (unit.clock, unit.step, unit.halted) = (0, None, false);
                unit.movement = unit.base_move_points;
            }
        }
        let cities: Vec<(CityId, Side, bool, Option<u32>)> = view
            .map
            .cities()
            .iter()
            .filter(|c| c.side != side)
            .map(|c| {
                let walls = if sight.sees(c.pos) { None } else { sight.walls_seen(c.pos) };
                (c.id, sight.holder(c), sight.knows(c.pos), walls)
            })
            .collect();
        for (id, holder, known, walls) in cities {
            if let Some(city) = view.map.city_mut(id) {
                (city.building, city.progress) = (None, 0);
                (city.sends, city.queue, city.resting) = (Sends::Stay, Queue::default(), false);
                city.units.clear();
                city.side = holder;
                // In the fog: its walls as they were when it was seen.
                city.walls = walls.unwrap_or(city.walls);
                if !known {
                    (city.side, city.walls, city.income) = (Side::Grey, 0, 0);
                    city.name.clear();
                }
            }
        }
        if self.setup.fog == Fog::Total {
            let hexes = (0..self.map.rows()).flat_map(|row| (0..self.map.cols()).map(move |col| Hex::new(row, col)));
            for hex in hexes.filter(|h| !sight.knows(*h)) {
                if let Some(tile) = view.map.tile_mut(hex) {
                    (tile.kind, tile.road) = (TileType::Grass, false);
                }
            }
        }
        for (i, gold) in view.treasury.iter_mut().enumerate() {
            if i != side.index() {
                *gold = 0;
            }
        }
        if let Some(home) = self.capital(side) {
            view.map.cursor = home;
        }
        view
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
        self.onward = false;
    }

    /// In rounds, units that were just given a plan stay selected to be
    /// sent on from where it ends (the owner's word, 2026-11-22: "setting a
    /// goto point could leave the stack active with MP to spend from that
    /// point"): `range` is where all of them can go from there with what
    /// the plan's way leaves them, and a hex chosen in it is a waypoint
    /// more (`Order::Onward`). False, and nothing is selected, if they
    /// have no plan together, or can go nowhere from its end. Selecting
    /// them any other way starts afresh: from where they stand, and a hex
    /// chosen then is a new plan.
    pub fn select_onward(&mut self, units: &[UnitId]) -> bool {
        self.deselect();
        if !self.rounds() || units.is_empty() || !units.iter().all(|id| self.own(*id)) {
            return false;
        }
        self.onward = true;
        let there = self.select(units.to_vec()) && self.range.as_ref().is_some_and(|r| r.reachable().next().is_some());
        if !there {
            self.deselect();
        }
        there
    }

    /// Where the selected units go on from, if they are selected to be sent
    /// on from their plan's end (`select_onward`).
    pub fn onward_from(&self) -> Option<Hex> {
        self.onward.then(|| self.plan_end(&self.selected)).flatten()
    }

    /// Select one unit by name (a slot of the stack was chosen), and no other.
    /// False, and nothing changes, if there is no such unit any more, or it is
    /// not the side's whose turn it is.
    pub fn select_unit(&mut self, id: UnitId) -> bool {
        if !self.own(id) {
            return false;
        }
        self.onward = false;
        self.select(vec![id])
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
            // (In rounds, one that comes to join them is taken with them,
            // or left out.)
            let all: Vec<UnitId> = units.iter().copied().chain([id]).collect();
            if !(self.rounds() && self.gathers(&all)) {
                units.clear();
            }
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
        self.onward = false;
        self.select(units)
    }

    fn own(&self, id: UnitId) -> bool {
        self.map.units().get(id).is_some_and(|u| u.side() == self.player())
    }

    /// Select these units (they are there), with where all of them can go.
    fn select(&mut self, units: Vec<UnitId>) -> bool {
        let map = &self.map;
        if units.iter().any(|id| map.units().get(*id).is_none()) {
            return false;
        }
        let sight = units.first().and_then(|id| map.units().get(*id)).map_or(Sight::all(), |u| self.sight(u.side()));
        // (Units that cannot go together can go nowhere: `selection_problem`.)
        // In rounds, with those that come to join them (`rounds::plan`):
        // from the hex they gather on, with what the way there leaves each.
        // (Sent on from their plan's end, `select_onward`: from there, with
        // what the plan leaves them.)
        self.range = match (self.rounds(), self.onward) {
            (true, true) => self.onward_range(&units, &sight),
            (true, false) => self.joint_range(&units, &sight),
            (false, _) => self.group_range(&units, &sight),
        };
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
    /// - Units are selected and this is their hex: they stay selected.
    /// - Units are selected and all of them can reach the hex: they move there
    ///   (`Order::Move`: each pays its cost; into another side's units it is a
    ///   battle; a city may be taken) and are deselected; if the move is
    ///   refused, they stay selected and the outcome says why.
    /// - Units are selected and the hex is one of the side's rally points
    ///   with no other side's units it sees on it (`rally_target`): they are
    ///   sent there bound to the point, whether they reach it this turn or
    ///   not.
    /// - Units are selected and they cannot get there this turn: the hex's
    ///   own units, if it has any, are selected instead; else they are sent
    ///   there (`Order::GoTo`, over as many turns as it takes), or, if they
    ///   cannot be, stay selected and the outcome says why.
    /// - Nothing is selected and the hex has units of the side whose turn it
    ///   is: select them all, the stack (those with points left, if any have).
    ///
    /// C++: MapRenderer::selectHex. There a selected unit could go to any hex that
    /// the flood fill had not reached, mountains included, for free; here that
    /// drops the selection.
    pub fn select_hex(&mut self, hex: Hex) -> SelectOutcome {
        self.refresh();
        let Some(first) = self.selected() else { return self.select_stack(hex) };
        // Their own hex: they stay selected, as they are (some of the stack,
        // taken by their boxes, stay so); if they were to be sent on from
        // their plan's end, afresh: from where they stand.
        if self.map.units().get(first).is_some_and(|u| u.pos() == hex) {
            if self.onward {
                self.onward = false;
                self.select(self.selected.clone());
            }
            return SelectOutcome::Selected(first);
        }
        // Sent on from their plan's end (`select_onward`): a hex in reach
        // from there is a waypoint more; the end itself, nothing; one
        // beyond, where they go by way of the plan's stops, over as many
        // rounds as it takes.
        let onward = self.onward_from();
        if onward == Some(hex) {
            return SelectOutcome::Selected(first);
        }
        // A rally point of the side's: they are sent there (Go to), bound to
        // it, near or far (beside it, once it is full). In reach: a move
        // there (onto another of the side's stacks: they join it).
        let side = self.player();
        let to_point = self.rally_target(side, hex).is_some();
        let Some(cost) = self.range.as_ref().and_then(|r| r.cost(hex)).filter(|_| !to_point) else {
            let units = self.selected.clone();
            self.deselect();
            // Beyond their reach: another of the side's stacks there is
            // selected instead; else they are sent there, if the side's.
            if !to_point {
                match self.select_stack(hex) {
                    SelectOutcome::Nothing => {}
                    selected => return selected,
                }
            }
            if !units.iter().all(|id| self.own(*id)) {
                return SelectOutcome::Deselected;
            }
            // In rounds, land the side has never seen (total fog of war):
            // they set off toward it, to look, and find their way as the
            // round plays out.
            let via: Vec<Hex> = match onward {
                Some(_) => {
                    units.first().and_then(|id| self.map.units().get(*id)).map_or(Vec::new(), |u| u.route.clone())
                }
                None => Vec::new(),
            };
            // (A plan they had goes, as with any new plan.)
            let order = if self.rounds() && !self.sight(side).knows(hex) {
                Order::Toward { units: units.clone(), to: hex }
            } else {
                Order::GoTo { units: units.clone(), to: hex, via }
            };
            return match self.apply(order) {
                Ok(events) => SelectOutcome::Sent(events),
                Err(why) => {
                    self.onward = onward.is_some();
                    self.select(units);
                    SelectOutcome::Refused(why)
                }
            };
        };
        let units = self.selected.clone();
        let order = match onward {
            Some(_) => Order::Onward { units: units.clone(), to: hex },
            None => Order::Move { units: units.clone(), to: hex },
        };
        let done = self.apply(order);
        self.deselect();
        match done {
            Ok(events) if events.iter().any(|e| matches!(e, Event::Battle { .. })) => SelectOutcome::Fought(events),
            Ok(events) => SelectOutcome::Moved { unit: first, cost, events },
            // (A full stack, for one.) They stay selected, and it says why.
            Err(why) => {
                self.onward = onward.is_some();
                self.select(units);
                SelectOutcome::Refused(why)
            }
        }
    }

    /// Select the side's stack on a hex: those of its units with points left,
    /// or all of them if none has any.
    fn select_stack(&mut self, hex: Hex) -> SelectOutcome {
        self.onward = false;
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
        // Not those that defend, were put aside, have moved this turn or are
        // on a standing order (a march, a city's new units on their way, a
        // patrol, marauding), unless they are all it has (the stack's boxes
        // take them in one by one): a stack put aside in part was selected
        // whole again, and sent off, defenders and all; a stack that moved
        // onto another went on with it; a new unit on its way went with the
        // garrison, its order lost.
        let free = |u: &Unit| !u.defending && !u.done && !u.moved && (u.route.is_empty() || u.stalled) && !u.on_duty();
        let waiting: Vec<UnitId> = able.iter().copied().filter(|id| units.get(*id).is_some_and(free)).collect();
        // (At sea, all of them: the ships carry who has no points left. On
        // patrol or marauding, all of them too: they go as one.)
        // (With none that waits: those that moved, were put aside or are on
        // their way, not the defenders and the patrols beside them: the
        // review, 2026-11-23, a stack moved into a city took its garrison
        // along.)
        let loose: Vec<UnitId> =
            able.iter().copied().filter(|id| units.get(*id).is_some_and(|u| !u.defending && !u.on_duty())).collect();
        let mut stack = if self.map.open_water(hex) || patrol::together(&self.map, &own) {
            own
        } else if !waiting.is_empty() {
            waiting
        } else if !loose.is_empty() {
            loose
        } else if !able.is_empty() {
            able
        } else {
            own
        };
        // In rounds, with those sent to join them here: ordered on, they
        // all go as one stack once they are together (`rounds::plan`).
        if self.rounds() && !stack.is_empty() {
            let joining = self.joining(side, hex);
            let all: Vec<UnitId> = stack.iter().copied().chain(joining).collect();
            if self.gathers(&all) {
                stack = all;
            }
        }
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
        let hexes = self.stacks_of(side);
        let able = self.waiting_stacks(side);
        next_in(if able.is_empty() { &hexes } else { &able }, from, back)
    }

    /// The hexes with stacks of `side`'s, in reading order.
    pub fn stacks_of(&self, side: Side) -> Vec<Hex> {
        let map = &self.map;
        let mut hexes: Vec<Hex> = map.units().of_side(side).map(|u| u.pos()).filter(|h| map.contains(*h)).collect();
        hexes.sort_by_key(|h| (h.row, h.col));
        hexes.dedup();
        hexes
    }

    /// The hexes of `side`'s stacks that wait for orders, in reading order:
    /// one of its units can still go somewhere, its player is not done with
    /// it (nor has moved it this turn: a stack moved and stopped has had its
    /// orders), and it is on no standing order (or one that stalled), nor on
    /// patrol or marauding (unless it fought, or its patrol stalled).
    pub fn waiting_stacks(&self, side: Side) -> Vec<Hex> {
        let map = &self.map;
        let can_go = |hex: &Hex| {
            map.units_at(*hex).iter().filter_map(|id| map.units().get(*id)).any(|u| {
                u.side() == side
                    && (patrol::needs_word(u)
                        || u.movement > 0
                            && !u.done
                            && !u.moved
                            && !u.defending
                            && !u.on_duty()
                            && (u.route.is_empty() || u.stalled)
                            && compute_range(map, u).reachable().next().is_some())
            })
        };
        self.stacks_of(side).into_iter().filter(can_go).collect()
    }

    /// What `side` has: each of its stacks and cities, as a task (what it
    /// has left to do with it this turn), the most pressing first
    /// ([`Task::tier`]), then stacks before cities, then by place.
    pub fn tasks(&self, side: Side) -> Vec<Task> {
        let map = &self.map;
        let mut tasks: Vec<Task> = self
            .stacks_of(side)
            .into_iter()
            .map(|at| {
                let units: Vec<&Unit> = map
                    .units_at(at)
                    .iter()
                    .filter_map(|id| map.units().get(*id))
                    .filter(|u| u.side() == side)
                    .collect();
                let (state, about) = self.stack_task(&units);
                Task::Stack { at, units: about.len(), state }
            })
            .collect();
        for city in map.cities().iter().filter(|c| c.side == side && side != Side::Grey) {
            let state = match city.production() {
                None if city.resting => CityState::Resting,
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

    /// How a stack (a side's `units` on a hex) stands as far as orders go,
    /// and which of them that is about: those that wait for orders, if any
    /// do (not the ones beside them that defend, or are on their way);
    /// else those moved, put aside, on duty, marching, defending; else all
    /// of them.
    fn stack_task(&self, units: &[&Unit]) -> (StackState, Vec<UnitId>) {
        let map = &self.map;
        let can_go = |u: &&Unit| {
            u.movement > 0 && !u.defending && !u.on_duty() && compute_range(map, u).reachable().next().is_some()
        };
        let free: Vec<&Unit> = units.iter().copied().filter(can_go).collect();
        let those = |of: &[&Unit], are: &dyn Fn(&Unit) -> bool| -> Vec<UnitId> {
            of.iter().filter(|u| are(u)).map(|u| u.id()).collect()
        };
        let stalled = |u: &Unit| !u.route.is_empty() && u.stalled && !u.done && !u.moved;
        let waiting = |u: &Unit| !u.moved && !u.done && u.route.is_empty();
        let moved = |u: &Unit| !u.done && u.route.is_empty();
        // (Put aside with somewhere to go still, it is on its way: a plan
        // goes as the round plays out, whatever D said.)
        let aside = |u: &Unit| u.done && (u.route.is_empty() || u.stalled);
        if let Some(alarm) = patrol::alarm_state(units) {
            (alarm, those(units, &|u| patrol::needs_word(u)))
        } else if free.iter().any(|u| stalled(u)) {
            // (One its player put aside is done with, stopped or not.)
            (StackState::Stalled, those(&free, &stalled))
        } else if free.iter().any(|u| waiting(u)) {
            (StackState::Waiting, those(&free, &waiting))
        } else if free.iter().any(|u| moved(u)) {
            (StackState::MovesLeft, those(&free, &moved))
        } else if free.iter().any(|u| aside(u)) {
            (StackState::Done, those(&free, &aside))
        } else if let Some(duty) = patrol::duty_state(units) {
            (duty, those(units, &|u| u.on_duty()))
        } else if units.iter().any(|u| !u.route.is_empty()) {
            (StackState::Marching, those(units, &|u| !u.route.is_empty()))
        } else if units.iter().any(|u| u.defending) {
            (StackState::Defending, those(units, &|u| u.defending))
        } else {
            (StackState::Spent, those(units, &|_| true))
        }
    }

    /// The units of `side` on `at` that its task there is about
    /// (`Game::tasks`): of a stack some of which waits for orders, those
    /// that do.
    pub fn task_units(&self, side: Side, at: Hex) -> Vec<UnitId> {
        let map = &self.map;
        let units: Vec<&Unit> =
            map.units_at(at).iter().filter_map(|id| map.units().get(*id)).filter(|u| u.side() == side).collect();
        self.stack_task(&units).1
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
        let way = vec![START, Hex::new(50, 51), Hex::new(49, 52), target];
        let moved = Event::Moved { unit: id, from: START, to: target, cost: 6, way };
        assert_eq!(game.select_hex(target), SelectOutcome::Moved { unit: id, cost: 6, events: vec![moved] });
        assert_eq!(game.map.units().get(id).unwrap().movement, 14);
        assert_eq!(game.map.units().get(id).unwrap().pos(), target);
        assert_eq!(game.map.units_at(target), [id]);
        assert!(game.map.units_at(START).is_empty());
        assert!(game.selected().is_none() && game.range().is_none());
        assert!(!game.can_move_to(Hex::new(50, 54)));

        // Selecting again uses what is left; choosing its own hex selects it
        // still.
        assert_eq!(game.select_hex(target), SelectOutcome::Selected(id));
        assert!(game.range().unwrap().reachable().all(|(_, c)| c <= 14));
        assert_eq!(game.select_hex(target), SelectOutcome::Selected(id));
        assert_eq!(game.selected(), Some(id));
        assert_eq!(game.map.units().get(id).unwrap().movement, 14);
    }

    #[test]
    fn a_unit_cannot_go_where_it_cannot_reach() {
        let mut game = grassland();
        let &[id] = game.map.units_at(START) else { panic!() };
        // The C++ teleported to all of these for free (deviation D2); here
        // a mountain, or a hex off the map, is refused: the unit stays
        // selected, where it was.
        assert_eq!(game.select_hex(START), SelectOutcome::Selected(id));
        for hex in [Hex::new(0, 0), Hex::new(51, 51), Hex::new(100, 100)] {
            assert!(matches!(game.select_hex(hex), SelectOutcome::Refused(_)), "{hex}");
            assert_eq!(game.selected(), Some(id), "still selected");
            assert_eq!(game.map.units().get(id).unwrap().pos(), START);
            assert_eq!(game.map.units().get(id).unwrap().movement, 20);
        }
        // Grass beyond its reach this turn: it is sent there (Go to).
        assert!(matches!(game.select_hex(Hex::new(50, 70)), SelectOutcome::Sent(_)));
        assert_eq!(game.selected(), None);
        assert_eq!(game.map.units().get(id).unwrap().goto(), Some(Hex::new(50, 70)));
        // With nothing selected, a hex without units is nothing.
        assert_eq!(game.select_hex(Hex::new(0, 0)), SelectOutcome::Nothing);
    }

    #[test]
    fn a_go_to_that_has_not_gone_anywhere_yet_can_be_taken_back() {
        let mut game = grassland();
        let &[id] = game.map.units_at(START) else { panic!() };
        game.map.unit_mut(id).unwrap().movement = 0;
        assert_eq!(game.select_hex(START), SelectOutcome::Selected(id));
        assert_eq!(game.select_hex(Hex::new(50, 70)), SelectOutcome::Sent(Vec::new()));
        assert_eq!(game.map.units().get(id).unwrap().goto(), Some(Hex::new(50, 70)));
        assert!(game.can_undo());
        game.apply(Order::Undo).unwrap();
        assert_eq!(game.map.units().get(id).unwrap().goto(), None, "its order is gone");
        assert_eq!(game.selected(), Some(id), "and it is selected again");
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
    fn a_click_onto_a_full_stack_of_the_sides_refuses_the_move() {
        let mut game = grassland();
        let &[cav] = game.map.units_at(START) else { panic!() };
        let cost = game.map.units().get(cav).unwrap().move_cost;
        let target = Hex::new(50, 51);
        let mut guards = Vec::new();
        for i in 0..MAX_STACK {
            let id = game.map.new_unit(Side::Black, &format!("Guard {i}"), 1, 1, 0, cost);
            game.map.place_unit(id, target).unwrap();
            guards.push(id);
        }
        // A click on the side's other stack, in reach: a move onto it, to
        // join it (the screen asks first whether to take it instead):
        // refused, it is full; the cavalry stays selected.
        game.select_hex(START);
        assert_eq!(game.select_hex(target), SelectOutcome::Refused("there is no room there".to_string()));
        assert_eq!(game.selected(), Some(cav));
        assert!(game.map.units_at(target).len() == guards.len(), "nobody joined");
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
        assert!(matches!(game.select_hex(next_door), SelectOutcome::Refused(_)), "select_hex refreshes by itself");
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
        let dir = std::env::temp_dir().join(format!("hexlords-game-{}", std::process::id()));
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
