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
    },
    /// A side's unit went into another's (or nobody's) city with nobody in it.
    Taken {
        city: CityId,
        from: Side,
        by: Side,
    },
    /// A city finished what it builds: the unit stands in it.
    Built {
        city: CityId,
        unit: UnitId,
    },
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
