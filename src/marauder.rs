//! Marauders: stacks a side has handed to the computer (`Order::Maraud`).
//! At the start of each of the side's turns, before its player plays (the
//! session runs it for a person, `agents::play_turn` for an agent), the
//! computer leads each of its marauding stacks on its own, in the bold
//! planner's ways (`planner::BOLD`): it attacks the other sides' stacks it
//! sees and can reach at the bold odds or better, and takes the cities it
//! can reach that are empty or held by few enough; else it goes toward the
//! city most worth the trouble that it could take (or one in the fog, to
//! look), or a stack it could beat; else, under total fog, to see more of
//! the map; else toward the nearest city that is not its side's, to wait
//! for a chance near it. Its moves are ordinary orders (`Order::Roam`: a move
//! that keeps them marauding) that the rules carry out or refuse; it is
//! shown its side's view of the game (`Game::view_for`), no more. A stack
//! that fought waits for its player's word (the rules say so,
//! `Unit::fought`): the computer leaves it be.
//!
//! Not a port: the C++ had no players.

use std::cmp::Reverse;

use crate::agents::TurnReport;
use crate::game::Game;
use crate::orders::Order;
use crate::planner::{BOLD, advance, daring};
use crate::state::combat::walls_bonus;
use crate::state::{Hex, MoveRange, Side, Unit, UnitId};

/// How many orders it gives a stack in a turn, at the most.
const STACK_ORDERS: usize = 6;
/// How many of the nearest places at the edge of the known land it tries,
/// to see more (total fog).
const SCOUT_TRIES: usize = 6;

/// The computer leads `side`'s marauding stacks this turn (`side`'s turn),
/// one after the other in reading order, a few orders each. What came of it,
/// and the orders the game refused (something unseen in the way: it leaves
/// that hex be for the rest of the turn).
pub fn maraud(game: &mut Game, side: Side) -> TurnReport {
    let mut report = TurnReport::default();
    let mut led: Vec<UnitId> = Vec::new();
    let mut blocked: Vec<Hex> = Vec::new();
    loop {
        if game.winner().is_some() || game.player() != side {
            break;
        }
        let map = &game.map;
        let ready = |u: &&Unit| u.marauding && !u.fought && map.contains(u.pos()) && !led.contains(&u.id());
        let Some(here) = map.units().of_side(side).filter(ready).map(|u| u.pos()).min_by_key(|h| (h.row, h.col)) else {
            break;
        };
        let ids: Vec<UnitId> =
            map.units().of_side(side).filter(ready).filter(|u| u.pos() == here).map(|u| u.id()).collect();
        led.extend(ids.iter().copied());
        lead_stack(game, side, ids, (&mut blocked, &mut report));
    }
    report
}

/// The computer leads one marauding stack of the side whose turn it is
/// (`ids`, on one hex) now, with what movement it has: when its player hands
/// it over in the middle of a turn, or lets it go on after a battle.
pub fn lead(game: &mut Game, ids: &[UnitId]) -> TurnReport {
    let mut report = TurnReport::default();
    let side = game.player();
    let ready: Vec<UnitId> = ids
        .iter()
        .copied()
        .filter(|id| game.map.units().get(*id).is_some_and(|u| u.side() == side && u.marauding && !u.fought))
        .collect();
    if game.winner().is_none() && !ready.is_empty() {
        lead_stack(game, side, ready, (&mut Vec::new(), &mut report));
    }
    report
}

/// A few orders for the stack `ids` of `side`, while it goes on.
fn lead_stack(game: &mut Game, side: Side, mut ids: Vec<UnitId>, (blocked, report): (&mut Vec<Hex>, &mut TurnReport)) {
    for _ in 0..STACK_ORDERS {
        let view = game.view_for(side);
        let Some(order) = decide(&view, side, &ids, blocked) else { break };
        let Order::Roam { units, to } = order.clone() else { break };
        match game.apply(order.clone()) {
            Ok(events) => report.events.extend(events),
            Err(why) => {
                blocked.push(to);
                report.refused.push((order, why));
                continue;
            }
        }
        // On, with those that went, while they may.
        let map = &game.map;
        ids = units
            .into_iter()
            .filter(|id| map.units().get(*id).is_some_and(|u| u.marauding && !u.fought && u.pos() == to))
            .collect();
        if ids.is_empty() {
            break;
        }
    }
}

/// The next order for the marauding stack `ids` of `side` (on one hex), as
/// the side knows the game (`game`: its view): a strike, or a step toward
/// what it goes for; `None` when it has done what it can this turn. The
/// stack goes as one, where all of it can get: once one of it has no moves
/// left, it has done. (`blocked`: hexes the game refused to send it to this
/// turn.)
pub fn decide(game: &Game, side: Side, ids: &[UnitId], blocked: &[Hex]) -> Option<Order> {
    let map = &game.map;
    let here = map.units().get(*ids.first()?)?.pos();
    let units: Vec<&Unit> = ids.iter().filter_map(|id| map.units().get(*id)).filter(|u| u.pos() == here).collect();
    if units.iter().any(|u| u.movement == 0) {
        return None;
    }
    let lead = units.iter().copied().min_by_key(|u| u.movement)?;
    let ids: Vec<UnitId> = units.iter().map(|u| u.id()).collect();
    let sight = game.sight(side);
    let dare = daring(&BOLD, game).0;
    let ranges: Vec<MoveRange> = units.iter().map(|u| game.known_range(u, &sight)).collect();
    let reach = |hex: Hex| ranges.iter().all(|r| r.cost(hex).is_some());
    let odds = |hex: Hex| {
        if map.units_at(hex).is_empty() { Some(100) } else { game.battle_odds(&ids, hex) }
    };
    let roam = |to: Hex| Order::Roam { units: ids.clone(), to };
    let free = |hex: &Hex| !blocked.contains(hex);

    // A strike this turn: the other sides' stacks it sees (nobody's, only in
    // their cities) and the cities not its side's, empty or not, at the bold
    // odds or better; a city first, then the best odds, then the nearest.
    let theirs = map.units().iter().filter(|u| u.side() != side && u.side() != Side::Grey).map(|u| u.pos());
    let cities = map.cities().iter().filter(|c| c.side != side).map(|c| c.pos);
    let mut strikes: Vec<(bool, u32, Hex)> = theirs
        .chain(cities)
        .filter(|h| map.contains(*h) && sight.sees(*h) && free(h) && reach(*h))
        .filter_map(|h| Some((map.city_at(h).is_some(), odds(h)?, h)))
        .filter(|(_, odds, _)| *odds >= dare)
        .collect();
    strikes.sort_by_key(|(city, odds, h)| (Reverse(*city), Reverse(*odds), here.distance(*h), h.row, h.col));
    if let Some((_, _, to)) = strikes.first() {
        return Some(roam(*to));
    }

    // Else on toward what it could take: the city most worth the trouble
    // (its taxes, more for another side's than for nobody's, against the way
    // there and what holds it; one in the fog it goes to look at), or a
    // stack of the other sides' it sees and could beat.
    let mut goals: Vec<(i64, Hex)> = Vec::new();
    for city in map.cities().iter().filter(|c| c.side != side && sight.knows(c.pos) && free(&c.pos)) {
        let held = if sight.sees(city.pos) {
            if odds(city.pos).is_none_or(|o| o < dare) {
                continue;
            }
            map.units_at(city.pos).len()
        } else {
            sight.ghost_at(city.pos).map_or(2, |g| g.count)
        };
        let value = 20 + i64::from(city.income) * 3 + if city.side == Side::Grey { 0 } else { 10 };
        let trouble = i64::from(here.distance(city.pos)) + 3 * held as i64 + i64::from(walls_bonus(city.walls))
            - 2 * ids.len() as i64;
        goals.push((value * 100 / (10 + trouble.max(0)), city.pos));
    }
    for at in map.units().iter().filter(|u| u.side() != side && u.side() != Side::Grey).map(|u| u.pos()) {
        if map.city_at(at).is_none() && sight.sees(at) && free(&at) && odds(at).is_some_and(|o| o >= dare) {
            goals.push((2000 / (10 + i64::from(here.distance(at))), at));
        }
    }
    goals.sort_by_key(|(score, h)| (Reverse(*score), h.row, h.col));
    goals.dedup_by_key(|(_, h)| *h);
    // (A step is one all of it can get to: the stack stays whole.)
    let shut = |h: Hex| blocked.contains(&h) || !reach(h);
    let toward = |to: Hex| match advance(game, (side, &sight), (&ids, lead), to, &shut)? {
        Order::Move { units, to } => Some(Order::Roam { units, to }),
        _ => None,
    };
    if let Some(order) = goals.iter().find_map(|(_, to)| toward(*to)) {
        return Some(order);
    }
    // Else, to see more (total fog): the nearest of the edge of the land its
    // side knows.
    let edge = game.frontier_for(lead, &sight);
    if let Some(order) =
        edge.into_iter().filter(|(h, _)| *h != here && free(h)).take(SCOUT_TRIES).find_map(|(h, _)| toward(h))
    {
        return Some(order);
    }
    // Else near the nearest city that is not its side's, to wait for a
    // chance there (never into it, at odds below its own).
    let mut near: Vec<Hex> =
        map.cities().iter().filter(|c| c.side != side && sight.knows(c.pos) && free(&c.pos)).map(|c| c.pos).collect();
    near.sort_by_key(|h| (here.distance(*h), h.row, h.col));
    near.into_iter().filter(|h| !reach(*h)).find_map(toward)
}
