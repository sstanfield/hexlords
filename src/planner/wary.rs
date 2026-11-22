//! What the planner fears (the owner's word, 2026-11-22: "you mentioned
//! some AI improvements that would help turn mode, you can do them"): the
//! other sides' stacks it sees, where each could be, and how a stack on its
//! way keeps clear of those it could not beat. Made for rounds
//! (`super::rounds`), where it won games; played out in turns it won as
//! many, so it is the planner's way in both (`docs/agents.md` has the
//! numbers).
//!
//! - **The others' reach.** Every stack of another side it sees could be
//!   anywhere a turn's movement takes it ([`Foe`]): on the others' turns,
//!   in turns; in rounds at the moment its speed gets it there.
//! - **It does not stand where it would be beaten.** A stack on its way to
//!   gather, to look, to a temple or toward a city stops short of where a
//!   stack it fears could come to it (`wary`; how much it fears is its
//!   style's `nerve`), and one that would have nowhere better to stand
//!   stays where it is. In rounds, where everyone moves at once, it does
//!   not pass a hex such a stack could be on by then either. An attack is
//!   another matter: that, it means.
//!
//! Not a port: the C++ had no players.

use std::rc::Rc;

use super::{Planner, worth};
use crate::game::{Game, ROUND, Sight};
use crate::orders::Order;
use crate::state::{Hex, MAX_STACK, MoveRange, Side, Unit, UnitId, compute_range};

/// How much braver a long game makes it, in percent of its nerve a round
/// after `LATE`: two that fear each other would wait for ever.
const LATE: u32 = 40;
const LATE_NERVE: u32 = 2;

/// The other sides' stacks it sees, as last worked out, and when that was
/// (`Planner::spot`: the round, the side, and in turns the map's revision).
pub(super) type Spotted = ((u32, Side, u64), Rc<Vec<Foe>>);

/// Another side's stack it sees, and where it could be this round.
pub(super) struct Foe {
    pub(super) at: Hex,
    /// What its units are worth in battle (`worth`), together.
    worth: f64,
    /// Where the slowest of it gets with a turn's movement (and what it may
    /// have kept), and how fast that one is.
    range: MoveRange,
    speed: u32,
}

impl Foe {
    /// When it could be on `hex` at the earliest (`ROUND` is a round), if it
    /// can get there this round.
    pub(super) fn there_by(&self, hex: Hex) -> Option<u64> {
        if hex == self.at {
            return Some(0);
        }
        self.range.cost(hex).map(|cost| u64::from(cost) * ROUND / u64::from(self.speed.max(1)))
    }
}

/// The other sides' stacks `side` sees (not nobody's: those stay where they
/// are), each with its reach.
pub(super) fn foes(game: &Game, side: Side) -> Vec<Foe> {
    let map = &game.map;
    let mut stacks: Vec<(Hex, Vec<&Unit>)> = Vec::new();
    for unit in map.units().iter().filter(|u| u.side() != side && u.side() != Side::Grey && map.contains(u.pos())) {
        match stacks.iter_mut().find(|(at, _)| *at == unit.pos()) {
            Some((_, units)) => units.push(unit),
            None => stacks.push((unit.pos(), vec![unit])),
        }
    }
    let foe = |(at, units): (Hex, Vec<&Unit>)| {
        // At the pace of its slowest (a commander keeps up with the rest).
        let soldiers = units.iter().filter(|u| !map.is_commander(u.id())).min_by_key(|u| u.base_move_points);
        let slowest = soldiers.or(units.first())?;
        let mut fresh = (*slowest).clone();
        fresh.movement = fresh.base_move_points.saturating_add(2);
        let worth = units.iter().map(|u| worth(u.fighting_power(), game)).sum();
        Some(Foe { at, worth, range: compute_range(map, &fresh), speed: fresh.speed() })
    };
    stacks.into_iter().filter_map(foe).collect()
}

/// What the strongest of `foes` that could be on `hex` is worth: by the
/// moment `by`, or at any time this round.
pub(super) fn menace(foes: &[Foe], hex: Hex, by: Option<u64>) -> f64 {
    let there = |foe: &&Foe| foe.there_by(hex).is_some_and(|when| by.is_none_or(|by| when <= by));
    foes.iter().filter(there).map(|foe| foe.worth).fold(0.0, f64::max)
}

/// What `ids` are worth in battle, together.
pub(super) fn force(game: &Game, ids: &[UnitId]) -> f64 {
    ids.iter().filter_map(|id| game.map.units().get(*id)).map(|u| worth(u.fighting_power(), game)).sum()
}

/// How fast `ids` go as a stack: as their slowest (a commander keeps up
/// with the rest).
pub(super) fn pace(game: &Game, ids: &[UnitId]) -> u32 {
    let map = &game.map;
    let units = || ids.iter().filter_map(|id| map.units().get(*id));
    let soldiers = units().filter(|u| !map.is_commander(u.id())).map(|u| u.speed()).min();
    soldiers.or_else(|| units().map(|u| u.speed()).min()).unwrap_or(1)
}

/// True if there is room on `hex` for `ids` among those of their side that
/// are there (a stack's worth, and one commander), and nobody else's.
pub(super) fn room(game: &Game, side: Side, ids: &[UnitId], hex: Hex) -> bool {
    let map = &game.map;
    let there = map.units_at(hex);
    if there.iter().any(|id| id.side != side) {
        return false;
    }
    let leaders = there.iter().chain(ids).filter(|id| map.is_commander(**id)).count();
    there.iter().all(|id| ids.contains(id)) || (there.len() + ids.len() - leaders <= MAX_STACK && leaders <= 1)
}

impl Planner {
    /// How much a stack of another side may be worth, in percent of its
    /// own stack's worth, for that stack to go where the other could meet
    /// it: its style's nerve, and more as a long game goes on.
    pub(super) fn nerve(&self, game: &Game) -> f64 {
        let late = game.turn().saturating_sub(LATE) * LATE_NERVE;
        f64::from(self.style.nerve + late)
    }

    /// The other sides' stacks it sees and their reach: in rounds worked
    /// out once a round (nothing moves while the orders are given); in
    /// turns again whenever the map has changed (its own stacks stand in
    /// their way, or no longer, and its attacks change who is left).
    pub(super) fn spot(&mut self, game: &Game, side: Side) -> Rc<Vec<Foe>> {
        let stamp = if game.rounds() { 0 } else { game.map.revision() };
        let now = (game.turn(), side, stamp);
        match &self.foes {
            Some((of, foes)) if *of == now => foes.clone(),
            _ => {
                let foes = Rc::new(foes(game, side));
                self.foes = Some((now, foes.clone()));
                foes
            }
        }
    }

    /// An order to move (not an attack), made wary: the stack goes along
    /// its way only as far as it may stand: to stand on a hex a stack it
    /// fears could come to only if where it stands now is no better (in
    /// one of its own cities it is better); and in rounds not past a hex
    /// where such a stack could be by the time it gets there (it would run
    /// into it; in turns nobody else moves while it does). `None` if it
    /// had better stay where it is.
    pub(super) fn wary(&self, game: &Game, (side, sight): (Side, &Sight), foes: &[Foe], order: Order) -> Option<Order> {
        let Order::Move { units, to } = order else { return Some(order) };
        let map = &game.map;
        let lead = units.iter().filter_map(|id| map.units().get(*id)).min_by_key(|u| u.movement)?;
        let here = lead.pos();
        let way = game.known_way(lead, to, sight)?;
        let (mine, pace) = (force(game, &units), u64::from(pace(game, &units)));
        let fears = |worth: f64| worth * 100.0 > mine * self.nerve(game);
        let home = map.city_at(here).is_some_and(|c| c.side == side);
        let now = if home { 0.0 } else { menace(foes, here, None) };
        let mut stop = None;
        for (hex, cost) in &way {
            let by = u64::from(*cost) * ROUND / pace.max(1);
            if game.rounds() && fears(menace(foes, *hex, Some(by))) {
                break;
            }
            let standing = menace(foes, *hex, None);
            if (!fears(standing) || standing <= now) && room(game, side, &units, *hex) {
                stop = Some(*hex);
            }
        }
        let stop = stop?;
        if stop == to {
            return Some(Order::Move { units, to });
        }
        let coming: Vec<UnitId> = units
            .into_iter()
            .filter(|id| map.units().get(*id).is_some_and(|u| game.known_range(u, sight).cost(stop).is_some()))
            .collect();
        (!coming.is_empty()).then_some(Order::Move { units: coming, to: stop })
    }
}
