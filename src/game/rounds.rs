//! Rounds, the second way to play (`docs/gameplay.md`, "Rounds"; the
//! owner's word, 2026-11-22: "implement the round (simultaneous turns)
//! feature"): every side gives all its orders for the round, without seeing
//! what the others' do; then the round plays out at once, for everyone.
//!
//! The rules are the turns' own, applied at another moment:
//!
//! - **Planning.** The sides give their orders one after the other (who
//!   plans first learns nothing by it: nothing moves). What moves no unit is
//!   carried out at once (a city's work, its walls, a rally point, Defend,
//!   Done, a blessing sought, a unit sent home). An order to move is a
//!   **plan**: `Order::Move`, `Toward` and `Roam` give the units a route to
//!   that hex for this round (`Unit::planned`); `GoTo` and `Patrol` are
//!   standing orders as ever, and take their first step with the round.
//!   With total fog of war, `Toward` may name land the side has never
//!   seen: the units set off toward it over the land they have seen, and
//!   find their way as they come to see more (`way_toward`).
//!   `Order::EndTurn` says the side has done; `Order::Undo` takes a plan
//!   back.
//! - **The round plays out** (`Game::play_out`) once every side has done, a
//!   **tick** at a time: in each tick, every stack with somewhere to go
//!   takes one step, a hex, if it can pay for it; the sides step in the
//!   round's order (drawn anew each round, if the game is set up so), so
//!   who steps first in a tick is the one thing chance decides. A stack
//!   that steps into a hex another side stands on attacks it: so two
//!   stacks making for the same hex fight there, the one that got there
//!   first defending it; two that would swap hexes fight on the hex of the
//!   one that steps second; and a stack sent against one that has left
//!   finds the hex empty, and takes it. Only a plan attacks a stack it sees
//!   (where it ends); a march or a patrol on its way stops short, as in
//!   turns (a patrol attacks at its odds). Stacks of a side beside the hex
//!   they are all sent against attack it together. A battle ends a stack's
//!   round (it has no movement left).
//! - **The round ends** when a tick goes by with nobody moving: the plans
//!   end, got there or not; the standing orders use what movement is left
//!   as they do at a turn's end (and say if they are stopped short); wounds
//!   heal; then the next round begins for every side together: its units'
//!   movement, its taxes, its upkeep, what its cities built.
//!
//! Without a screen the last side's `EndTurn` plays the whole round out;
//! while one watches (`Game::watch`) it only begins it, and the screen
//! plays the ticks (`play_out`), showing each as it happens.
//!
//! Not a port: the C++ had no turns.

use super::{Game, MarchState, Sight, UNSEEN, Undo};
use crate::orders::Event;
use crate::setup::{Mode, TurnOrder};
use crate::state::{Hex, MoveCostId, Side, TileType, Unit, UnitId, hostile};

/// A round plays out in at most this many ticks (a guard: movement runs out
/// long before).
const MAX_TICKS: u32 = 400;

/// How far a round that plays out has got: the ticks played.
#[derive(Clone, Debug, Default)]
pub(super) struct PlayOut {
    tick: u32,
}

impl Game {
    /// True if the game is played in rounds: all sides give their orders,
    /// then the round plays out at once.
    pub fn rounds(&self) -> bool {
        self.setup.mode == Mode::Rounds
    }

    /// True while a round plays out (rounds): nobody gives orders, and
    /// `play_out` goes on with it.
    pub fn playing_out(&self) -> bool {
        self.playing.is_some()
    }

    /// In rounds, `Order::Move` (`reach`: every one of them can get there
    /// this turn, as things stand), `Order::Toward` (as far as they get) and
    /// `Order::Roam` (`roam`: the computer's move for marauders, who maraud
    /// on): `ids` of `side` plan to go to `to` this round. Nothing moves
    /// yet. An undo gives them back what they were doing before.
    pub(super) fn plan(
        &mut self,
        side: Side,
        ids: &[UnitId],
        to: Hex,
        reach: bool,
        roam: bool,
    ) -> Result<Vec<Event>, String> {
        let mut ids = ids.to_vec();
        ids.dedup();
        if ids.is_empty() {
            return Err("no units".to_string());
        }
        let before: Vec<(UnitId, MarchState)> = ids
            .iter()
            .map(|id| self.own_unit(side, *id).map(|u| (*id, MarchState::of(u))))
            .collect::<Result<_, _>>()?;
        if roam && before.iter().any(|(_, was)| !was.marauding) {
            return Err("they are not marauding".to_string());
        }
        // (Toward land the side has never seen, they may set off: they find
        // their way as they come to see it, `way_toward`.)
        let sight = self.sight(side);
        if !sight.knows(to) && reach {
            return Err(UNSEEN.to_string());
        }
        // As a move is refused: who cannot go together, cannot get there, or
        // would leave others in the water.
        let group = self.group(&ids)?;
        for (id, table) in &group.movers {
            let unit = self.own_unit(side, *id)?;
            if reach && self.known_range_by(unit, *table, &sight).cost(to).is_none() {
                return Err(format!("{} cannot get there this turn", unit.name));
            }
            if !reach && unit.pos() != to && self.way_toward(unit, *table, to, &sight).is_none() {
                return Err("there is no way there".to_string());
            }
        }
        let mut origins: Vec<Hex> =
            before.iter().filter_map(|(id, _)| self.map.units().get(*id)).map(|u| u.pos()).collect();
        origins.dedup();
        for from in origins {
            if !self.afloat(from, &self.there_after(side, from, &ids, &[])) {
                return Err("that would leave units in the water with no ship to carry them".to_string());
            }
        }
        for id in &ids {
            if let Some(unit) = self.map.unit_mut(*id) {
                unit.stand_down();
                (unit.route, unit.bound, unit.stalled, unit.defending) = (vec![to], None, false, false);
                (unit.planned, unit.moved, unit.marauding) = (true, true, roam);
            }
        }
        // (The computer's plans for marauders are not their player's to take
        // back.)
        self.undo = (!roam).then_some(Undo::Ordered { goals: before });
        Ok(Vec::new())
    }

    /// In rounds, `Order::EndTurn`: the side whose turn it is has given its
    /// orders. The next side still in gives its own; after the last, the
    /// round plays out: all of it now, or, while a screen watches
    /// (`Game::watch`), as the screen plays it (`play_out`).
    pub(super) fn end_planning(&mut self) -> Vec<Event> {
        self.deselect();
        self.undo = None;
        if self.order.is_empty() {
            self.round += 1;
            return Vec::new();
        }
        let next = (self.current + 1..self.order.len()).find(|i| !self.out.contains(&self.order[*i]));
        if let Some(next) = next {
            self.current = next;
            let (round, side) = (self.round, self.player());
            let events = vec![Event::Began { side, round }];
            self.remember(round, side, &events);
            return events;
        }
        // The stacks left alone after a battle on patrol or marauding go on;
        // what each side's units saw on their way last round is seen no
        // longer (what they see on this round's way is, until the next
        // plays out).
        for side in self.order.clone() {
            self.stand_easy(side);
        }
        self.note_sightings();
        for trail in &mut self.trail {
            trail.clear();
        }
        self.playing = Some(PlayOut::default());
        if self.watched.is_some() {
            return Vec::new();
        }
        let mut events = Vec::new();
        while self.playing.is_some() {
            events.extend(self.play_out());
        }
        events
    }

    /// The way of `unit` (at the prices of `table`) toward `to`, as its
    /// side knows the map: each hex and what it costs to get there. To land
    /// the side has never seen (total fog of war: shut until it is seen)
    /// there is no way yet: the way is to the hex it can get to that is
    /// nearest to it, if that is nearer than where it stands (never one
    /// another side's units stand on); as it goes it sees more, and the next
    /// tick finds the way on. So a stack can be sent to look into the dark,
    /// a step at a time over land it has seen.
    fn way_toward(&self, unit: &Unit, table: MoveCostId, to: Hex, sight: &Sight) -> Option<Vec<(Hex, u32)>> {
        if sight.knows(to) {
            return self.known_path_by(unit, table, to, u32::MAX, sight);
        }
        let mut far = unit.clone();
        far.movement = u32::MAX / 2;
        let range = self.known_range_by(&far, table, sight);
        let (side, here) = (unit.side(), unit.pos());
        let (nearest, _) = range
            .reachable()
            .filter(|(hex, _)| !hostile(&self.map, side, *hex))
            .min_by_key(|(hex, cost)| (hex.distance(to), *cost, hex.row, hex.col))?;
        if nearest.distance(to) >= here.distance(to) {
            return None;
        }
        self.known_path_by(unit, table, nearest, u32::MAX, sight)
    }

    /// The round that plays out goes on (`playing_out`): a tick, in which
    /// every stack with somewhere to go takes a step, the sides in the
    /// round's order; and, once a tick goes by with nobody moving, the
    /// round's end and the next one's beginning (`Event::Began` for the
    /// side that gives its orders first). What happened, in its order.
    pub fn play_out(&mut self) -> Vec<Event> {
        let Some(play) = self.playing.clone() else { return Vec::new() };
        let (mut events, went) = self.tick();
        if went && play.tick + 1 < MAX_TICKS && self.winner.is_none() {
            self.playing = Some(PlayOut { tick: play.tick + 1 });
            return events;
        }
        events.extend(self.close_round());
        events
    }

    /// One tick of the round: each side in the round's order, each of its
    /// stacks a step. What happened, and whether anybody moved or fought.
    fn tick(&mut self) -> (Vec<Event>, bool) {
        let round = self.round;
        let (mut all, mut went) = (Vec::new(), false);
        // Who has stepped this tick (in another stack's attack, it may be).
        let mut stepped: Vec<UnitId> = Vec::new();
        for side in self.order.clone() {
            if self.out.contains(&side) {
                continue;
            }
            for (patrols, stack) in self.stacks_going(side) {
                if self.winner.is_some() {
                    break;
                }
                let units = self.map.units();
                let stack: Vec<UnitId> =
                    stack.into_iter().filter(|id| !stepped.contains(id) && units.get(*id).is_some()).collect();
                if stack.is_empty() {
                    continue;
                }
                self.fought.clear();
                let events = if patrols {
                    self.patrol_tick(side, &stack, &mut stepped)
                } else {
                    self.march_tick(side, &stack, &mut stepped)
                };
                went |= events.iter().any(|e| matches!(e, Event::Moved { .. } | Event::Battle { .. }));
                self.remember(round, side, &events);
                all.extend(events);
            }
        }
        self.note_sightings();
        (all, went)
    }

    /// `side`'s stacks with somewhere to go: the units of a hex with the
    /// same route (a plan, a march), then those on the same patrol (true),
    /// not one that fought and waits for its player's word.
    fn stacks_going(&self, side: Side) -> Vec<(bool, Vec<UnitId>)> {
        let map = &self.map;
        let mut groups: Vec<(Hex, &[Hex], bool, Vec<UnitId>)> = Vec::new();
        for unit in map.units().of_side(side).filter(|u| map.contains(u.pos()) && !u.route.is_empty()) {
            let same = |(at, route, planned, _): &&mut (Hex, &[Hex], bool, Vec<UnitId>)| {
                *at == unit.pos() && *route == unit.route.as_slice() && *planned == unit.planned
            };
            match groups.iter_mut().find(same) {
                Some((_, _, _, ids)) => ids.push(unit.id()),
                None => groups.push((unit.pos(), &unit.route, unit.planned, vec![unit.id()])),
            }
        }
        let mut stacks: Vec<(bool, Vec<UnitId>)> = groups.into_iter().map(|(_, _, _, ids)| (false, ids)).collect();
        let patrolling: Vec<UnitId> =
            map.units().of_side(side).filter(|u| u.patrol.is_some() && u.route.is_empty()).map(|u| u.id()).collect();
        for stack in self.patrol_stacks(side, &patrolling) {
            let waits = stack.iter().any(|id| map.units().get(*id).is_some_and(|u| u.fought));
            if !waits {
                stacks.push((true, stack));
            }
        }
        stacks
    }

    /// A stack of `side` with a route (`ids`: those of them on the first
    /// one's hex) takes this tick's step along it: a hex, on the way its
    /// side knows, if the slowest of them can pay for it and there is room
    /// for all of them. At a stop it goes on to the next at once (as a
    /// march does: a blessing sought at a temple stops it for the round).
    /// Into another side's units it sees, only a plan goes, where it ends:
    /// an attack, with the side's other stacks beside that hex that are
    /// sent against it too (`company`); one that is no plan waits, and one
    /// it does not see is met, in a battle. A battle ends a plan.
    fn march_tick(&mut self, side: Side, ids: &[UnitId], stepped: &mut Vec<UnitId>) -> Vec<Event> {
        let mut events = Vec::new();
        let mut ids = ids.to_vec();
        loop {
            let units: Vec<&Unit> = ids.iter().filter_map(|id| self.map.units().get(*id)).collect();
            let Some(first) = units.first() else { break };
            let (at, mut route, bound, planned) = (first.pos(), first.route.clone(), first.bound, first.planned);
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
                            (unit.bound, unit.planned) = (None, false);
                            unit.defending |= defend && bound.is_some();
                        }
                    }
                }
                if !planned && self.map.tile(at).is_some_and(|t| t.kind == TileType::Temple) {
                    let blessed = self.blessings(at, &ids);
                    if !blessed.is_empty() {
                        events.extend(blessed);
                        break;
                    }
                }
                continue;
            }
            let Ok(group) = self.group(&ids) else { break };
            let Some((lead, table)) = group.lead(&self.map) else { break };
            if lead.movement == 0 {
                break;
            }
            let sight = self.sight(side);
            let way = self.way_toward(lead, table, to, &sight);
            let Some((hex, cost)) = way.and_then(|way| way.first().copied()) else { break };
            if cost > lead.movement {
                break;
            }
            let foe = sight.sees(hex) && hostile(&self.map, side, hex);
            if foe && !(planned && hex == to) {
                break;
            }
            if !foe && !hostile(&self.map, side, hex) && !self.fits(hex, &ids) {
                break;
            }
            let mut coming = self.coming(&group, hex, &sight);
            if coming.is_empty() {
                break;
            }
            if foe {
                coming = self.company(side, hex, coming, stepped, &sight);
            }
            let was: Vec<(UnitId, MarchState)> =
                coming.iter().filter_map(|id| self.map.units().get(*id)).map(|u| (u.id(), MarchState::of(u))).collect();
            let Ok(moved) = self.move_units(side, &coming, hex) else { break };
            let battled = moved.iter().any(|e| matches!(e, Event::Battle { .. }));
            events.extend(moved);
            // The move ended their orders: they keep them (a plan, until it
            // has fought; marauders maraud on, and wait for their player's
            // word after a battle).
            for (id, was) in was {
                let Some(unit) = self.map.unit_mut(id) else { continue };
                if !(was.planned && battled) {
                    (unit.route, unit.bound, unit.planned) = (was.route, was.bound, was.planned);
                }
                if was.marauding {
                    (unit.marauding, unit.fought) = (true, battled || was.fought);
                }
            }
            stepped.extend(coming);
            break;
        }
        events
    }

    /// The hexes `side`'s stacks with somewhere to go (a plan, a march) pass
    /// as the round plays out, as things stand and as far as their movement
    /// takes them, each stack's in its order; and their next stops, however
    /// far. For a screen to show a side its plans.
    pub fn ways_this_round(&self, side: Side) -> Vec<Hex> {
        let sight = self.sight(side);
        let mut hexes: Vec<Hex> = Vec::new();
        for (patrols, ids) in self.stacks_going(side) {
            let Some(first) = ids.first().and_then(|id| self.map.units().get(*id)) else { continue };
            let to = if patrols { first.patrol.as_ref().map(|p| p.target()) } else { first.goto() };
            let Some(to) = to else { continue };
            let Ok(group) = self.group(&ids) else { continue };
            let Some((lead, table)) = group.lead(&self.map) else { continue };
            let way = self.way_toward(lead, table, to, &sight).unwrap_or_default();
            let near = way.into_iter().take_while(|(_, cost)| *cost <= lead.movement).map(|(hex, _)| hex);
            for hex in near.chain([to]) {
                if !hexes.contains(&hex) {
                    hexes.push(hex);
                }
            }
        }
        hexes
    }

    /// Who attacks `target` with `ids` (a stack of `side`'s that steps into
    /// it now): the side's other stacks beside it whose plan ends there,
    /// that have not stepped this tick and can attack it now, a stack at a
    /// time for as long as all of them make one attack (`Game::fits`).
    fn company(&self, side: Side, target: Hex, ids: Vec<UnitId>, stepped: &[UnitId], sight: &Sight) -> Vec<UnitId> {
        let map = &self.map;
        let mut stacks: Vec<(Hex, Vec<UnitId>)> = Vec::new();
        let beside = |u: &&Unit| {
            map.contains(u.pos())
                && u.pos().distance(target) == 1
                && u.planned
                && u.route.as_slice() == [target]
                && !ids.contains(&u.id())
                && !stepped.contains(&u.id())
        };
        for unit in map.units().of_side(side).filter(beside) {
            match stacks.iter_mut().find(|(at, _)| *at == unit.pos()) {
                Some((_, stack)) => stack.push(unit.id()),
                None => stacks.push((unit.pos(), vec![unit.id()])),
            }
        }
        let mut all = ids;
        for (_, stack) in stacks {
            let Ok(group) = self.group(&stack) else { continue };
            let coming = self.coming(&group, target, sight);
            let with: Vec<UnitId> = all.iter().chain(&coming).copied().collect();
            if !coming.is_empty() && self.group(&with).is_ok() && self.fits(Hex::new(u16::MAX, u16::MAX), &with) {
                all = with;
            }
        }
        all
    }

    /// The round has played out: its plans end; the standing orders use
    /// what movement is left, as at a turn's end (a march or a patrol that
    /// is stopped short says so); wounds heal; and, unless the game is over,
    /// the next round begins for every side: its units' movement, its
    /// taxes, its upkeep, what its cities built; the first side still in
    /// gives its orders.
    fn close_round(&mut self) -> Vec<Event> {
        let round = self.round;
        let mut all = Vec::new();
        let planned: Vec<UnitId> = self.map.units().iter().filter(|u| u.planned).map(|u| u.id()).collect();
        for id in planned {
            if let Some(unit) = self.map.unit_mut(id) {
                unit.route.clear();
                unit.planned = false;
            }
        }
        for side in self.order.clone() {
            if self.out.contains(&side) || self.winner.is_some() {
                continue;
            }
            self.fought.clear();
            let mut events = self.marches(side);
            events.extend(self.patrols(side));
            self.remember(round, side, &events);
            all.extend(events);
        }
        self.heal(|_| true);
        self.note_sightings();
        self.playing = None;
        self.undo = None;
        if self.winner.is_some() {
            return all;
        }
        self.round += 1;
        if self.setup.order == TurnOrder::Drawn {
            self.rng.shuffle(&mut self.order);
        }
        let round = self.round;
        self.raised.clear();
        self.fights.retain(|(_, fought, _)| fought + super::REPLAY_ROUNDS > round);
        for side in self.order.clone() {
            if self.out.contains(&side) {
                continue;
            }
            self.map.reset_move_points(side);
            let events = self.pay_day(side);
            self.remember(round, side, &events);
            all.extend(events);
        }
        self.current = (0..self.order.len()).find(|i| !self.out.contains(&self.order[*i])).unwrap_or(0);
        let began = vec![Event::Began { side: self.player(), round }];
        self.remember(round, self.player(), &began);
        all.extend(began);
        all
    }
}
