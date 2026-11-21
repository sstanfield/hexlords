//! Patrols and marauders, as rules (`docs/gameplay.md`, "Patrols",
//! "Marauders"). `Order::Patrol` sends a stack round waypoints turn after
//! turn, attacking what it sees at its odds or never; `Order::Maraud` hands
//! a stack to the computer (`crate::marauder`), whose moves for it are
//! `Order::Roam`. A stack on either that fights waits for its player's word
//! (`Unit::fought`): `Order::Done` lets it go on, another order ends its
//! duty, and left alone it goes on the next turn.
//!
//! Not a port: the C++ had no orders.

use std::cmp::Reverse;

use super::{Game, MarchState, StackState, Undo};
use crate::orders::Event;
use crate::state::{Hex, Map, Patrol, Side, Unit, UnitId, hostile};

/// How many legs and waypoints a patrol goes by in a turn, at the most (a
/// guard: its movement runs out long before).
const PATROL_LEGS: usize = 64;

/// True if `unit` patrols or marauds and waits for its player's word: it
/// fought, or its patrol could not go on; and its player has not said Done.
pub(super) fn needs_word(unit: &Unit) -> bool {
    unit.on_duty() && !unit.done && (unit.fought || unit.stalled && unit.patrol.is_some())
}

/// How a stack (its side's `units` on a hex) stands if some of it patrols or
/// marauds and waits for its player's word: it fought, or its patrol is
/// stopped short.
pub(super) fn alarm_state(units: &[&Unit]) -> Option<StackState> {
    if units.iter().any(|u| needs_word(u) && u.fought) {
        Some(StackState::Fought)
    } else if units.iter().any(|u| needs_word(u)) {
        Some(StackState::Stalled)
    } else {
        None
    }
}

/// True if `ids` (a side's units on a hex) all patrol, or all maraud: a
/// click on their hex selects them all.
pub(super) fn together(map: &Map, ids: &[UnitId]) -> bool {
    let units: Vec<&Unit> = ids.iter().filter_map(|id| map.units().get(*id)).collect();
    !units.is_empty()
        && (units.iter().all(|u| u.patrol.is_some() && u.patrol == units[0].patrol)
            || units.iter().all(|u| u.marauding))
}

/// How a stack stands if some of it patrols or marauds.
pub(super) fn duty_state(units: &[&Unit]) -> Option<StackState> {
    if units.iter().any(|u| u.patrol.is_some()) {
        Some(StackState::Patrolling)
    } else if units.iter().any(|u| u.marauding) {
        Some(StackState::Marauding)
    } else {
        None
    }
}

impl Game {
    /// `Order::Patrol`: `units` of `side` patrol `waypoints`, and take this
    /// turn's part of it at once. An undo gives them back what they were
    /// doing before, as for Go to (one leg, no battle).
    pub(super) fn patrol(
        &mut self,
        side: Side,
        units: &[UnitId],
        waypoints: &[Hex],
        attack: Option<u32>,
    ) -> Result<Vec<Event>, String> {
        let before: Vec<(UnitId, MarchState)> = units
            .iter()
            .map(|id| self.own_unit(side, *id).map(|u| (*id, MarchState::of(u))))
            .collect::<Result<_, _>>()?;
        let patrol = Patrol::new(waypoints, attack)?;
        self.check_waypoints(side, units, waypoints)?;
        for id in units {
            if let Some(unit) = self.map.unit_mut(*id) {
                unit.stand_down();
                (unit.route, unit.bound, unit.stalled, unit.defending, unit.done) =
                    (Vec::new(), None, false, false, false);
                unit.patrol = Some(patrol.clone());
            }
        }
        self.undo = None;
        let events = self.patrol_step(side, units);
        let still = events.iter().all(|e| matches!(e, Event::Stalled { .. }));
        match &mut self.undo {
            Some(Undo::Moved { goals, .. }) => *goals = before,
            None if still => self.undo = Some(Undo::Ordered { goals: before }),
            _ => self.undo = None,
        }
        Ok(events)
    }

    /// Why `units` of `side` cannot patrol by way of `waypoints` (in order,
    /// from where they stand), if they cannot: they are not all on one hex,
    /// a leg is one they cannot make over the land the side knows
    /// (`check_route`), or a waypoint is in a city that, as far as the side
    /// knows, is not its own (a patrol goes round, it takes nothing). A
    /// screen asks it as each waypoint is picked; `Order::Patrol` too.
    pub fn check_waypoints(&self, side: Side, units: &[UnitId], waypoints: &[Hex]) -> Result<(), String> {
        let mut at = None;
        for id in units {
            let pos = self.own_unit(side, *id)?.pos();
            if at.is_some_and(|a| a != pos) {
                return Err("the units are not all on one hex".to_string());
            }
            at = Some(pos);
        }
        if at.is_none() {
            return Err("no units".to_string());
        }
        self.check_route(side, units, waypoints)?;
        let sight = self.sight(side);
        for hex in waypoints {
            if self.map.city_at(*hex).is_some_and(|c| sight.holder(c) != side) {
                return Err("a patrol goes into no city but its side's".to_string());
            }
        }
        Ok(())
    }

    /// `Order::Maraud`: the computer leads `units` of `side` (`on`; any other
    /// standing order of theirs ends), or no longer.
    pub(super) fn maraud(&mut self, side: Side, units: &[UnitId], on: bool) -> Result<Vec<Event>, String> {
        for id in units {
            self.own_unit(side, *id)?;
        }
        if units.is_empty() {
            return Err("no units".to_string());
        }
        for id in units {
            if let Some(unit) = self.map.unit_mut(*id) {
                unit.stand_down();
                if on {
                    (unit.route, unit.bound, unit.stalled, unit.defending) = (Vec::new(), None, false, false);
                    unit.marauding = true;
                }
            }
        }
        Ok(Vec::new())
    }

    /// `Order::Roam`: marauding `units` of `side` go to `to`, as a move, and
    /// maraud on; after a battle they wait for their player's word. The
    /// computer's, not the player's to take back.
    pub(super) fn roam(&mut self, side: Side, units: &[UnitId], to: Hex) -> Result<Vec<Event>, String> {
        for id in units {
            if !self.own_unit(side, *id)?.marauding {
                return Err("they are not marauding".to_string());
            }
        }
        let alarmed: Vec<UnitId> =
            units.iter().copied().filter(|id| self.map.units().get(*id).is_some_and(|u| u.fought)).collect();
        let events = self.move_units(side, units, to)?;
        let fought = events.iter().any(|e| matches!(e, Event::Battle { .. }));
        for id in units {
            if let Some(unit) = self.map.unit_mut(*id) {
                unit.marauding = true;
                unit.fought = fought || alarmed.contains(id);
            }
        }
        self.undo = None;
        Ok(events)
    }

    /// `Order::Done` for those of `units` that fought on patrol or
    /// marauding: their player has had a word with them. A patrol goes on at
    /// once, with what movement it has (if it waited for the word since the
    /// turn began); a marauder when the computer next leads it.
    pub(super) fn carry_on(&mut self, side: Side, units: &[UnitId]) -> Vec<Event> {
        let alarmed: Vec<UnitId> =
            units.iter().copied().filter(|id| self.map.units().get(*id).is_some_and(|u| u.fought)).collect();
        for id in &alarmed {
            if let Some(unit) = self.map.unit_mut(*id) {
                unit.fought = false;
            }
        }
        let mut events = Vec::new();
        for ids in self.patrol_stacks(side, &alarmed) {
            events.extend(self.patrol_step(side, &ids));
        }
        if !events.is_empty() {
            self.undo = None;
        }
        events
    }

    /// Those of `side`'s units on `at` that patrol or maraud held in a battle
    /// there: they wait for their player's word.
    pub(super) fn alarm(&mut self, at: Hex) {
        let ids: Vec<UnitId> = self.map.units_at(at).to_vec();
        for id in ids {
            if self.map.units().get(id).is_some_and(|u| u.on_duty()) {
                if let Some(unit) = self.map.unit_mut(id) {
                    unit.fought = true;
                }
            }
        }
    }

    /// `side`'s turn ends: those of its units that waited for its word after
    /// a battle and were left alone go on next turn.
    pub(super) fn stand_easy(&mut self, side: Side) {
        let ids: Vec<UnitId> = self.map.units().of_side(side).filter(|u| u.fought).map(|u| u.id()).collect();
        for id in ids {
            if let Some(unit) = self.map.unit_mut(id) {
                unit.fought = false;
            }
        }
    }

    /// At the start of `side`'s turn, its patrols go their way, a stack (the
    /// units of a hex on the same patrol) at a time; not one that fought
    /// since its last turn and waits for its player's word.
    pub(super) fn patrols(&mut self, side: Side) -> Vec<Event> {
        let ids: Vec<UnitId> = self.map.units().of_side(side).filter(|u| u.patrol.is_some()).map(|u| u.id()).collect();
        let mut events = Vec::new();
        for stack in self.patrol_stacks(side, &ids) {
            let waits = stack.iter().any(|id| self.map.units().get(*id).is_some_and(|u| u.fought));
            if !waits {
                events.extend(self.patrol_step(side, &stack));
            }
        }
        // What went on its own is not the player's to take back.
        self.undo = None;
        events
    }

    /// Those of `ids` (`side`'s) that patrol, by stack: the units of a hex on
    /// the same patrol.
    fn patrol_stacks(&self, side: Side, ids: &[UnitId]) -> Vec<Vec<UnitId>> {
        let map = &self.map;
        let mut stacks: Vec<(Hex, &Patrol, Vec<UnitId>)> = Vec::new();
        for unit in ids.iter().filter_map(|id| map.units().get(*id)) {
            let Some(patrol) = unit.patrol.as_ref().filter(|_| unit.side() == side && map.contains(unit.pos())) else {
                continue;
            };
            match stacks.iter_mut().find(|(at, p, _)| *at == unit.pos() && *p == patrol) {
                Some((_, _, stack)) => stack.push(unit.id()),
                None => stacks.push((unit.pos(), patrol, vec![unit.id()])),
            }
        }
        stacks.into_iter().map(|(_, _, stack)| stack).collect()
    }

    /// A patrolling stack of `side` (`ids`, on one hex) goes its way this
    /// turn: first a battle, if it attacks and sees a stack it can reach and
    /// beat at its odds (once a turn at most: an attack spends its
    /// movement); else toward its next waypoint, as far as the slowest gets
    /// where there is room for all, as a march goes, and on from a waypoint
    /// at once; never into another side's units it sees (one it does not see
    /// is met, in a battle). With no way on it stops short and waits for its
    /// player (it keeps its patrol). More than one leg, or a battle, is not
    /// the player's to take back.
    fn patrol_step(&mut self, side: Side, ids: &[UnitId]) -> Vec<Event> {
        let mut events = Vec::new();
        let mut ids = ids.to_vec();
        let (mut legs, mut battled) = (0, false);
        for _ in 0..PATROL_LEGS {
            let units: Vec<&Unit> = ids.iter().filter_map(|id| self.map.units().get(*id)).collect();
            let Some(first) = units.first() else { break };
            let Some(patrol) = first.patrol.clone() else { break };
            let at = first.pos();
            ids = units.iter().filter(|u| u.pos() == at).map(|u| u.id()).collect();
            let to = patrol.target();
            if at == to {
                // A waypoint: on to the next at once.
                let mut next = patrol;
                next.advance();
                self.keep_patrol(&ids, &next, false);
                continue;
            }
            if let Some(fight) = self.patrol_attack(side, &ids, &patrol) {
                events.extend(fight);
                battled = true;
                break;
            }
            let group = self.group(&ids);
            let Some((lead, table)) = group.as_ref().ok().and_then(|g| g.lead(&self.map)) else {
                events.extend(self.stall(side, ids.clone(), at, to));
                break;
            };
            if lead.movement == 0 {
                break;
            }
            // This turn's step, as a march's.
            let sight = self.sight(side);
            let room = |hex: Hex| self.fits(hex, &ids);
            let step = self
                .known_path_by(lead, table, to, u32::MAX, &sight)
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
                events.extend(self.stall(side, ids.clone(), at, to));
                break;
            };
            // Units met on the way, unseen: a battle there.
            let met = moved.iter().any(|e| matches!(e, Event::Battle { .. }));
            events.extend(moved);
            legs += 1;
            // The move ended their patrol: they keep it.
            self.keep_patrol(&coming, &patrol, met);
            ids = coming;
            if met {
                battled = true;
                break;
            }
            if hex != to {
                // On the way: that is this turn's, but for a battle with
                // what movement is left.
                if let Some(fight) = self.patrol_attack(side, &ids, &patrol) {
                    events.extend(fight);
                    battled = true;
                }
                break;
            }
        }
        if legs > 1 || battled {
            self.undo = None;
        }
        // Nor has the player moved them this turn.
        for id in &ids {
            if let Some(unit) = self.map.unit_mut(*id) {
                unit.moved = false;
            }
        }
        events
    }

    /// The battle a patrolling stack of `side` (`ids`, on one hex) picks, if
    /// its patrol attacks: of the other sides' stacks it sees and can reach
    /// now (not nobody's), the one it is likeliest to beat (the nearest, of
    /// two alike), if the odds are its least or better. Its events, if it
    /// fought; those who live keep their patrol, and wait for their player's
    /// word.
    fn patrol_attack(&mut self, side: Side, ids: &[UnitId], patrol: &Patrol) -> Option<Vec<Event>> {
        let least = patrol.attack?;
        let at = self.map.units().get(*ids.first()?)?.pos();
        let sight = self.sight(side);
        let range = self.group_range(ids, &sight)?;
        let map = &self.map;
        let mut targets: Vec<Hex> = map
            .units()
            .iter()
            .filter(|u| u.side() != side && u.side() != Side::Grey && map.contains(u.pos()))
            .map(|u| u.pos())
            .filter(|h| sight.sees(*h) && range.cost(*h).is_some())
            .collect();
        targets.sort_by_key(|h| (h.row, h.col));
        targets.dedup();
        let (_, target) = targets
            .into_iter()
            .filter_map(|h| self.battle_odds(ids, h).map(|odds| (odds, h)))
            .filter(|(odds, _)| *odds >= least)
            .max_by_key(|(odds, h)| (*odds, Reverse(at.distance(*h)), Reverse((h.row, h.col))))?;
        let group = self.group(ids).ok()?;
        let coming = self.coming(&group, target, &sight);
        if coming.is_empty() {
            return None;
        }
        let events = self.move_units(side, &coming, target).ok()?;
        self.keep_patrol(&coming, patrol, true);
        self.undo = None;
        Some(events)
    }

    /// `ids` patrol `patrol` (and are not stopped short); `fought`: they
    /// fought, and wait for their player's word.
    fn keep_patrol(&mut self, ids: &[UnitId], patrol: &Patrol, fought: bool) {
        for id in ids {
            if let Some(unit) = self.map.unit_mut(*id) {
                (unit.patrol, unit.stalled) = (Some(patrol.clone()), false);
                unit.fought |= fought;
            }
        }
    }
}
