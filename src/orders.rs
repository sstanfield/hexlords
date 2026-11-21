//! What a side can do, as values: orders. The rules (`Game::apply`) carry an
//! order out, or refuse it, and say what came of it as events. A person's
//! clicks become orders, an agent makes them, and (later) a saved game and a
//! network game are made of them (`docs/gameplay.md`).

use crate::state::{Blessing, CityId, Hex, QueueEnd, QueueItem, Sends, Side, UnitId};

/// How many rally points a side has: 0 to 9.
pub const RALLY_POINTS: usize = 10;

/// One of a side's rally points (`Order::RallyPoint`): a hex its units are
/// sent to, by Go to or by its cities (`Sends::Point`), to gather into
/// stacks: once one is full, those that come after stand beside it. If it
/// says so they defend there when they get there (nobody need see to them,
/// until they are sent on); else they wait for orders.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct RallyPoint {
    pub at: Hex,
    pub defend: bool,
}

/// What a side asks for.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Order {
    /// Move units (a stack, or some of one) to a hex every one of them can
    /// reach this turn. Into another side's units: a battle.
    Move { units: Vec<UnitId>, to: Hex },
    /// Move units together as far as the slowest gets this turn on its
    /// cheapest way toward a hex, near or far.
    Toward { units: Vec<UnitId>, to: Hex },
    /// Set what a city builds, one after another, for ever: an index into
    /// what it can build; or nothing, on purpose (it is no task that needs
    /// orders). Its queue, if it had one, is gone.
    Build { city: CityId, what: Option<usize> },
    /// Set a city's production queue: its items in turn (each so many of a
    /// kind, sent somewhere), and what it does after the last. It goes on
    /// from the item it was building, if that is still there; an empty list
    /// takes the queue away (what it builds now it builds for ever).
    Queue { city: CityId, items: Vec<QueueItem>, end: QueueEnd },
    /// Send units to a hex, near or far, by way of the stops `via` in order
    /// (none: straight there): they go as far as the slowest gets this turn,
    /// and on at the end of each of their side's turns until they are
    /// there (a standing order; another order for them ends it). At a stop
    /// they go on to the next at once, with what movement they have left;
    /// at a stop on a temple (or the end), those it has not blessed seek its
    /// blessing, and stop there for the turn. To one of the side's rally
    /// points, they are bound to it (`Unit::bound`).
    GoTo { units: Vec<UnitId>, to: Hex, via: Vec<Hex> },
    /// The player is done with these units this turn: they are not offered
    /// again (Next stack) though they have moves left.
    Done { units: Vec<UnitId> },
    /// These units defend where they stand (`on`), turn after turn, until
    /// they are sent somewhere (a move or a standing order ends it), or no
    /// longer (`!on`). Next stack and the tasks pass a defender by.
    Defend { units: Vec<UnitId>, on: bool },
    /// Units on a temple seek its blessing: each the temple has not blessed
    /// yet is blessed (chance says how) and stops there for the turn (its
    /// movement is spent); one it has blessed already is not, and keeps its
    /// movement.
    Bless { units: Vec<UnitId> },
    /// Where a city sends the units it builds (`Sends`): nowhere (they stay
    /// in it), nowhere (they defend it), along a route (as `GoTo`), or to one
    /// of the side's rally points.
    Rally { city: CityId, to: Sends },
    /// Put one of the side's rally points (0 to 9) somewhere, or move it
    /// (the units bound to it go to its new place), or take it away (`None`:
    /// those bound to it stop and wait for orders, and the cities that sent
    /// their units there keep them).
    RallyPoint { n: u8, point: Option<RallyPoint> },
    /// Raise a city's walls a level, paid at once (`state::wall_price`):
    /// once a turn, up to `state::WALLS_CAP`.
    Walls { city: CityId },
    /// Send units home for good: they are gone, and their upkeep with them
    /// (to make room for better ones). A ship at sea goes only with those it
    /// carries, or with ships enough left to carry them.
    Disband { units: Vec<UnitId> },
    /// Take back the side's last order this turn, if it had no chance in it
    /// (a move that was no battle and put nobody out, a city's work set).
    Undo,
    /// The side has done all it means to this turn.
    EndTurn,
    /// Units on one hex patrol `waypoints` (on land the side has seen), turn
    /// after turn: the first of them first, round again after the last if it
    /// is the first again, else back the way they came. They go as far as
    /// the slowest gets this turn, and on at the end of each of their side's
    /// turns, on at once from a waypoint with what movement is left.
    /// With `attack` (in percent) they attack the other sides' stacks they
    /// see and can reach, at those odds or better, once a turn at most; else
    /// never. A battle (theirs, or one they are attacked in) and they need
    /// their player's word: `Done` lets them go on; another order for them
    /// ends the patrol.
    Patrol { units: Vec<UnitId>, waypoints: Vec<Hex>, attack: Option<u32> },
    /// The computer leads these units (`on`): it roams, hunts the other
    /// sides' units and takes their cities (`marauder`), until their player
    /// calls them back (`!on`) or gives them another order. After a battle
    /// they need their player's word, as on patrol.
    Maraud { units: Vec<UnitId>, on: bool },
    /// Marauding units go to a hex all of them can reach this turn (as
    /// `Move`; into another side's units, a battle), and maraud on: the
    /// computer's moves for them.
    Roam { units: Vec<UnitId>, to: Hex },
}

/// What happened.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Event {
    Moved {
        unit: UnitId,
        from: Hex,
        to: Hex,
        cost: u32,
    },
    /// A battle: `attacker`'s units went into `defender`'s on `at`.
    Battle {
        at: Hex,
        attacker: Side,
        defender: Side,
        /// How many fought on each side, and how many of them died.
        attackers: usize,
        defenders: usize,
        attackers_lost: usize,
        defenders_lost: usize,
        /// The attackers won: the defenders are gone, and the attackers that
        /// live stand on the hex (a `Moved` each follows).
        won: bool,
        /// Who died, in the order they fell (the units themselves are
        /// gone).
        dead: Vec<Fallen>,
    },
    /// A unit rose to a level (`name`, its title: "Captain Aldric"; a
    /// commander's levels are numbers, the others' ranks).
    Promoted {
        unit: UnitId,
        side: Side,
        name: String,
        level: u8,
        commander: bool,
    },
    /// A unit was blessed at a temple.
    Blessed {
        unit: UnitId,
        side: Side,
        name: String,
        blessing: Blessing,
    },
    /// A captain rose from the ranks of a stack that won a hard battle
    /// without one.
    Emerged {
        unit: UnitId,
        side: Side,
        at: Hex,
        name: String,
    },
    /// With fog of war, `side`'s units met another side's on their way, where
    /// they had not been seen: the move ended there, in a battle (the next
    /// event).
    Surprise {
        side: Side,
        at: Hex,
    },
    /// Units of `side` that rode ships that sank (or were left with none
    /// after a battle at sea) went down at `at`.
    Drowned {
        side: Side,
        at: Hex,
        dead: Vec<Fallen>,
    },
    /// Units on a standing order (`GoTo`) could not go on: the next step was
    /// into another side's units, or there is no way. They wait for orders.
    Stalled {
        side: Side,
        units: Vec<UnitId>,
        at: Hex,
        to: Hex,
    },
    /// A side's unit went into another's (or nobody's) city with nobody in it.
    /// Its walls fall a level: `walls`, what is left of them, if they fell.
    Taken {
        city: CityId,
        from: Side,
        by: Side,
        walls: Option<u32>,
    },
    /// A city's holder raised its walls a level, to `walls`, for `price`.
    WallsRaised {
        city: CityId,
        side: Side,
        walls: u32,
        price: u32,
    },
    /// A side sent units home (`Order::Disband`): what they were called.
    Disbanded {
        side: Side,
        names: Vec<String>,
    },
    /// A unit of `side`'s left it, unpaid: its treasury could not pay all
    /// the upkeep. (It is gone; this is its name.)
    Deserted {
        side: Side,
        name: String,
    },
    /// A city finished what it builds: the unit stands in it.
    Built {
        city: CityId,
        unit: UnitId,
    },
    /// The side's last order was taken back: all is as it was before it.
    Undone,
    /// A side's turn began.
    Began {
        side: Side,
        round: u32,
    },
    /// A side lost its last city: it is out of the game.
    Out {
        side: Side,
    },
    /// One side is left.
    Won {
        side: Side,
    },
}

/// A unit that fell in a battle: its side, what it was called (its kind,
/// "Militia", or a captain's title, "Captain Aldric"), and whether that is a
/// person's name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Fallen {
    pub side: Side,
    pub name: String,
    pub person: bool,
}

/// Something that happened, as the game keeps it (`Game::history`): the
/// event, the round, and the side whose turn it was.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Record {
    pub round: u32,
    pub turn: Side,
    pub event: Event,
}
