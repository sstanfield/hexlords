//! What a side can do, as values: orders. The rules (`Game::apply`) carry an
//! order out, or refuse it, and say what came of it as events. A person's
//! clicks become orders, an agent makes them, and (later) a saved game and a
//! network game are made of them (`docs/gameplay.md`).

use crate::state::{CityId, Hex, Side, UnitId};

/// What a side asks for.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Order {
    /// Move a unit to a hex it can reach this turn.
    Move { unit: UnitId, to: Hex },
    /// Move a unit as far as it gets this turn on its cheapest way toward a
    /// hex, near or far.
    Toward { unit: UnitId, to: Hex },
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
