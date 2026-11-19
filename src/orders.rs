//! What a side can do, as values: orders. The rules (`Game::apply`) carry an
//! order out, or refuse it, and say what came of it as events. A person's
//! clicks become orders, an agent makes them, and (later) a saved game and a
//! network game are made of them (`docs/gameplay.md`).

use crate::state::{CityId, Hex, Side, UnitId};

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
        /// Who died, in the order they fell: each one's side and name (the
        /// units themselves are gone).
        dead: Vec<(Side, String)>,
    },
    /// A side's unit went into another's (or nobody's) city with nobody in it.
    Taken {
        city: CityId,
        from: Side,
        by: Side,
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

/// Something that happened, as the game keeps it (`Game::history`): the
/// event, the round, and the side whose turn it was.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Record {
    pub round: u32,
    pub turn: Side,
    pub event: Event,
}
