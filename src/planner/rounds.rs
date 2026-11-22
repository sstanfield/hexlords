//! The planner in a game of rounds (the owner's word, 2026-11-22: "Round
//! based games probably need their own AI since they are very different
//! from turn based"). In turns a side sees the board, moves, and sees what
//! came of it before its next order; in rounds every side's orders are
//! given before anything moves, the round then plays out on a clock (the
//! faster stacks are there first), and a stack fights whoever of another
//! side it finds on its way (`game::rounds`). What its cities build, who
//! stays in them, where its armies gather and what they go for are the
//! planner's as in turns (`super`); what is its own in rounds, each kept
//! because it won games against the planner playing rounds as it plays
//! turns (`<style>/turns`; `docs/agents.md` has the numbers):
//!
//!
//! - **It keeps clear of what it cannot beat** (`super::wary`, as in
//!   turns), and here, where everyone moves at once, that is a matter of
//!   time: a stack it fears could be on a hex of its way by the moment its
//!   own stack passes, and it stops short of that.
//! - **An attack arrives as one.** Each stack attacks when it gets there,
//!   so it sends the units of one hex if they are enough, the nearest
//!   first; units of several hexes only where the game makes one stack of
//!   them (`Game::goes_as_one`), never as so many attacks one after the
//!   other at odds worked out for them all.
//! - **It gathers for an assault.** Stacks around a city that could take
//!   it together, but not as one stack this round, join on the one of
//!   their hexes nearest to it, and attack as one the next round.
//! - **A stack in the field is not where it was.** By the time anyone gets
//!   to it, it has gone where it was going: it is attacked only at odds
//!   well above those a city is attacked at, and one near its cities is
//!   left to come to their walls.
//!
//! (What it first did in rounds only, and now does in turns too, for
//! winning there as well: marching with an army a unit smaller, going to a
//! temple only for half of it, spending its treasury, `Planner::reserve`,
//! and the wariness itself.)
//!
//! Tried and left out, for winning nothing: holding an attack back for
//! the units of a city's holder that could be in it first; sending a
//! threatened city only those that get in before the others could; more
//! defenders at home.
//!
//! Not a port: the C++ had no players.

use super::wary::{Foe, room};
use super::{Planner, advance, daring, strongest_stack};
use crate::game::{Game, ROUND, Sight};
use crate::orders::Order;
use crate::state::{Hex, MAX_STACK, MoveRange, Side, TileType, Unit, UnitId};

/// How far above the odds it attacks a city at it attacks a stack in the
/// field, in percent: the stack may not be there any more.
const FIELD_ODDS: u32 = 15;
impl Planner {
    /// A game of rounds: this round's next plan (`decide` has seen to the
    /// treasury, the cities' work and their walls). `staying`: who stays
    /// home; `short`: its cities short of defenders.
    pub(super) fn round(
        &mut self,
        game: &Game,
        (side, sight): (Side, &Sight),
        (staying, short): (&[UnitId], &[Hex]),
    ) -> Order {
        let foes = self.spot(game, side);
        self.plan(game, (side, sight), (staying, short), &foes)
    }

    fn plan(
        &mut self,
        game: &Game,
        (side, sight): (Side, &Sight),
        (staying, short): (&[UnitId], &[Hex]),
        foes: &[Foe],
    ) -> Order {
        let map = &game.map;
        let style = self.style;
        let free: Vec<&Unit> = map
            .units()
            .of_side(side)
            .filter(|u| map.contains(u.pos()) && u.movement > 0)
            .filter(|u| !self.done.contains(&u.id()) && !staying.contains(&u.id()))
            .collect();
        if free.is_empty() {
            return Order::EndTurn;
        }
        let ranges: Vec<(UnitId, MoveRange)> = free.iter().map(|u| (u.id(), game.known_range(u, sight))).collect();
        let cost = |id: UnitId, hex: Hex| ranges.iter().find(|(u, _)| *u == id).and_then(|(_, r)| r.cost(hex));
        let reaches = |id: UnitId, hex: Hex| cost(id, hex).is_some();
        // When a unit of its own gets to a hex, going there by itself.
        let when = |unit: &Unit, hex: Hex| cost(unit.id(), hex).map(|c| u64::from(c) * ROUND / u64::from(unit.speed()));

        // A threatened city first: free units that can get in this round do.
        for city in short.iter().filter(|c| !self.is_blocked(**c, game)) {
            let coming: Vec<UnitId> = free
                .iter()
                .filter(|u| u.pos() != *city && reaches(u.id(), *city) && !map.is_commander(u.id()))
                .map(|u| u.id())
                .take(MAX_STACK.saturating_sub(map.units_at(*city).len()))
                .collect();
            if !coming.is_empty() {
                self.done.extend(coming.iter().copied());
                return Order::Move { units: coming, to: *city };
            }
        }

        // Attacks: the cities most worth taking, the other sides' stacks
        // near its cities, then stacks in the field near its units.
        let mut targets: Vec<(Hex, u32)> = Vec::new();
        for unit in map.units().iter().filter(|u| u.side() != side && map.contains(u.pos())) {
            let near_home = map
                .cities()
                .iter()
                .any(|c| c.side == side && c.pos.distance(unit.pos()) <= unit.base_move_points / 2 + 1);
            let near_free = free.iter().any(|u| u.pos().distance(unit.pos()) <= 4 || reaches(u.id(), unit.pos()));
            if (near_home || near_free) && map.city_at(unit.pos()).is_none() {
                // (One near its cities is likely on its way to them: below
                // the cities it could take, where in turns it comes first.)
                targets.push((unit.pos(), if near_home { 90 } else { 50 }));
            }
        }
        for city in map.cities().iter().filter(|c| c.side != side && sight.knows(c.pos)) {
            targets.push((city.pos, 100 + city.income * 4));
        }
        targets.sort_by_key(|(h, value)| (std::cmp::Reverse(*value), h.row, h.col));
        targets.dedup_by_key(|(h, _)| *h);
        // (A city it cannot see may hold anyone: it goes to look first.)
        targets.retain(|(h, _)| sight.sees(*h) && !self.is_blocked(*h, game));
        let (dare, wear, _) = daring(&style, game);
        for (target, _) in &targets {
            let able: Vec<&Unit> = free.iter().copied().filter(|u| reaches(u.id(), *target)).collect();
            if able.is_empty() {
                continue;
            }
            let city = map.city_at(*target);
            // Those of one hex, the hex that gets them there first; then all
            // of them, if the game makes one stack of them.
            let mut origins: Vec<Hex> = able.iter().map(|u| u.pos()).collect();
            origins.sort_by_key(|h| (h.row, h.col));
            origins.dedup();
            let arrival = |ids: &[UnitId]| {
                let units = ids.iter().filter_map(|id| map.units().get(*id));
                units.filter_map(|u| when(u, *target)).max().unwrap_or(0)
            };
            let mut options: Vec<(Vec<UnitId>, u64)> = origins
                .iter()
                .map(|origin| {
                    let here: Vec<UnitId> = able.iter().filter(|u| u.pos() == *origin).map(|u| u.id()).collect();
                    let stack = strongest_stack(game, here, *target);
                    let at = arrival(&stack);
                    (stack, at)
                })
                .collect();
            options.sort_by_key(|(_, at)| *at);
            if origins.len() > 1 {
                let all = strongest_stack(game, able.iter().map(|u| u.id()).collect(), *target);
                if game.goes_as_one(side, &all, *target) {
                    options.push((all, u64::MAX));
                }
            }
            let need = if city.is_some() { dare } else { (dare + FIELD_ODDS).min(95) };
            let odds_of = |stack: &[UnitId]| game.battle_forecast(stack, *target).map_or(100, |f| f.odds);
            // One stack that is enough, the first there.
            let enough = options.iter().find(|(stack, _)| !stack.is_empty() && odds_of(stack) >= need);
            if let Some((stack, _)) = enough {
                self.done.extend(stack.iter().copied());
                return Order::Move { units: stack.clone(), to: *target };
            }
            // A city they could take together, from hexes the game cannot
            // make one stack of this round: they gather on the one of
            // their hexes nearest to it, to attack as one the next (the
            // owner's word: "order one stack to another then combine them").
            if city.is_some() && origins.len() > 1 {
                let all = strongest_stack(game, able.iter().map(|u| u.id()).collect(), *target);
                let odds = game.battle_forecast(&all, *target).map_or(100, |f| f.odds);
                let mut hexes = origins.clone();
                hexes.sort_by_key(|h| (h.distance(*target), h.row, h.col));
                let gather = hexes.into_iter().find_map(|at| {
                    let there: Vec<UnitId> =
                        all.iter().copied().filter(|id| map.units().get(*id).is_some_and(|u| u.pos() == at)).collect();
                    let coming: Vec<UnitId> =
                        all.iter().copied().filter(|id| !there.contains(id) && reaches(*id, at)).collect();
                    (!coming.is_empty() && room(game, side, &coming, at)).then_some((there, coming, at))
                });
                if let (true, Some((there, coming, at))) = (odds >= dare, gather) {
                    self.done.extend(there);
                    self.done.extend(coming.iter().copied());
                    return Order::Move { units: coming, to: at };
                }
            }
            // Else, at a city's walls: a wave when more can follow, or a
            // siege (as in turns), by the first there.
            if let Some((stack, _)) = options.first().filter(|(stack, _)| city.is_some() && !stack.is_empty()) {
                let forecast = game.battle_forecast(stack, *target);
                let more = able.len() > stack.len();
                if (more && forecast.map_or(100, |f| f.odds) >= wear)
                    || crate::agents::siege(game, side, *target, forecast)
                {
                    self.done.extend(stack.iter().copied());
                    return Order::Move { units: stack.clone(), to: *target };
                }
            }
        }

        // The rest: a stack at a time.
        let temples: Vec<Hex> = (0..map.rows())
            .flat_map(|row| (0..map.cols()).map(move |col| Hex::new(row, col)))
            .filter(|h| map.tile(*h).is_some_and(|t| t.kind == TileType::Temple))
            .collect();
        let hub = self.hub.filter(|h| !self.is_blocked(*h, game));
        // With total fog: the edge of the land it knows; and whether it knows
        // of any city to go for (if not, every stack goes to see more).
        let frontier = sight.frontier(map);
        let lost = !frontier.is_empty()
            && !map.cities().iter().any(|c| c.side != side && sight.knows(c.pos) && !self.is_blocked(c.pos, game));
        // While there is land it has not seen, one unit goes on looking: its
        // fastest that is free and no commander (another when it is lost).
        // It goes where it goes: it is there to see.
        if !frontier.is_empty() && !lost {
            if self.scout.is_none_or(|id| map.units().get(id).is_none()) {
                let fastest = free.iter().filter(|u| !map.is_commander(u.id())).max_by_key(|u| u.base_move_points);
                self.scout = fastest.map(|u| u.id());
            }
            if let Some(scout) = self.scout.and_then(|id| free.iter().copied().find(|u| u.id() == id)) {
                self.done.push(scout.id());
                if let Some(order) = self.explore(game, (side, sight), (&[scout.id()], scout), &frontier) {
                    return order;
                }
            }
        }
        let mut seen_hexes: Vec<Hex> = Vec::new();
        for unit in &free {
            let here = unit.pos();
            if seen_hexes.contains(&here) {
                continue;
            }
            seen_hexes.push(here);
            let stack: Vec<&Unit> = free.iter().copied().filter(|u| u.pos() == here).collect();
            let ids: Vec<UnitId> = stack.iter().map(|u| u.id()).collect();
            self.done.extend(ids.iter().copied());
            let all_reach = |hex: Hex| ids.iter().all(|id| reaches(*id, hex));
            let wary = |planner: &Planner, order: Order| planner.wary(game, (side, sight), foes, order);
            // A temple this round that has a blessing for half of them or
            // more: they go (a plan that ends on a temple seeks it). (For
            // one new unit an army went back to its temple, again and
            // again.)
            let fits = |t: Hex| {
                let there = map.units_at(t);
                there.iter().filter_map(|id| map.units().get(*id)).all(|u| u.side() == side)
                    && there.len() + ids.len() <= MAX_STACK
            };
            let unblessed = |t: Hex| stack.iter().filter(|u| !u.blessed_at(t)).count();
            let temple = temples.iter().copied().find(|t| {
                *t != here
                    && unblessed(*t) > 0
                    && 2 * unblessed(*t) >= stack.len()
                    && all_reach(*t)
                    && fits(*t)
                    && !self.is_blocked(*t, game)
            });
            if let Some(temple) = temple {
                let order = Order::Move { units: ids.clone(), to: temple };
                if let Some(order) = wary(self, order.clone()).filter(|wary| *wary == order) {
                    return order;
                }
            }
            let lead = stack.iter().copied().min_by_key(|u| u.movement).unwrap_or(unit);
            if lost {
                match self.explore(game, (side, sight), (&ids, lead), &frontier) {
                    Some(order) => return order,
                    None => continue,
                }
            }
            // A small stack joins the army first, if the army is near; the
            // army's own, while small, waits for them. (Not for a city of
            // nobody's it sees and is likely to take as it is.)
            let target = self.objective(game, (side, sight), here, lead, ids.len());
            let takes = |t: Hex| {
                map.city_at(t).is_some_and(|c| c.side == Side::Grey)
                    && (map.units_at(t).is_empty() || game.battle_forecast(&ids, t).is_some_and(|f| f.odds >= dare))
            };
            let small = ids.len() < style.muster && !target.is_some_and(|t| sight.sees(t) && takes(t));
            if small && hub == Some(here) {
                continue;
            }
            if let Some(hub) = hub.filter(|h| *h != here && small && h.distance(here) <= 12) {
                let space = map.units_at(hub).len() + ids.len() <= MAX_STACK + 1;
                let blocked = |h: Hex| self.is_blocked(h, game);
                let order = space.then(|| advance(game, (side, sight), (&ids, lead), hub, &blocked)).flatten();
                if let Some(order) = order.and_then(|order| wary(self, order)) {
                    return order;
                }
            }
            // Too few to go alone: in one of its cities, they wait for more.
            if small && map.city_at(here).is_some_and(|c| c.side == side) {
                continue;
            }
            let Some(target) = target else {
                // Nothing it knows of to go for: it goes to see more.
                match self.explore(game, (side, sight), (&ids, lead), &frontier) {
                    Some(order) => return order,
                    None => continue,
                }
            };
            // A city it cannot see: near enough, it goes where it can see
            // it (no nearer), to know what holds it.
            if !sight.sees(target) {
                let look = (0..map.rows())
                    .flat_map(|row| (0..map.cols()).map(move |col| Hex::new(row, col)))
                    .filter(|h| *h != target && h.distance(target) <= crate::game::UNIT_SIGHT)
                    .filter(|h| all_reach(*h) && room(game, side, &ids, *h) && !self.is_blocked(*h, game))
                    .min_by_key(|h| (h.distance(here), h.row, h.col));
                if let Some(look) = look {
                    match wary(self, Order::Move { units: ids.clone(), to: look }) {
                        Some(order) => return order,
                        None => continue,
                    }
                }
                if all_reach(target) {
                    continue;
                }
            } else if all_reach(target) {
                // Near enough to attack, but not strong enough: it waits
                // there for more, out of the way.
                continue;
            }
            let blocked = |h: Hex| self.is_blocked(h, game);
            let order = advance(game, (side, sight), (&ids, lead), target, &blocked);
            if let Some(order) = order.and_then(|order| wary(self, order)) {
                return order;
            }
        }
        Order::EndTurn
    }
}

#[cfg(test)]
mod tests {
    use super::super::wary::{foes, menace};
    use super::super::{STEADY, STYLES};
    use super::*;
    use crate::agents::Agent;
    use crate::setup::{Mode, Setup};
    use crate::state::combat::System;
    use crate::state::scenario::{self, Scenario};

    const REDHOLD: Hex = Hex::new(15, 5);
    const MIDWAY: Hex = Hex::new(15, 15);

    /// 30 x 30 grass with a temple at (10, 5): red's Redhold in the west,
    /// blue's Bluemoor in the east, nobody's Midway between them; red the
    /// computer's, in rounds (or in turns), battles by strength with no
    /// chance in them. Soldiers and levies go 6 hexes a round, riders 10.
    fn world(mode: Mode) -> Game {
        let mut rows = vec!["g".repeat(30); 30];
        rows[15] = format!("{}c{}c{}c{}", "g".repeat(5), "g".repeat(9), "g".repeat(11), "g".repeat(2));
        rows[10] = format!("{}t{}", "g".repeat(5), "g".repeat(24));
        let terrain = format!("30\n30\n{}\n", rows.join("\n"));
        let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nenter grass 2, city 1\n\
                     unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n\
                     unit riders\nname Riders\nstrength 3\nmoves 20\nenter grass 2, city 1\n";
        let cities =
            "city 15 5\nname Redhold\nside red\ncity 15 15\nname Midway\ncity 15 27\nname Bluemoor\nside blue\n";
        let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
        let game = Game::from_scenario(Scenario { map, assets: None, warnings });
        let mut setup = Setup::solo(&game.map, None, "passive");
        setup.set_players("red=steady,blue=passive").unwrap();
        (setup.mode, setup.combat.system, setup.combat.chance) = (mode, System::Strength, 0);
        let mut game = game.with_setup(setup);
        // (Its capital's own: they stay.)
        put(&mut game, Side::Red, "soldiers", REDHOLD, 2);
        game
    }

    fn put(game: &mut Game, side: Side, kind: &str, hex: Hex, n: usize) -> Vec<UnitId> {
        let kind = game.map.units().kind_named(kind).unwrap();
        (0..n)
            .map(|_| {
                let id = game.map.new_unit_of(side, kind).unwrap();
                game.map.place_unit(id, hex).unwrap();
                id
            })
            .collect()
    }

    /// `n` more of red's soldiers in Redhold; the temple has blessed them,
    /// and those that were there.
    fn blessed(game: &mut Game, n: usize) -> Vec<UnitId> {
        let army = put(game, Side::Red, "soldiers", REDHOLD, n);
        for id in game.map.units_at(REDHOLD).to_vec() {
            game.map.unit_mut(id).unwrap().blessings.push((Hex::new(10, 5), crate::state::Blessing::Strength));
        }
        army
    }

    /// The orders `planner` gives red this round (carried out as it gives
    /// them), but for what its cities build.
    fn orders(game: &mut Game, planner: &mut Planner) -> Vec<Order> {
        let mut given = Vec::new();
        for _ in 0..80 {
            let order = planner.next_order(&game.view_for(Side::Red), Side::Red);
            if order == Order::EndTurn {
                break;
            }
            if let Err(why) = game.apply(order.clone()) {
                planner.refused(&order, &why);
            }
            if !matches!(order, Order::Build { .. }) {
                given.push(order);
            }
        }
        given
    }

    /// Where the plan of `unit` ends, if it has one.
    fn bound(game: &Game, unit: UnitId) -> Option<Hex> {
        game.map.units().get(unit).filter(|u| u.planned).and_then(|u| u.route.last().copied())
    }

    /// An army on its way to a city went to the end of its movement, into
    /// the reach of a stack twice its strength, to be run into there (in
    /// turns: to be attacked there on the other's turn). It stops where
    /// that stack cannot come.
    #[test]
    fn an_army_stops_short_of_where_a_stack_it_fears_could_meet_it() {
        let foe = Hex::new(15, 17);
        // How near the foe red's army ends, with `foes` of blue's `kind`
        // there: where its plans end (rounds), or where it stands (turns).
        let ends = |mode: Mode, kind: &str, foes: usize| {
            let mut game = world(mode);
            // (Three in the capital besides its own, free to go for
            // Midway, which nobody holds; the temple has blessed them.)
            blessed(&mut game, 3);
            put(&mut game, Side::Blue, kind, foe, foes);
            // (Whichever three of the five it sends.)
            let all = game.map.units_at(REDHOLD).to_vec();
            let given = orders(&mut game, &mut Planner::new(STEADY));
            let ends: Vec<Hex> = match mode {
                Mode::Rounds => all.iter().filter_map(|id| bound(&game, *id)).collect(),
                Mode::Turns => {
                    let at = |id: &UnitId| game.map.units().get(*id).map(|u| u.pos());
                    all.iter().filter_map(at).filter(|hex| *hex != REDHOLD).collect()
                }
            };
            assert_eq!(ends.len(), 3, "three set out: {given:?}");
            ends.into_iter().map(|hex| hex.distance(foe)).min().unwrap()
        };
        for mode in [Mode::Rounds, Mode::Turns] {
            // Blue's six go seven hexes with what they may have kept.
            assert!(ends(mode, "soldiers", 6) > 7, "{mode:?}: short of their reach");
            // A stack it does not fear does not stop it.
            assert!(ends(mode, "levy", 1) <= 7, "{mode:?}: on, to the end of its movement");
        }
    }

    /// Two stacks either side of a city that together take it, and cannot
    /// be one stack this round: as in turns it sent all four against it
    /// in one order (in rounds two attacks, each lost). In rounds they join
    /// first, and attack as one the round after.
    #[test]
    fn stacks_that_cannot_attack_as_one_this_round_gather_first() {
        let (west, east) = (Hex::new(15, 13), Hex::new(15, 18));
        let set = |game: &mut Game| {
            put(game, Side::Grey, "soldiers", MIDWAY, 2);
            (put(game, Side::Red, "soldiers", west, 2), put(game, Side::Red, "soldiers", east, 2))
        };
        // In turns' way: one order for the four, which the game cannot
        // make one stack of.
        let mut game = world(Mode::Rounds);
        let (a, b) = set(&mut game);
        let all: Vec<UnitId> = a.iter().chain(&b).copied().collect();
        assert!(!game.goes_as_one(Side::Red, &all, MIDWAY));
        assert_eq!(game.battle_odds(&all, MIDWAY), Some(100));
        assert_eq!(game.battle_odds(&a, MIDWAY), Some(0));
        let given = orders(&mut game, &mut Planner::turnwise(STEADY));
        assert!(given.contains(&Order::Move { units: all.clone(), to: MIDWAY }), "{given:?}");
        // In rounds' way: the eastern stack joins the western one, the
        // nearer to the city, and nobody attacks yet.
        let mut game = world(Mode::Rounds);
        let (a, b) = set(&mut game);
        let mut planner = Planner::new(STEADY);
        let given = orders(&mut game, &mut planner);
        assert_eq!(given, [Order::Move { units: b.clone(), to: west }], "{given:?}");
        assert!(a.iter().all(|id| bound(&game, *id).is_none()), "those they join wait for them");
        // The round plays out; the next, the four attack as one, and take
        // Midway.
        game.apply(Order::EndTurn).unwrap();
        game.apply(Order::EndTurn).unwrap();
        assert!(a.iter().chain(&b).all(|id| game.map.units().get(*id).is_some_and(|u| u.pos() == west)));
        let given = orders(&mut game, &mut planner);
        let attack = given.iter().find_map(|order| match order {
            Order::Move { units, to } if *to == MIDWAY => Some(units.len()),
            _ => None,
        });
        assert_eq!(attack, Some(4), "{given:?}");
        game.apply(Order::EndTurn).unwrap();
        game.apply(Order::EndTurn).unwrap();
        assert_eq!(game.map.city_at(MIDWAY).map(|c| c.side), Some(Side::Red));
    }

    /// Units of two hexes that the game makes one stack of (the farther
    /// can come by the nearer and on) are sent in one order, and those of
    /// one hex that are enough go by themselves.
    #[test]
    fn an_attack_is_one_stack() {
        let mut game = world(Mode::Rounds);
        put(&mut game, Side::Grey, "soldiers", MIDWAY, 2);
        let near = put(&mut game, Side::Red, "soldiers", Hex::new(15, 13), 2);
        let far = put(&mut game, Side::Red, "soldiers", Hex::new(15, 11), 2);
        let all: Vec<UnitId> = near.iter().chain(&far).copied().collect();
        assert!(game.goes_as_one(Side::Red, &all, MIDWAY));
        let given = orders(&mut game, &mut Planner::new(STEADY));
        assert_eq!(given, [Order::Move { units: all, to: MIDWAY }]);
        // Enough on one hex: they go, and the others are not held up for it.
        let mut game = world(Mode::Rounds);
        put(&mut game, Side::Grey, "levy", MIDWAY, 1);
        let near = put(&mut game, Side::Red, "soldiers", Hex::new(15, 13), 2);
        put(&mut game, Side::Red, "soldiers", Hex::new(15, 11), 2);
        let given = orders(&mut game, &mut Planner::new(STEADY));
        assert_eq!(given.first(), Some(&Order::Move { units: near, to: MIDWAY }), "{given:?}");
    }

    /// An army went back to its temple for every new unit that had no
    /// blessing of it yet. It goes when half of it has one to get.
    #[test]
    fn an_army_goes_to_a_temple_only_for_half_of_it() {
        let (temple, camp) = (Hex::new(10, 5), Hex::new(10, 8));
        let to_temple = |mode: Mode, blessed: usize| {
            let mut game = world(mode);
            let army = put(&mut game, Side::Red, "soldiers", camp, 4);
            for id in &army[..blessed] {
                game.map.unit_mut(*id).unwrap().blessings.push((temple, crate::state::Blessing::Strength));
            }
            let given = orders(&mut game, &mut Planner::new(STEADY));
            given.iter().any(|order| matches!(order, Order::Move { to, .. } if *to == temple))
        };
        for mode in [Mode::Rounds, Mode::Turns] {
            assert!(to_temple(mode, 0) && to_temple(mode, 2), "{mode:?}: half of it or more: it goes");
            assert!(!to_temple(mode, 3), "{mode:?}: for one of four it does not");
        }
    }

    /// It marches with three (the careful with four), and its treasury
    /// pays for more than its taxes keep: in rounds first, and in turns
    /// too since that won there as well.
    #[test]
    fn it_sets_out_with_few_and_spends_its_gold() {
        assert_eq!(STYLES.map(|style| style.muster), [3, 3, 4]);
        for mode in [Mode::Rounds, Mode::Turns] {
            let game = world(mode);
            let gold = i64::from(game.gold(Side::Red));
            assert!(gold >= 24, "{gold}");
            assert_eq!(Planner::new(STEADY).reserve(&game, Side::Red), gold / 12, "{mode:?}");
            assert_eq!(Planner::turnwise(STEADY).reserve(&game, Side::Red), gold / 12);
        }
        // The agents' names: a planner, and a planner that plays rounds as
        // turns.
        assert!(crate::agents::by_name("steady/turns").is_ok() && crate::agents::by_name("bold/turns").is_ok());
        assert!(crate::agents::by_name("simple/turns").is_err() && crate::agents::by_name("steady/rounds").is_err());
    }

    /// Where a stack could be, and when: twice as far for riders, and a
    /// hex three steps off is theirs in three tenths of a round, the
    /// soldiers' in half of one.
    #[test]
    fn a_foe_could_be_where_its_movement_takes_it_when_its_speed_gets_it_there() {
        let mut game = world(Mode::Rounds);
        put(&mut game, Side::Blue, "riders", Hex::new(5, 20), 1);
        put(&mut game, Side::Blue, "soldiers", Hex::new(25, 20), 2);
        put(&mut game, Side::Grey, "soldiers", MIDWAY, 1);
        let foes = foes(&game, Side::Red);
        assert_eq!(foes.len(), 2, "blue's two stacks: nobody's garrison stays where it is");
        let (riders, soldiers) = (&foes[0], &foes[1]);
        assert_eq!(riders.there_by(Hex::new(5, 17)), Some(6 * ROUND / 20));
        assert_eq!(soldiers.there_by(Hex::new(25, 17)), Some(6 * ROUND / 12));
        assert_eq!((riders.there_by(Hex::new(5, 20)), riders.there_by(Hex::new(5, 5))), (Some(0), None));
        // (With the two points a unit may have kept.)
        assert!(riders.there_by(Hex::new(5, 9)).is_some() && riders.there_by(Hex::new(5, 8)).is_none());
        assert!(soldiers.there_by(Hex::new(25, 13)).is_some() && soldiers.there_by(Hex::new(25, 12)).is_none());
        // What could be on a hex is the strongest of them that could, by
        // then.
        let near = Hex::new(25, 17);
        assert!(menace(&foes, near, None) > 0.0 && menace(&foes, near, Some(ROUND / 4)) == 0.0);
        assert_eq!(menace(&foes, Hex::new(15, 5), None), 0.0);
    }
}
