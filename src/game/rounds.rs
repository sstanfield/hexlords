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
//!   that hex for this round (`Unit::planned`) and the way they will take
//!   (`Unit::way`: the way the side knows when it gives the order, round
//!   the stacks it sees); `GoTo` and `Patrol` are standing orders as ever,
//!   and take their first step with the round. With total fog of war,
//!   `Toward` may name land the side has never seen: the units set off
//!   toward it over the land they have seen, and find their way as they
//!   come to see more (`way_toward`). Units sent to a hex where others of
//!   their side stand can be ordered on with them (`Order::Move` for all of
//!   them): they **join** there, those that stand there wait for them, and
//!   they go on as one stack (`gathering`). `Order::EndTurn` says the side
//!   has done; `Order::Undo` takes a plan back.
//! - **The round plays out** (`Game::play_out`) once every side has done,
//!   on a clock (the owner's word, 2026-11-22: "fast units should get to
//!   spend the same MP as slower units faster than them ... combat should
//!   probably happen on any given subtick"). A step takes a unit the share
//!   of a round that its cost is of the unit's movement a turn
//!   (`Unit::speed`; [`ROUND`] is a round): riders with 20 points are
//!   three hexes of grass on when soldiers with 12 are two. A stack goes
//!   at the pace of its slowest. Each stack sets off on the next hex of its
//!   way, and is there when the step's time is up; the stacks get there in
//!   the order of the clock (at the same moment: in the round's order of
//!   the sides, drawn anew as each round begins to play out, if the game
//!   is set up so). A stack that gets to a hex another side stands on
//!   attacks it, there and then, wherever its way was taking it ("stacks
//!   that run into each other should probably fight vs just block each
//!   other"): it does not go round who has come to stand on its way. So
//!   the faster of two stacks sent to a city is in it first, and the other
//!   attacks it there; two that would swap hexes fight on the hex of the
//!   one that is slower to leave; a stack sent against one that has left
//!   finds the hex empty, and takes it. Each stack attacks when it gets
//!   there: to attack together, stacks join first. A stack waits behind
//!   others of its side that are on their way and leave it no room, and
//!   goes through those that stay; two of a side that come toward each
//!   other and have no room on one hex pass each other: they change places
//!   at once (`passes`). A battle ends the attackers' round (they
//!   have no movement left); a patrol goes round and attacks at its odds,
//!   as ever.
//! - **The round ends** when nobody is on a step any more: the plans end,
//!   got there or not; the standing orders use what movement is left as
//!   they do at a turn's end (and say if they are stopped short); wounds
//!   heal; then the next round begins for every side together: its units'
//!   movement, its taxes, its upkeep, what its cities built.
//!
//! Without a screen the last side's `EndTurn` plays the whole round out;
//! while one watches (`Game::watch`) it only begins it, and the screen
//! plays it on a beat at a time ([`BEAT`]; a battle ends a beat), showing
//! each as it happens.
//!
//! Not a port: the C++ had no turns.

use super::{Game, Group, MarchState, SEE_ALL, Sight, UNSEEN, Undo};
use crate::orders::Event;
use crate::setup::{Mode, TurnOrder};
use crate::state::{Hex, MAX_STACK, MoveCostId, Passage, Side, TileType, Unit, UnitId, hostile, path_with, reach_with};

/// A round on the clock a round plays out by: the time a unit takes to
/// spend a turn's movement (`Unit::speed`). A step that costs a unit `c` of
/// its `m` points a turn takes it `c * ROUND / m`. (Every whole number up to
/// 16 divides it, and 18 to 30 in twos.) A stack that waited, or kept
/// points from the turn before, goes on past it: the clock says who is
/// first, not how far anybody gets.
pub const ROUND: u64 = 720_720;

/// What `Game::play_out` plays at a time, for a screen to show: a sixth of
/// a round, or up to a battle.
pub const BEAT: u64 = ROUND / 6;

/// How long a patrol with no step to take rests before it looks again.
const REST: u64 = ROUND / 12;

/// A round plays out in at most this many steps (a guard: movement runs out
/// long before).
const MAX_STEPS: u32 = 200_000;

/// How far a round that plays out has got: the clock; the steps taken; and
/// whether the stacks that wait for others of their side wait no longer
/// (nobody was on a step, and some waited).
#[derive(Clone, Debug, Default)]
pub(super) struct PlayOut {
    now: u64,
    steps: u32,
    hasty: bool,
}

impl PlayOut {
    /// For `Game::digest`.
    pub(super) fn digest(&self, sum: &mut super::Digest) {
        sum.n(1 + self.now);
        sum.n(u64::from(self.steps));
        sum.flags(&[self.hasty]);
    }
}

/// A plan's legs: for each unit, its stops and its way, hex by hex.
type Legs = Vec<(UnitId, Vec<Hex>, Vec<Hex>)>;

/// What a stack with somewhere to go does next (`Game::look`).
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Look {
    /// It sets off for `hex` (the `passed`th of its way from here: past
    /// hexes its own side fills), those of it that can (`coming`), and is
    /// there `time` later.
    Go { hex: Hex, passed: usize, coming: Vec<UnitId>, time: u64 },
    /// It waits: for those that come to join it, or behind others of its
    /// side on their way. It looks again when something has happened.
    Wait,
    /// A patrol with no step to take now: it looks again in a while.
    Rest,
    /// It takes no more steps this round.
    Stop,
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

    /// The clock of the round that plays out ([`ROUND`] is a round), if one
    /// does.
    pub fn round_clock(&self) -> Option<u64> {
        self.playing.as_ref().map(|play| play.now)
    }

    /// In rounds, `Order::Move` (`reach`: every one of them can get there
    /// this turn, as things stand), `Order::Toward` (as far as they get) and
    /// `Order::Roam` (`roam`: the computer's move for marauders, who maraud
    /// on): `ids` of `side` plan to go to `to` this round. Nothing moves
    /// yet. An undo gives them back what they were doing before.
    ///
    /// Units on one hex go from there. Units on several: if those that are
    /// not on one of the hexes are all sent there (a plan that ends on it),
    /// they join the ones that stand there and go on with them, who wait
    /// for them (`gathering`); else they gather first on the one of their
    /// hexes nearest to `to` that all of them can get on from (`muster`:
    /// one order is one stack, and into another side's units one attack,
    /// as in turns); if there is none, the units of each hex go on their
    /// own.
    pub(super) fn plan(
        &mut self,
        side: Side,
        ids: &[UnitId],
        to: Hex,
        reach: bool,
        roam: bool,
    ) -> Result<Vec<Event>, String> {
        let ids = super::distinct(ids);
        if ids.is_empty() {
            return Err("no units".to_string());
        }
        let before: Vec<(UnitId, MarchState)> = ids
            .iter()
            .map(|id| self.own_unit(side, *id).map(|u| (*id, MarchState::of(u))))
            .collect::<Result<_, _>>()?;
        if !self.map.contains(to) {
            return Err(format!("{to} is not on the map"));
        }
        if roam && before.iter().any(|(_, was)| !was.marauding) {
            return Err("they are not marauding".to_string());
        }
        // (Toward land the side has never seen, they may set off: they find
        // their way as they come to see it, `way_toward`.)
        let sight = self.sight(side);
        if !sight.knows(to) && reach {
            return Err(UNSEEN.to_string());
        }
        // The stacks they go as: one, from the hex they are on or gather
        // on; or one from each of their hexes. All of them, or none: each
        // stack's legs first.
        let stacks = self.by_hex(&ids);
        let together = if reach && !roam { self.gathering(&ids) } else { None };
        let mut plans = Vec::new();
        match together {
            Some(from) => plans.push(self.legs(side, &ids, (from, to), reach, &sight, (None, false))?),
            None if stacks.len() == 1 => {
                plans.push(self.legs(side, &ids, (stacks[0].0, to), reach, &sight, (None, false))?)
            }
            None => match self.muster(side, &ids, to, &sight).filter(|_| reach && !roam) {
                Some(legs) => plans.push(legs),
                None => {
                    for (from, stack) in &stacks {
                        plans.push(self.legs(side, stack, (*from, to), reach, &sight, (None, false))?);
                    }
                }
            },
        }
        for legs in plans {
            // (The units of one order are a party: they go as a stack, and
            // those of another order on their hex do not hold them up.)
            self.parties += 1;
            let party = self.parties;
            for (id, route, way) in legs {
                if let Some(unit) = self.map.unit_mut(id) {
                    unit.stand_down();
                    (unit.route, unit.bound, unit.stalled, unit.defending) = (route, None, false, false);
                    (unit.planned, unit.party, unit.moved, unit.marauding) = (true, party, true, roam);
                    unit.way = way;
                }
            }
        }
        // (The computer's plans for marauders are not their player's to take
        // back.)
        self.undo = (!roam).then_some(Undo::Ordered { goals: before });
        Ok(Vec::new())
    }

    /// In rounds, `Order::Onward`: `ids` of `side`, who have one plan for
    /// this round (`plan_end`), go on from where it ends to `to`, which all
    /// of them can reach from there with what the way so far leaves each.
    /// Nothing moves yet. An undo takes this leg back: the plan is as it
    /// was.
    pub(super) fn plan_onward(&mut self, side: Side, ids: &[UnitId], to: Hex) -> Result<Vec<Event>, String> {
        let ids = super::distinct(ids);
        if ids.is_empty() {
            return Err("no units".to_string());
        }
        let before: Vec<(UnitId, MarchState)> = ids
            .iter()
            .map(|id| self.own_unit(side, *id).map(|u| (*id, MarchState::of(u))))
            .collect::<Result<_, _>>()?;
        let sight = self.sight(side);
        if !sight.knows(to) {
            return Err(UNSEEN.to_string());
        }
        let from = self.plan_end(&ids).ok_or("they have no plan together to go on from")?;
        if from == to {
            return Err("that is where their plan ends".to_string());
        }
        if !self.goes_on_from(side, from, &sight) {
            return Err("their plan ends in a battle, or in a city to take: nothing comes after it".to_string());
        }
        if self.stopped_at(&ids, from) {
            return Err(
                "their plan ends on a temple that has yet to bless them: it stops them for the round".to_string()
            );
        }
        let legs = self.legs(side, &ids, (from, to), true, &sight, (None, true))?;
        self.parties += 1;
        let party = self.parties;
        for (id, route, way) in legs {
            if let Some(unit) = self.map.unit_mut(id) {
                (unit.route, unit.way, unit.party) = (route, way, party);
            }
        }
        self.undo = Some(Undo::Ordered { goals: before });
        Ok(Vec::new())
    }

    /// Where the plan of `ids` (a side's, on one hex) for this round ends,
    /// if they have one together: all of them planned, by the same stops
    /// and the same way. From there they can be sent on (`Order::Onward`).
    pub fn plan_end(&self, ids: &[UnitId]) -> Option<Hex> {
        let units: Vec<&Unit> = ids.iter().map(|id| self.map.units().get(*id)).collect::<Option<_>>()?;
        let first = units.first()?;
        let same = |u: &&Unit| u.planned && u.pos() == first.pos() && u.route == first.route && u.way == first.way;
        let end = first.way.last().copied().filter(|end| first.route.last() == Some(end))?;
        units.iter().all(same).then_some(end)
    }

    /// True if a plan of `side`'s that ends on `hex` can have more after
    /// it: nobody of another side is there that it sees (getting there is a
    /// battle, which ends the attackers' round), nor a city to take.
    fn goes_on_from(&self, side: Side, hex: Hex, sight: &Sight) -> bool {
        let fight = hostile(&self.map, side, hex) && sight.sees(hex);
        !fight && self.map.city_at(hex).is_none_or(|c| sight.holder(c) == side)
    }

    /// True if `ids` would be stopped for the round on `hex`, a stop of
    /// their plan: a temple that has not blessed all of them (they seek its
    /// blessing there, `stops`).
    fn stopped_at(&self, ids: &[UnitId], hex: Hex) -> bool {
        self.map.tile(hex).is_some_and(|t| t.kind == TileType::Temple)
            && ids.iter().filter_map(|id| self.map.units().get(*id)).any(|u| !u.blessed_at(hex))
    }

    /// Where `ids` (a side's, with one plan for this round) can all go on
    /// to from where their plan ends (`plan_end`), with what its way leaves
    /// each of them. `None` if they have no plan together, or none that
    /// can have more after it (it ends in a battle, in a city to take, or
    /// on a temple that stops them).
    pub(super) fn onward_range(&self, ids: &[UnitId], sight: &Sight) -> Option<crate::state::MoveRange> {
        let from = self.plan_end(ids)?;
        let side = self.map.units().get(*ids.first()?)?.side();
        if !self.goes_on_from(side, from, sight) || self.stopped_at(ids, from) {
            return None;
        }
        self.range_from(ids, from, sight, true)
    }

    /// What `unit`, selected to be sent on from its plan's end
    /// (`Game::onward_from`), has left there: its movement less the plan's
    /// way. `None` if it is not.
    pub fn onward_left(&self, unit: &Unit) -> Option<u32> {
        let from = self.onward_from()?;
        self.left_at(unit, unit.move_cost, from, &self.sight(unit.side()), true)
    }

    /// `ids` by the hex they stand on, in their order.
    fn by_hex(&self, ids: &[UnitId]) -> Vec<(Hex, Vec<UnitId>)> {
        let mut stacks: Vec<(Hex, Vec<UnitId>)> = Vec::new();
        for unit in ids.iter().filter_map(|id| self.map.units().get(*id)) {
            match stacks.iter_mut().find(|(at, _)| *at == unit.pos()) {
                Some((_, stack)) => stack.push(unit.id()),
                None => stacks.push((unit.pos(), vec![unit.id()])),
            }
        }
        stacks
    }

    /// The plan of `ids` (`side`'s, on several hexes) to go to `to` as one
    /// stack: they gather on the one of their hexes nearest to `to` that
    /// the others can all get to, with room for them, and all get on to
    /// `to` from; those that stand there wait for the others. `None` if
    /// there is no such hex.
    fn muster(&self, side: Side, ids: &[UnitId], to: Hex, sight: &Sight) -> Option<Legs> {
        let stacks = self.by_hex(ids);
        let mut hexes: Vec<Hex> = stacks.iter().map(|(at, _)| *at).collect();
        hexes.sort_by_key(|hex| (hex.distance(to), hex.row, hex.col));
        hexes.into_iter().find_map(|on| {
            let mut coming = Vec::new();
            for (from, stack) in stacks.iter().filter(|(from, _)| *from != on) {
                coming.extend(self.legs(side, stack, (*from, on), true, sight, (None, false)).ok()?);
            }
            self.legs(side, ids, (on, to), true, sight, (Some(&coming), false)).ok()
        })
    }

    /// True if, in rounds, one order sending `ids` of `side` to `to` would
    /// make one stack of them (`plan`): they stand on one hex, or are sent
    /// to join each other, or can gather on one of their hexes and get on
    /// from there. If not, the units of each hex would go on their own: so
    /// many attacks, one after the other. For an agent that weighs the odds
    /// of them all together.
    pub fn goes_as_one(&self, side: Side, ids: &[UnitId], to: Hex) -> bool {
        self.by_hex(ids).len() <= 1
            || self.gathering(ids).is_some()
            || self.muster(side, ids, to, &self.sight(side)).is_some()
    }

    /// Where `ids` (a side's) set off from together, if they do: the hex
    /// they all stand on; or, on several hexes, the one some of them stand
    /// on that all the others are sent to (a plan whose way ends there):
    /// they join there, and go on as one stack.
    fn gathering(&self, ids: &[UnitId]) -> Option<Hex> {
        let units: Vec<&Unit> = ids.iter().filter_map(|id| self.map.units().get(*id)).collect();
        let comes = |u: &Unit, hex: Hex| u.planned && u.route.last() == Some(&hex) && u.way.last() == Some(&hex);
        let here = |hex: &Hex| units.iter().all(|u| u.pos() == *hex || comes(u, *hex));
        units.iter().map(|u| u.pos()).find(here)
    }

    /// True if `ids` set off from one hex together (`gathering`), some of
    /// them only once they have come there, and can go together.
    pub(super) fn gathers(&self, ids: &[UnitId]) -> bool {
        self.gathering(ids).is_some() && self.group(ids).is_ok()
    }

    /// The units of `side` that are sent to `hex` and are not there yet (a
    /// plan that ends on it), in rounds while orders are given: those who
    /// stand there can be ordered on with them (`plan`). For a screen, to
    /// offer them with the stack.
    pub fn joining(&self, side: Side, hex: Hex) -> Vec<UnitId> {
        let comes =
            |u: &&Unit| u.planned && u.pos() != hex && u.route.last() == Some(&hex) && u.way.last() == Some(&hex);
        self.map.units().of_side(side).filter(|u| self.map.contains(u.pos())).filter(comes).map(|u| u.id()).collect()
    }

    /// What `unit` (at the prices of `table`) has left when it stands on
    /// `from`: its movement, less the way there if it is not there yet (a
    /// plan takes it there). `None` if it cannot get there.
    fn left_at(&self, unit: &Unit, table: MoveCostId, from: Hex, sight: &Sight, onward: bool) -> Option<u32> {
        // (Sent on from its plan's end, `onward`, it has gone the plan's
        // way though that ends where it stands: out and back.)
        if unit.pos() == from && !onward {
            return Some(unit.movement);
        }
        // A plan that takes it there: by the plan's way (its waypoints may
        // make it longer than the cheapest).
        if unit.planned && unit.way.last() == Some(&from) {
            let costs = self.map.units().move_cost(table)?;
            let (_, steps) = self.ground(unit, costs, sight);
            let mut spent = 0u32;
            let mut at = unit.pos();
            for hex in &unit.way {
                spent += u32::from(steps(at, *hex)?);
                at = *hex;
            }
            return unit.movement.checked_sub(spent);
        }
        let spent = self.known_range_by(unit, table, sight).cost(from)?;
        Some(unit.movement.saturating_sub(spent))
    }

    /// Where `ids` (a side's) can all go this round, setting off from the
    /// hex they are on or gather on (`gathering`), as their side knows the
    /// map: for units that come to join the others first, with what the
    /// way there leaves them. `None` if they do not go together.
    pub(super) fn joint_range(&self, ids: &[UnitId], sight: &Sight) -> Option<crate::state::MoveRange> {
        let from = self.gathering(ids)?;
        self.range_from(ids, from, sight, false)
    }

    /// Where `ids` (a side's) can all go this round from `from`, the hex
    /// they stand on or their plans take them to (`onward`: sent on from
    /// their plan's end), with what each has left there (`left_at`).
    fn range_from(&self, ids: &[UnitId], from: Hex, sight: &Sight, onward: bool) -> Option<crate::state::MoveRange> {
        let group = self.group(ids).ok()?;
        // Units that go at different prices (flyers with units that walk):
        // where there is a way all of them can go and pay for.
        if group.mixed() {
            let units = self.map.units();
            let left = |(id, table): &(UnitId, MoveCostId)| self.left_at(units.get(*id)?, *table, from, sight, onward);
            let left: Vec<u32> = group.movers.iter().map(left).collect::<Option<_>>()?;
            return Some(self.reach_together(&group, from, &left, sight)?.range());
        }
        let mut range: Option<crate::state::MoveRange> = None;
        for (id, table) in &group.movers {
            let unit = self.map.units().get(*id)?;
            let left = self.left_at(unit, *table, from, sight, onward)?;
            let costs = self.map.units().move_cost(*table)?;
            let (pass, steps) = self.ground(unit, costs, sight);
            let one = reach_with(&self.map, from, &steps, left, &pass);
            range = Some(match range {
                Some(all) => all.both(&one),
                None => one,
            });
        }
        range
    }

    /// The plan of `ids` (`side`'s, a stack: on `from`, or sent there) to
    /// go from `from` to `to`: for each, its stops and its way, hex by hex.
    /// Why not, as a move is refused: who cannot go together, cannot get
    /// there (`reach`), has no room there, or would leave others in the
    /// water. Without `reach` (`Order::Toward`) the way is as far as it is
    /// known. `coming`: the stops and the ways of those that come to
    /// `from` first, if they are not the units' own yet (`muster`);
    /// `onward`: they are sent on from their plan's end, and keep its way
    /// (though it ends where they stand).
    fn legs(
        &self,
        side: Side,
        ids: &[UnitId],
        (from, to): (Hex, Hex),
        reach: bool,
        sight: &Sight,
        (coming, onward): (Option<&Legs>, bool),
    ) -> Result<Legs, String> {
        let group = self.group(ids)?;
        // What each that moves has left on `from`; the way is the one of
        // whoever has least, if they all go at one table's prices; else
        // one all of them can go (`Game::ground_together`).
        let mixed = group.mixed();
        let mut slowest: Option<(&Unit, MoveCostId, u32)> = None;
        let mut lefts = Vec::new();
        for (id, table) in &group.movers {
            let unit = self.own_unit(side, *id)?;
            let short = || format!("{} cannot get there this turn", unit.name);
            let left = self.left_at(unit, *table, from, sight, onward).ok_or_else(short)?;
            lefts.push(left);
            if reach && !mixed {
                let costs = self.map.units().move_cost(*table).ok_or_else(short)?;
                let (pass, steps) = self.ground(unit, costs, sight);
                reach_with(&self.map, from, &steps, left, &pass).cost(to).ok_or_else(short)?;
            }
            if slowest.is_none_or(|(_, _, least)| left < least) {
                slowest = Some((unit, *table, left));
            }
        }
        let (lead, table, left) = slowest.ok_or("no units")?;
        let hexes = |way: Vec<(Hex, u32)>| -> Vec<Hex> { way.into_iter().map(|(hex, _)| hex).collect() };
        let way: Vec<Hex> = if reach && mixed {
            let together = self.reach_together(&group, from, &lefts, sight).ok_or("no units")?;
            let way = together.way(to).ok_or("they cannot all get there together this turn")?;
            way.into_iter().map(|(hex, _)| hex).collect()
        } else if reach {
            let costs = self.map.units().move_cost(table).ok_or("no units")?;
            let (pass, steps) = self.ground(lead, costs, sight);
            let way = path_with(&self.map, (from, to), &steps, left, &pass);
            hexes(way.ok_or(format!("{} cannot get there this turn", lead.name))?)
        } else {
            let way = self.way_toward(&group, to, sight);
            if way.is_none() && lead.pos() != to {
                return Err("there is no way there".to_string());
            }
            way.unwrap_or_default()
        };
        // Nor, as things stand, where there is no room for them (their own
        // side's full stack, a ship with no room aboard): the plan would
        // be accepted and never happen.
        if reach && !hostile(&self.map, side, to) {
            // (Those there that are on their way somewhere themselves make
            // room: they will have gone.)
            let units = self.map.units();
            let going = |id: &UnitId| units.get(*id).is_some_and(|u| !u.route.is_empty() || u.patrol.is_some());
            let staying = self.map.units_at(to).iter().filter(|id| !ids.contains(id) && !going(id));
            let all: Vec<UnitId> = staying.chain(ids).copied().collect();
            let commanders = all.iter().filter(|id| self.map.is_commander(**id)).count();
            if commanders > 1 || all.len() - commanders > MAX_STACK {
                return Err("there is no room there".to_string());
            }
            if !self.afloat(to, &self.there_after(side, to, &[], ids)) {
                return Err("there is no room aboard for them there".to_string());
            }
        }
        let mut origins: Vec<Hex> = ids.iter().filter_map(|id| self.map.units().get(*id)).map(|u| u.pos()).collect();
        origins.dedup();
        for origin in origins {
            if !self.afloat(origin, &self.there_after(side, origin, ids, &[])) {
                return Err("that would leave units in the water with no ship to carry them".to_string());
            }
        }
        // Who stands on `from` goes from there; who comes there first keeps
        // its way there, and goes on.
        let mut legs = Vec::new();
        for unit in ids.iter().filter_map(|id| self.map.units().get(*id)) {
            let sent = coming.and_then(|legs| legs.iter().find(|(id, _, _)| *id == unit.id()));
            let (mut route, mut hexes) = match sent {
                _ if unit.pos() == from && !onward => (Vec::new(), Vec::new()),
                Some((_, route, way)) => (route.clone(), way.clone()),
                None => (unit.route.clone(), unit.way.clone()),
            };
            route.push(to);
            hexes.extend(way.iter().copied());
            legs.push((unit.id(), route, hexes));
        }
        Ok(legs)
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
        // The stacks left alone after a battle on patrol or marauding go on.
        for side in self.order.clone() {
            self.ease(side);
        }
        self.note_sightings();
        // The order the sides step in, if it is drawn, is drawn now: when
        // the orders were given nobody knew who would step first (the
        // review, 2026-11-22: drawn before, it was the order the orders
        // were given in, and everybody knew).
        if self.setup.order == TurnOrder::Drawn {
            self.rng.shuffle(&mut self.order);
        }
        // Everybody sets off as the clock starts.
        self.rest(false);
        self.playing = Some(PlayOut::default());
        // Whoever watches the game, or keeps what is done to it, plays the
        // round out, a beat at a time (each beat is then a step of its
        // own, `Game::record`); else it plays out here and now.
        if self.watched.is_some() || self.journal.is_some() {
            return Vec::new();
        }
        let mut events = Vec::new();
        while self.playing.is_some() {
            events.extend(self.play_out());
        }
        events
    }

    /// Nobody is on a step: every unit's clock is at the round's start
    /// (and, `ways`, the ways found for this round are forgotten).
    fn rest(&mut self, ways: bool) {
        let busy = |u: &Unit| u.clock != 0 || u.step.is_some() || u.halted || (ways && !u.way.is_empty());
        let ids: Vec<UnitId> = self.map.units().iter().filter(|u| busy(u)).map(|u| u.id()).collect();
        for id in ids {
            if let Some(unit) = self.map.unit_mut(id) {
                (unit.clock, unit.step, unit.halted) = (0, None, false);
                if ways {
                    unit.way.clear();
                }
            }
        }
    }

    /// The way of `group` (a stack on a hex: its lead's, or, if its units
    /// go at different prices, one all of them can go) toward `to`, as its
    /// side knows the map: hex by hex. To land
    /// the side has never seen (total fog of war: shut until it is seen)
    /// there is no way yet: the way is to the hex it can get to that is
    /// nearest to it, if that is nearer than where it stands (never one
    /// another side's units stand on); as it goes it sees more, and the next
    /// tick finds the way on. So a stack can be sent to look into the dark,
    /// a step at a time over land it has seen.
    fn way_toward(&self, group: &Group, to: Hex, sight: &Sight) -> Option<Vec<Hex>> {
        let (lead, _) = group.lead(&self.map)?;
        let (side, here) = (lead.side(), lead.pos());
        let (pass, steps) = self.ground_far(group, sight)?;
        let path = |to: Hex| -> Option<Vec<Hex>> {
            let way = path_with(&self.map, (here, to), &*steps, u32::MAX, &*pass)?;
            Some(way.into_iter().map(|(hex, _)| hex).collect())
        };
        if sight.knows(to) {
            return path(to);
        }
        let range = reach_with(&self.map, here, &*steps, u32::MAX / 2, &*pass);
        let (nearest, _) = range
            .reachable()
            .filter(|(hex, _)| !hostile(&self.map, side, *hex))
            .min_by_key(|(hex, cost)| (hex.distance(to), *cost, hex.row, hex.col))?;
        if nearest.distance(to) >= here.distance(to) {
            return None;
        }
        path(nearest)
    }

    /// The round that plays out goes on (`playing_out`), a beat of its
    /// clock ([`BEAT`]), or up to a battle: the stacks that get where their
    /// steps take them in it, in the clock's order (`arrive`), each then
    /// setting off on its next (`set_off`); and, once nobody is on a step,
    /// the round's end and the next one's beginning (`Event::Began` for the
    /// side that gives its orders first). What happened, in its order.
    /// (However it is called, the round is the same: a beat is only how
    /// much a screen is shown at a time.)
    pub fn play_out(&mut self) -> Vec<Event> {
        // (No order: kept as a step of its own, `record`.)
        if self.playing.is_some() {
            if let Some(journal) = &mut self.journal {
                journal.push(crate::orders::Step::Beat);
            }
        }
        self.beat()
    }

    fn beat(&mut self) -> Vec<Event> {
        let Some(mut play) = self.playing.clone() else { return Vec::new() };
        let round = self.round;
        let mut events = Vec::new();
        let mut end = None;
        loop {
            if self.winner.is_some() || play.steps >= MAX_STEPS {
                break;
            }
            let (happened, went) = self.set_off(&play);
            for (side, set_off) in happened {
                self.remember(round, side, &set_off);
                events.extend(set_off);
            }
            // (Somebody set off who had waited: the others wait again.)
            if went {
                play.hasty = false;
            }
            let Some((due, side, ids, hex)) = self.next_there() else {
                // Nobody is on a step. Those that wait for others of their
                // side who do not come wait no longer; else the round is
                // over.
                if !play.hasty && self.any_waits(&play) {
                    play.hasty = true;
                    continue;
                }
                break;
            };
            let due = due.max(play.now);
            if due >= *end.get_or_insert((due / BEAT + 1) * BEAT) {
                self.playing = Some(play);
                return events;
            }
            (play.now, play.steps) = (due, play.steps + 1);
            self.fought.clear();
            let stepped = self.arrive(side, &ids, hex, &play);
            let battled = stepped.iter().any(|e| matches!(e, Event::Battle { .. }));
            self.remember(round, side, &stepped);
            self.note_sightings();
            events.extend(stepped);
            // A battle ends the beat: a screen shows it before the round
            // goes on.
            if battled && self.winner.is_none() {
                self.playing = Some(play);
                return events;
            }
        }
        events.extend(self.close_round());
        events
    }

    /// True if `unit` has somewhere to go as the round plays out: a route
    /// (a plan, a march) or a patrol (not one that fought and waits for its
    /// player's word), and has not stopped for the round.
    fn goes(&self, unit: &Unit) -> bool {
        self.map.contains(unit.pos())
            && !unit.halted
            && (!unit.route.is_empty() || unit.patrol.is_some() && !unit.fought)
    }

    /// The stack `unit` goes in: the units of its hex and side that go
    /// where it goes (the same route, of the same order: `Unit::party`; or
    /// the same patrol) and are on the same step.
    fn stack_of(&self, unit: &Unit) -> Vec<UnitId> {
        let party = |u: &Unit| if u.planned { u.party.max(1) } else { 0 };
        let with = |u: &&Unit| {
            u.side() == unit.side()
                && self.goes(u)
                && u.step == unit.step
                && u.route == unit.route
                && party(u) == party(unit)
                && (!unit.route.is_empty() || u.patrol == unit.patrol)
        };
        let here = self.map.units_at(unit.pos()).iter().filter_map(|id| self.map.units().get(*id));
        here.filter(with).map(|u| u.id()).collect()
    }

    /// The stacks with somewhere to go that are on no step, the sides in
    /// the round's order: each side, and its stack.
    fn idle_stacks(&self) -> Vec<(Side, Vec<UnitId>)> {
        let mut stacks: Vec<(Side, Vec<UnitId>)> = Vec::new();
        for side in self.order.iter().filter(|side| !self.out.contains(side)) {
            let from = stacks.len();
            for unit in self.map.units().of_side(*side).filter(|u| u.step.is_none() && self.goes(u)) {
                if stacks[from..].iter().any(|(_, stack)| stack.contains(&unit.id())) {
                    continue;
                }
                // (A patrol one of which fought waits for its player's word.)
                let stack = self.stack_of(unit);
                if !stack.is_empty() {
                    stacks.push((*side, stack));
                }
            }
        }
        stacks
    }

    /// Every stack with somewhere to go that is on no step sets off on its
    /// next (`look`), or waits, or stops for the round. First it finds its
    /// way, if it has none (a march, as it sets off; a plan toward land
    /// never seen, as far as it knows it), and passes the stops it is on.
    /// What happened at the stops (a blessing sought), side by side.
    fn set_off(&mut self, play: &PlayOut) -> (Vec<(Side, Vec<Event>)>, bool) {
        let mut happened = Vec::new();
        // Told that nobody comes (`hasty`), a stack that could go only once
        // another has left is not stopped for the round if some stack does
        // set off now: it waits behind that one (the review, 2026-11-23: a
        // stack sent where a bigger one stood, which left by way of its
        // hex, was stopped though the hex was empty a moment later).
        let mut held: Vec<(Side, Vec<UnitId>)> = Vec::new();
        let mut went = false;
        for (side, stack) in self.idle_stacks() {
            let patrols = self.map.units().get(stack[0]).is_some_and(|u| u.route.is_empty());
            let mut stack = stack;
            if patrols {
                self.patrol_waypoints(&stack);
            } else {
                let events = self.stops(side, &stack);
                if !events.is_empty() {
                    happened.push((side, events));
                }
                // (Those whose order ended there go nowhere.)
                stack.retain(|id| self.map.units().get(*id).is_some_and(|u| self.goes(u)));
                if stack.is_empty() {
                    continue;
                }
                self.find_way(side, &stack);
            }
            let look = if patrols { self.patrol_look(side, &stack) } else { self.look(side, &stack, play.hasty) };
            if play.hasty && !patrols && look == Look::Stop {
                held.push((side, stack));
                continue;
            }
            went |= play.hasty && matches!(look, Look::Go { .. });
            self.take_look(&stack, &look, play.now);
        }
        for (side, stack) in held {
            let look = if went { self.look(side, &stack, false) } else { Look::Stop };
            self.take_look(&stack, &look, play.now);
        }
        (happened, went)
    }

    /// The stack `stack` does what it found to do next (`look`), at the
    /// moment `now` of the round.
    fn take_look(&mut self, stack: &[UnitId], look: &Look, now: u64) {
        let clock = stack.iter().filter_map(|id| self.map.units().get(*id)).map(|u| u.clock).max().unwrap_or(0);
        let at = stack.first().and_then(|id| self.map.units().get(*id)).map(|u| u.pos());
        for id in stack {
            let Some(unit) = self.map.unit_mut(*id) else { continue };
            match look {
                Look::Go { hex, time, .. } => unit.step = Some((*hex, clock + *time)),
                // (A rest is a step to where it stands.)
                Look::Rest => unit.step = at.map(|at| (at, now + REST)),
                Look::Wait => unit.clock = unit.clock.max(now),
                Look::Stop => unit.halted = true,
            }
        }
    }

    /// The stack whose step is up first: when, its side, its units, and
    /// the hex it is to get to. Of those there at the same moment, the
    /// first in the round's order of the sides (and of a side's, in its
    /// units' order). None if nobody is on a step (patrols that rest are
    /// on none: they do not keep the round going).
    fn next_there(&self) -> Option<(u64, Side, Vec<UnitId>, Hex)> {
        let mut first: Option<(u64, &Unit, Hex)> = None;
        let mut moving = false;
        for side in self.order.iter().filter(|side| !self.out.contains(side)) {
            for unit in self.map.units().of_side(*side).filter(|u| self.goes(u)) {
                let Some((hex, due)) = unit.step else { continue };
                moving |= hex != unit.pos();
                if first.is_none_or(|(least, _, _)| due < least) {
                    first = Some((due, unit, hex));
                }
            }
        }
        first.filter(|_| moving).map(|(due, unit, hex)| (due, unit.side(), self.stack_of(unit), hex))
    }

    /// True if a stack waits (for others of its side) that would not if it
    /// were told nobody comes.
    fn any_waits(&self, play: &PlayOut) -> bool {
        let hasty = PlayOut { hasty: true, ..play.clone() };
        self.idle_stacks().iter().any(|(side, stack)| {
            let patrols = self.map.units().get(stack[0]).is_some_and(|u| u.route.is_empty());
            !patrols
                && self.look(*side, stack, play.hasty) == Look::Wait
                && self.look(*side, stack, hasty.hasty) != Look::Wait
        })
    }

    /// A stack of `side` with a route (`ids`, on a hex) passes the stops it
    /// stands on: on to the next (none: there, and the order ends; at a
    /// rally point that says so, they defend there). On the last leg to a
    /// rally point that is full, its stop is beside it. A plan or a march
    /// that stops on a temple seeks its blessing there (nobody can be asked
    /// as the round plays out), which stops it for the round.
    fn stops(&mut self, side: Side, ids: &[UnitId]) -> Vec<Event> {
        let mut events = Vec::new();
        loop {
            let units: Vec<&Unit> = ids.iter().filter_map(|id| self.map.units().get(*id)).collect();
            let Some(first) = units.first() else { break };
            let (at, mut route, bound) = (first.pos(), first.route.clone(), first.bound);
            // On the last leg to a rally point: to the point, or beside it
            // once it is full.
            if let (Some(n), [end]) = (bound, route.as_slice()) {
                let spot = self.rally_spot(side, n, ids, at).unwrap_or(*end);
                if spot != *end {
                    route = vec![spot];
                    for id in ids {
                        if let Some(unit) = self.map.unit_mut(*id) {
                            unit.route = route.clone();
                            unit.way.clear();
                        }
                    }
                }
            }
            if route.first() != Some(&at) {
                break;
            }
            let defend = bound.and_then(|n| self.rally_point(side, n)).is_some_and(|p| p.defend);
            let last = route.len() == 1;
            for id in ids {
                if let Some(unit) = self.map.unit_mut(*id) {
                    (unit.route, unit.stalled) = (route[1..].to_vec(), false);
                    if last {
                        (unit.bound, unit.planned) = (None, false);
                        unit.defending |= defend && bound.is_some();
                        unit.way.clear();
                    }
                }
            }
            if self.map.tile(at).is_some_and(|t| t.kind == TileType::Temple) {
                let blessed = self.blessings(at, ids);
                if !blessed.is_empty() {
                    events.extend(blessed);
                    break;
                }
            }
        }
        events
    }

    /// A stack of `side` with a route (`ids`, on a hex) that has no way, or
    /// one that does not lead on from where it stands, finds it: the way to
    /// its next stop as its side knows the map now, round the other sides'
    /// stacks it sees (`way_toward`); if there is none round them, the way
    /// through them.
    fn find_way(&mut self, side: Side, ids: &[UnitId]) {
        let Some(first) = ids.first().and_then(|id| self.map.units().get(*id)) else { return };
        let (at, to) = (first.pos(), first.goto());
        if first.way.first().is_some_and(|next| next.distance(at) == 1) {
            return;
        }
        let way = to.and_then(|to| {
            let group = self.group(ids).ok()?;
            let sight = self.sight(side);
            let round = self.way_toward(&group, to, &sight);
            round.or_else(|| self.way_through(&group, to, &sight))
        });
        let way: Vec<Hex> = way.unwrap_or_default();
        for id in ids {
            if let Some(unit) = self.map.unit_mut(*id) {
                unit.way = way.clone();
            }
        }
    }

    /// The way of `group` (as `way_toward`) to `to` over the land its side
    /// knows, whoever stands on it: for a stack with no way round the other
    /// sides' stacks (it fights the first it comes to). Another side's city
    /// still ends a way.
    fn way_through(&self, group: &Group, to: Hex, sight: &Sight) -> Option<Vec<Hex>> {
        let (lead, _) = group.lead(&self.map)?;
        let (pass, steps) = self.ground_far(group, sight)?;
        let side = lead.side();
        let through = |hex: Hex| match pass(hex) {
            Passage::Stop if hostile(&self.map, side, hex) && self.map.city_at(hex).is_none_or(|c| c.side == side) => {
                Passage::Open
            }
            other => other,
        };
        let way = path_with(&self.map, (lead.pos(), to), &*steps, u32::MAX, &through)?;
        Some(way.into_iter().map(|(hex, _)| hex).collect())
    }

    /// This step of `side`'s stack `ids` along `way` (each hex and what it
    /// costs to get there), with `movement` to pay for it: the next hex;
    /// or, past hexes where the side's own units leave no room for them,
    /// the first with room (a move passes through its side's stacks). None
    /// if it cannot pay for it, or there is none.
    pub(super) fn step_along(&self, side: Side, ids: &[UnitId], way: &[(Hex, u32)], movement: u32) -> Option<Hex> {
        way.iter()
            .take_while(|(_, cost)| *cost <= movement)
            .find(|(hex, _)| hostile(&self.map, side, *hex) || self.fits(*hex, ids))
            .map(|(hex, _)| *hex)
    }

    /// What going from `at` over `hexes`, one after the other, costs each
    /// of `movers` as things stand (into another side's units: an attack's
    /// price); `None` for one that cannot go that way.
    pub(super) fn costs_along(&self, movers: &[(UnitId, MoveCostId)], at: Hex, hexes: &[Hex]) -> Vec<Option<u32>> {
        let cost = |(id, table): &(UnitId, MoveCostId)| {
            let unit = self.map.units().get(*id)?;
            let costs = self.map.units().move_cost(*table)?;
            let (_, steps) = self.ground(unit, costs, &SEE_ALL);
            let mut from = at;
            let mut all = 0u32;
            for hex in hexes {
                all += u32::from(steps(from, *hex)?);
                from = *hex;
            }
            Some(all)
        };
        movers.iter().map(cost).collect()
    }

    /// `group` (a stack on `at`) sets off over `hexes` (the last is where
    /// it gets to), if every one of it that moves can pay for it, and it
    /// takes them as long as the slowest of them: what the way costs each over its
    /// movement a turn, of a [`ROUND`] (not counting a commander among
    /// others: it keeps up with them). It stops if none can.
    pub(super) fn go(&self, group: &Group, at: Hex, hexes: &[Hex]) -> Look {
        let Some(hex) = hexes.last().copied().filter(|_| !group.movers.is_empty()) else { return Look::Stop };
        let costs = self.costs_along(&group.movers, at, hexes);
        let units = self.map.units();
        let able = |((id, _), cost): (&(UnitId, MoveCostId), &Option<u32>)| {
            let unit = units.get(*id)?;
            cost.filter(|c| *c <= unit.movement).map(|c| (unit, c))
        };
        let able: Vec<(&Unit, u32)> = group.movers.iter().zip(&costs).filter_map(able).collect();
        // All of them, or none: a stack stays together (the owner's word,
        // 2026-11-22; its way is one all of them can go, `way_toward`).
        if able.len() < group.movers.len() {
            return Look::Stop;
        }
        // (A commander goes at the pace of those it leads: a captain with
        // riders rides, as its movement says.)
        let led = able.iter().any(|(unit, _)| !self.map.is_commander(unit.id()));
        let paced = able.iter().filter(|(unit, _)| !(led && self.map.is_commander(unit.id())));
        let time = paced.map(|(unit, cost)| (u64::from(*cost) * ROUND).div_ceil(u64::from(unit.speed()))).max();
        let coming = able.iter().map(|(unit, _)| unit.id()).chain(group.carried.iter().copied()).collect();
        Look::Go { hex, passed: hexes.len(), coming, time: time.unwrap_or(0) }
    }

    /// What a stack of `side` with a route (`ids`, on a hex, with its way)
    /// does next: it sets off on the next hex of its way, whoever of
    /// another side stands there (getting there is an attack). Past hexes
    /// its own side's stacks leave it no room on, it goes on to the first
    /// with room, as a move passes through them; but behind those that are
    /// on their way themselves it waits. It waits, too, for the units of
    /// its order that come to join it where it stands (`awaited`). `hasty`:
    /// it waits for nobody. It stops for the round if it cannot pay for the
    /// step, or has no way.
    pub(super) fn look(&self, side: Side, ids: &[UnitId], hasty: bool) -> Look {
        let Some(first) = ids.first().and_then(|id| self.map.units().get(*id)) else { return Look::Stop };
        let at = first.pos();
        if first.way.first().is_none_or(|next| next.distance(at) != 1) {
            return Look::Stop;
        }
        if !hasty && self.awaited(first, ids) {
            return Look::Wait;
        }
        let Ok(group) = self.group(ids) else { return Look::Stop };
        if group.lead(&self.map).is_none_or(|(lead, _)| lead.movement == 0) {
            return Look::Stop;
        }
        for (i, hex) in first.way.iter().enumerate() {
            if hostile(&self.map, side, *hex) || self.fits(*hex, ids) {
                return self.go(&group, at, &first.way[..=i]);
            }
            // Its own side's stack that comes the other way: the two pass
            // each other.
            if self.passes(ids, at, &first.way[..=i], hasty).is_some() {
                return self.go(&group, at, &first.way[..=i]);
            }
            // Its own side's stack, and no room: it follows one that is on
            // its way, and goes through one that stays.
            let leaving = |id: &UnitId| self.map.units().get(*id).is_some_and(|u| self.goes(u) && !ids.contains(id));
            if !hasty && self.map.units_at(*hex).iter().any(leaving) {
                return Look::Wait;
            }
        }
        Look::Stop
    }

    /// The stack that `ids` (a stack on `at`, whose step over `hexes`, the
    /// last where it would get to, finds no room there) passes, and the
    /// hexes of that stack's own step: one of its side that stands there
    /// and goes to `at` itself (it is on its step there; or that is where
    /// its next step gets it, past the stacks that stay, and it waits for
    /// nobody: `hasty`, it does not anyway), if each of the two can pay
    /// for its step and, with both gone, each hex has room for who comes.
    /// The two change places at once (`change_places`; the owner's word,
    /// 2026-11-23: "we really don't want overloaded hexes"). Two stacks
    /// sent to each other's hexes stood still for the round before: each
    /// waited for the other to leave.
    fn passes(&self, ids: &[UnitId], at: Hex, hexes: &[Hex], hasty: bool) -> Option<(Vec<UnitId>, Vec<Hex>)> {
        let hex = *hexes.last()?;
        let units = self.map.units();
        let comes = |u: &&Unit| self.goes(u) && !ids.contains(&u.id()) && !u.route.is_empty() && u.way.contains(&at);
        let first = self.map.units_at(hex).iter().filter_map(|id| units.get(*id)).find(comes)?;
        let (side, them) = (first.side(), self.stack_of(first));
        let there = first.way.iter().position(|h| *h == at)?;
        let back = &first.way[..=there];
        match first.step {
            Some((to, _)) if to != at => return None,
            Some(_) => {}
            None => {
                if !hasty && self.awaited(first, &them) {
                    return None;
                }
                // Its next step gets it to `at`: it goes through the hexes
                // before (its side's, with no room, where nobody is
                // leaving).
                let leaving = |id: &UnitId| {
                    units.get(*id).is_some_and(|u| self.goes(u) && !them.contains(id) && !ids.contains(id))
                };
                let through = |h: &Hex| {
                    !hostile(&self.map, side, *h)
                        && !self.fits(*h, &them)
                        && (hasty || !self.map.units_at(*h).iter().any(leaving))
                };
                if !back[..there].iter().all(through) {
                    return None;
                }
            }
        }
        let pays = |ids: &[UnitId], from: Hex, over: &[Hex]| {
            self.group(ids).is_ok_and(|group| matches!(self.go(&group, from, over), Look::Go { .. }))
        };
        if !pays(ids, at, hexes) || !pays(&them, hex, back) || !self.map.could_change_places(ids, &them) {
            return None;
        }
        // (On open water, who rides has a ship there still.)
        let afloat = self.afloat(at, &self.there_after(side, at, ids, &them))
            && self.afloat(hex, &self.there_after(side, hex, &them, ids));
        afloat.then(|| (them, back.to_vec()))
    }

    /// Two stacks of `side`, each on its step to the hex of the other
    /// (`passes`: the units of each and the hexes of its step, the last the
    /// other's), change places, and each of them pays its step. Refused,
    /// and nothing changes, if one cannot pay or there is no room.
    fn change_places(
        &mut self,
        side: Side,
        ours: (&[UnitId], &[Hex]),
        theirs: (&[UnitId], &[Hex]),
    ) -> Result<Vec<Event>, String> {
        let at = |ids: &[UnitId]| ids.first().and_then(|id| self.map.units().get(*id)).map(|u| u.pos());
        let (Some(x), Some(y)) = (at(ours.0), at(theirs.0)) else { return Err("no units".to_string()) };
        if ours.1.last() != Some(&y) || theirs.1.last() != Some(&x) {
            return Err("they do not go to each other's hexes".to_string());
        }
        let mut going: Vec<(UnitId, Hex, u32)> = Vec::new();
        for ((ids, over), from) in [(ours, x), (theirs, y)] {
            let group = self.group(ids)?;
            let costs = self.costs_along(&group.movers, from, over);
            for ((id, _), cost) in group.movers.iter().zip(costs) {
                let unit = self.own_unit(side, *id)?;
                let cost = cost.filter(|c| *c <= unit.movement).ok_or(format!("{} cannot get there", unit.name))?;
                going.push((*id, from, cost));
            }
            going.extend(group.carried.iter().map(|id| (*id, from, 0)));
        }
        let from = |hex: Hex| going.iter().filter(|g| g.1 == hex).map(|g| g.0).collect::<Vec<UnitId>>();
        let (a, b) = (from(x), from(y));
        self.map.change_places(&a, &b).map_err(|_| "there is no room there".to_string())?;
        let way = |from: Hex, over: &[Hex]| std::iter::once(from).chain(over.iter().copied()).collect::<Vec<Hex>>();
        let (way_a, way_b) = (way(x, ours.1), way(y, theirs.1));
        let mut events = Vec::new();
        for (id, from, cost) in going {
            let (to, way) = if from == x { (y, way_a.clone()) } else { (x, way_b.clone()) };
            events.push(Event::Moved { unit: id, from, to, cost, way });
            self.pay(id, cost);
            // (As a move leaves them, `move_units`: what they were about
            // is `arrive`'s to give back.)
            if let Some(unit) = self.map.unit_mut(id) {
                (unit.moved, unit.stalled, unit.defending) = (true, false, false);
                (unit.route, unit.bound) = (Vec::new(), None);
                unit.stand_down();
            }
        }
        for (way, count) in [(way_a, a.len()), (way_b, b.len())] {
            self.see_along(side, &way);
            self.seen_leaving(side, &way, count);
        }
        self.selected.retain(|id| !a.contains(id) && !b.contains(id));
        if self.selected.is_empty() {
            self.deselect();
        }
        Ok(events)
    }

    /// True if units of the order of `first` (which stands on a hex with
    /// `ids`) are still to come there, to go on with it: they have a stop
    /// on that hex, are not on it, and have not stopped for the round.
    fn awaited(&self, first: &Unit, ids: &[UnitId]) -> bool {
        let at = first.pos();
        if !first.planned || first.party == 0 {
            return false;
        }
        let comes = |u: &Unit| {
            u.planned
                && u.party == first.party
                && u.pos() != at
                && self.goes(u)
                && u.route.contains(&at)
                && !ids.contains(&u.id())
        };
        self.map.units().of_side(first.side()).any(comes)
    }

    /// The step of a stack of `side` (`ids`) to `hex` is up, at the moment
    /// of `play`: if that is still its step as things stand now, it is
    /// there: a move, or, onto a hex another side stands on, an attack (a
    /// battle ends a plan, and uses up the attackers' movement). If it is
    /// not (the hex is its own side's now, and full; its patrol has other
    /// prey), it looks again from now. A stack that cannot make the step
    /// after all stops for the round. What happened.
    fn arrive(&mut self, side: Side, ids: &[UnitId], hex: Hex, play: &PlayOut) -> Vec<Event> {
        let Some(first) = ids.first().and_then(|id| self.map.units().get(*id)) else { return Vec::new() };
        let (at, patrol, way) =
            (first.pos(), first.patrol.clone().filter(|_| first.route.is_empty()), first.way.clone());
        let look = match &patrol {
            // (A rest is over.)
            _ if hex == at => Look::Wait,
            Some(_) => self.patrol_look(side, ids),
            // (It set off: it waits for nobody now, though the others of
            // its side wait again, `set_off`.)
            None => match self.look(side, ids, play.hasty) {
                Look::Wait => self.look(side, ids, true),
                look => look,
            },
        };
        let mut events = Vec::new();
        let (mut halt, mut came, mut passed_by) = (false, Vec::new(), Vec::new());
        match look {
            Look::Go { hex: next, passed, coming, .. } if next == hex => {
                // Onto its own side's stack that comes the other way, with
                // no room for both: the two change places (`passes`).
                let full = !hostile(&self.map, side, hex) && !self.fits(hex, &coming);
                let over = &way[..passed.min(way.len())];
                let them = if full { self.passes(ids, at, over, true) } else { None };
                // How each stood, the way it was on, where its step takes
                // it and how many hexes on that is.
                type Stood = Vec<(UnitId, MarchState, Vec<Hex>, Hex, usize)>;
                let stood = |ids: &[UnitId], way: &[Hex], to: Hex, passed: usize| -> Stood {
                    let units = ids.iter().filter_map(|id| self.map.units().get(*id));
                    units.map(|u| (u.id(), MarchState::of(u), way.to_vec(), to, passed)).collect()
                };
                let mut was = stood(&coming, &way, hex, passed);
                if let Some((them, back)) = &them {
                    let theirs = them.first().and_then(|id| self.map.units().get(*id)).map(|u| u.way.clone());
                    was.extend(stood(them, &theirs.unwrap_or_default(), at, back.len()));
                }
                let moved = match &them {
                    Some((them, back)) => self.change_places(side, (&coming, over), (them, back)),
                    None => self.move_units(side, &coming, hex),
                };
                match moved {
                    Ok(moved) => {
                        let battled = moved.iter().any(|e| matches!(e, Event::Battle { .. }));
                        let took = moved.iter().any(|e| matches!(e, Event::Taken { .. }));
                        events.extend(moved);
                        // The move ended their orders: they keep them (a
                        // plan, until it has fought or taken a city;
                        // marauders maraud on, and wait for their player's
                        // word after a battle; a patrol as ever).
                        for (id, was, way, to, passed) in was {
                            let Some(unit) = self.map.unit_mut(id) else { continue };
                            let there = unit.pos() == to;
                            if was.patrol.is_some() {
                                (unit.patrol, unit.stalled, unit.moved) = (was.patrol, false, false);
                                unit.fought |= battled;
                            } else if !(was.planned && (battled || took)) {
                                (unit.route, unit.bound) = (was.route, was.bound);
                                (unit.planned, unit.party) = (was.planned, was.party);
                                unit.way = if there { way[passed.min(way.len())..].to_vec() } else { way.clone() };
                                // A stop gone through (its side's stack left
                                // no room on it) is behind them: not the
                                // next to make for (the review, 2026-11-23:
                                // from the plan's end the stack turned
                                // back for it).
                                if there {
                                    let through = &way[..passed.min(way.len()).saturating_sub(1)];
                                    while unit.route.len() > 1 && through.contains(&unit.route[0]) {
                                        unit.route.remove(0);
                                    }
                                }
                            }
                            if was.marauding {
                                (unit.marauding, unit.fought) = (true, battled || was.fought);
                            }
                        }
                        came = coming;
                        // The stack passed is there too: its step is done,
                        // at the moment it was to be.
                        passed_by = them.map(|(them, _)| them).unwrap_or_default();
                        for id in &passed_by {
                            if let Some(unit) = self.map.unit_mut(*id) {
                                let due = unit.step.map_or(play.now, |(_, due)| due);
                                (unit.step, unit.clock) = (None, unit.clock.max(due).max(play.now));
                            }
                        }
                    }
                    Err(_) => halt = true,
                }
            }
            Look::Stop => halt = true,
            _ => {}
        }
        for id in ids {
            if let Some(unit) = self.map.unit_mut(*id) {
                (unit.step, unit.clock) = (None, unit.clock.max(play.now));
                unit.halted |= halt;
            }
        }
        if patrol.is_none() {
            events.extend(self.stops(side, &came));
            events.extend(self.stops(side, &passed_by));
        }
        events
    }

    /// The hexes `side`'s stacks with somewhere to go (a plan, a march) pass
    /// as the round plays out, as things stand and as far as their movement
    /// takes them, each stack's in its order; and their next stops, however
    /// far. For a screen to show a side its plans.
    pub fn ways_this_round(&self, side: Side) -> Vec<Hex> {
        let mut hexes: Vec<Hex> = Vec::new();
        for hex in self.ways_by_stack(side).into_iter().flat_map(|(_, way)| way) {
            if !hexes.contains(&hex) {
                hexes.push(hex);
            }
        }
        hexes
    }

    /// `ways_this_round`, a stack at a time: its units, and the hexes of
    /// its way. For a screen to show one stack's plan apart from the
    /// others'.
    pub fn ways_by_stack(&self, side: Side) -> Vec<(Vec<UnitId>, Vec<Hex>)> {
        let sight = self.sight(side);
        let mut ways: Vec<(Vec<UnitId>, Vec<Hex>)> = Vec::new();
        let mut seen: Vec<UnitId> = Vec::new();
        for unit in self.map.units().of_side(side).filter(|u| self.goes(u)) {
            if seen.contains(&unit.id()) {
                continue;
            }
            let ids = self.stack_of(unit);
            seen.extend(ids.iter().copied());
            let patrols = unit.route.is_empty();
            let to = if patrols { unit.patrol.as_ref().map(|p| p.target()) } else { unit.goto() };
            let Some(to) = to else { continue };
            // (A march's later stops too.)
            let stops: Vec<Hex> = if patrols { vec![to] } else { unit.route.clone() };
            let Ok(group) = self.group(&ids) else { continue };
            // A plan's way is the one it was given; a march's and a
            // patrol's, as it would be found now.
            let way: Vec<Hex> = if unit.way.is_empty() {
                self.way_toward(&group, to, &sight).unwrap_or_default()
            } else {
                unit.way.clone()
            };
            // As far as all of them can pay for.
            let units = self.map.units();
            let pays = |((id, _), cost): (&(UnitId, MoveCostId), &Option<u32>)| {
                cost.is_some_and(|cost| units.get(*id).is_some_and(|u| cost <= u.movement))
            };
            let mut near = 0;
            for i in 0..way.len() {
                let costs = self.costs_along(&group.movers, unit.pos(), &way[..=i]);
                if !group.movers.iter().zip(&costs).all(pays) {
                    break;
                }
                near = i + 1;
            }
            let mut hexes: Vec<Hex> = Vec::new();
            for hex in way[..near].iter().copied().chain(stops) {
                if !hexes.contains(&hex) {
                    hexes.push(hex);
                }
            }
            ways.push((ids, hexes));
        }
        ways
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
        self.rest(true);
        let planned: Vec<UnitId> = self.map.units().iter().filter(|u| u.planned).map(|u| u.id()).collect();
        for id in planned {
            if let Some(unit) = self.map.unit_mut(id) {
                unit.route.clear();
                (unit.planned, unit.party) = (false, 0);
            }
        }
        self.parties = 0;
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
