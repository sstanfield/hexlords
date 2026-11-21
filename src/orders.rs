//! What a side can do, as values: orders. The rules (`Game::apply`) carry an
//! order out, or refuse it, and say what came of it as events. A person's
//! clicks become orders, an agent makes them, and (later) a saved game and a
//! network game are made of them (`docs/gameplay.md`).

use crate::state::{Blessing, CityId, Hex, Side, UnitId};

/// What a side asks for.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Order {
    /// Move units (a stack, or some of one) to a hex every one of them can
    /// reach this turn. Into another side's units: a battle.
    Move { units: Vec<UnitId>, to: Hex },
    /// Move units together as far as the slowest gets this turn on its
    /// cheapest way toward a hex, near or far.
    Toward { units: Vec<UnitId>, to: Hex },
    /// Set what a city builds: an index into what it can build, or nothing.
    Build { city: CityId, what: Option<usize> },
    /// Send units to a hex, near or far: they go as far as the slowest gets
    /// this turn, and on at the start of each of their side's turns until
    /// they are there (a standing order; another order for them ends it).
    GoTo { units: Vec<UnitId>, to: Hex },
    /// The player is done with these units this turn: they are not offered
    /// again (Next stack) though they have moves left.
    Done { units: Vec<UnitId> },
    /// Units on a temple seek its blessing: each the temple has not blessed
    /// yet is blessed (chance says how) and stops there for the turn (its
    /// movement is spent); one it has blessed already is not, and keeps its
    /// movement.
    Bless { units: Vec<UnitId> },
    /// Where a city sends the units it builds (they go there as `GoTo`), or
    /// nowhere.
    Rally { city: CityId, to: Option<Hex> },
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
