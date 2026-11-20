//! The computer's players that plan ahead: one agent, in a few styles
//! (`steady`, `bold`, `careful`). Each turn, order by order, it:
//!
//! 1. keeps its treasury in hand: what its cities build is what it can keep
//!    (their upkeep within its taxes) and what is worth most *under the rules
//!    of this game* (with little chance in battle a strong unit is worth far
//!    more than a weak one; with much, or with stacks that fight as one, the
//!    numbers count), and it keeps a commander;
//! 2. defends: a city with other sides' units near enough to strike keeps
//!    defenders enough for the threat, and free units near it come in;
//! 3. strikes where the odds are good enough for its style (the odds are the
//!    game's own, so the rules in effect are in them), waves at walls when
//!    more follow, and stacks it can beat in the field;
//! 4. gathers the rest into armies around its commander, blessed at the
//!    temples on the way, and sends each toward the city most worth taking
//!    for the trouble.
//!
//! It is given its side's view of the game (`Game::view_for`) and knows no
//! more than a person would: with fog of war, a city it cannot see is one it
//! goes to look at before it attacks (what it remembers of its garrison
//! counts), the other sides' stacks it saw near its cities lately are a
//! threat to them still, and a place the game refuses to send it (something
//! unseen in the way) it leaves be for a while. With total fog, it goes by
//! the land it has seen, and when it knows of no city to go for, it goes to
//! see more.
//!
//! Not a port: the C++ had no players.

use crate::agents::Agent;
use crate::game::{Game, Sight};
use crate::orders::Order;
use crate::state::combat::System;
use crate::state::{CityId, Hex, MAX_STACK, Side, TileType, Unit, UnitId, UnitProd, compute_range, hostile};

/// What makes one style of play another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Style {
    pub name: &'static str,
    /// Units that stay in each of its cities, at the least.
    pub keep: usize,
    /// The odds, in percent, it attacks at; and those it starts a wave at
    /// when more than a stack can follow.
    pub dare: u32,
    pub wear: u32,
    /// How much of a threat it answers with defenders, in percent of the
    /// threat's strength.
    pub guard: u32,
    /// Units it gathers before it marches on a city it cannot see, or that is
    /// held.
    pub muster: usize,
    /// What its cities' work weighs, in percent: speed (a unit's moves), and
    /// price (against what it is worth).
    pub speed: u32,
    pub thrift: u32,
}

/// Balanced: armies around a commander, cities defended as they are
/// threatened, attacks at fair odds.
pub const STEADY: Style =
    Style { name: "steady", keep: 1, dare: 60, wear: 25, guard: 100, muster: 4, speed: 30, thrift: 100 };
/// Fast and many: grabs the empty and weak cities first, few defenders,
/// attacks at lower odds.
pub const BOLD: Style =
    Style { name: "bold", keep: 1, dare: 55, wear: 18, guard: 70, muster: 3, speed: 20, thrift: 140 };
/// Slow and sure: strong defenders, big armies, high odds.
pub const CAREFUL: Style =
    Style { name: "careful", keep: 1, dare: 68, wear: 30, guard: 150, muster: 5, speed: 10, thrift: 70 };

pub const STYLES: [Style; 3] = [STEADY, BOLD, CAREFUL];

/// The style of a name.
pub fn style(name: &str) -> Option<Style> {
    STYLES.into_iter().find(|s| s.name == name)
}

/// What threatens each of a side's cities (see [`threat`]).
type Threats = Vec<(CityId, f64)>;

/// A player of a [`Style`].
pub struct Planner {
    style: Style,
    /// The turn it is playing (round and side), and the units it has given an
    /// order this turn (or kept back).
    turn: Option<(u32, Side)>,
    done: Vec<UnitId>,
    /// What threatens each of its cities, worked out once a turn.
    threats: Option<((u32, Side), Threats)>,
    /// Where its armies gather this turn: its commander's hex at the start
    /// of the turn (or its biggest stack's).
    hub: Option<Hex>,
    /// Hexes the game refused to send it to (something it could not see is
    /// in the way, likely), and the round until which it leaves them be.
    blocked: Vec<(Hex, u32)>,
    /// The orders refused this turn.
    refusals: usize,
    /// Where its stacks went to see more of the map this turn (total fog);
    /// and the unit that goes on seeing more while there is land it has not
    /// seen, its scout.
    scouted: Vec<Hex>,
    scout: Option<UnitId>,
    /// How many steps each unit has taken this turn to see more: a few, while
    /// it has the moves (the edge of what it knows is near).
    explored: Vec<(UnitId, u32)>,
}

impl Planner {
    pub fn new(style: Style) -> Planner {
        Planner {
            style,
            turn: None,
            done: Vec::new(),
            threats: None,
            hub: None,
            blocked: Vec::new(),
            refusals: 0,
            scouted: Vec::new(),
            scout: None,
            explored: Vec::new(),
        }
    }

    /// True while it leaves `hex` be (the game refused to send it there).
    fn is_blocked(&self, hex: Hex, game: &Game) -> bool {
        self.blocked.iter().any(|(h, until)| *h == hex && game.turn() <= *until)
    }
}

/// How many rounds it leaves a hex be after the game refused to send it
/// there.
const BLOCKED_ROUNDS: u32 = 3;
/// How many refused orders a turn it tries again with other orders for the
/// same units.
const MAX_REFUSALS: usize = 4;
/// How many rounds a stack it saw near one of its cities, now out of its
/// sight, is still a threat to it.
const REMEMBERED_ROUNDS: u32 = 2;
/// How far apart, at the least, the places are its stacks go to see more of
/// the map from (total fog), and how many of the nearest it tries.
const SCOUT_SPREAD: u32 = 4;
const SCOUT_TRIES: usize = 6;
/// How many steps a turn a stack takes to see more, at the most.
const SCOUT_STEPS: u32 = 3;

/// How much a unit is worth in battle under the rules in effect, against
/// one of strength 1: in lines a unit fights its duels alone, and its odds go
/// as its strength to the power the chance gives (`combat`: `k = 2 (10 -
/// chance) / 5`); in strength against strength, a stack's strengths add up.
///
/// (In plain arithmetic, with no `powf`: `k` is a fifth of a whole number,
/// so `s^k` is the fifth root of `s` to that number; the same on every
/// machine, so a seed plays the same game everywhere.)
pub fn worth(strength: u32, game: &Game) -> f64 {
    let rules = game.setup().combat;
    // k = m / 5.
    let m = 2 * u32::from(10u8.saturating_sub(rules.chance));
    let s = f64::from(strength.max(1));
    match rules.system {
        System::Lines => fifth_root(power(s, m.max(2))),
        System::Strength => s * (1.0 + f64::from(m) / 20.0),
    }
}

/// `x` to the `n`th, by multiplying.
fn power(x: f64, n: u32) -> f64 {
    (0..n).fold(1.0, |p, _| p * x)
}

/// The fifth root of `x` (positive), by Newton's steps from a start above it.
fn fifth_root(x: f64) -> f64 {
    let mut r = x.max(1.0);
    for _ in 0..200 {
        let next = (4.0 * r + x / power(r, 4)) / 5.0;
        if (r - next).abs() <= 1e-12 * r {
            return next;
        }
        r = next;
    }
    r
}

/// The odds a style attacks at, and starts a wave at, under the rules in
/// effect: the more chance in battle (or with stacks that fight as one), the
/// nearer the odds of any battle are to even, and the less it waits for odds
/// it will seldom get.
pub fn daring(style: &Style, game: &Game) -> (u32, u32, u32) {
    let rules = game.setup().combat;
    let luck = u32::from(rules.chance.saturating_sub(5));
    // Played out over many games (the tournaments): with much chance it pays
    // to wait for better odds, and to keep fewer at home (walls and strength
    // do little for defenders then).
    // And a long game makes it bolder: two that wait for each other would
    // wait for ever.
    let late = game.turn().saturating_sub(40) / 4;
    let dare = (style.dare + luck * 3).saturating_sub(late).clamp(45, 90);
    let guard = style.guard * (10 - luck * 3 / 2).max(3) / 10;
    (dare, style.wear, guard)
}

/// What a city's work is worth to `style`: the unit's worth in battle, a
/// little more for its speed, for the turns and the gold it takes.
fn production_score(prod: &UnitProd, game: &Game, style: &Style) -> f64 {
    let speed = 1.0 + f64::from(style.speed) / 100.0 * f64::from(prod.movement) / 10.0;
    let price = f64::from(prod.cost) + 8.0 * f64::from(prod.upkeep);
    let turns = f64::from(prod.time.max(1));
    worth(prod.strength, game) * speed / (turns * 6.0 + price * f64::from(style.thrift) / 100.0)
}

/// Of these units, as many as attack together (a hex's worth: the strongest
/// [`MAX_STACK`] and a commander), the strongest first.
fn strongest_stack(game: &Game, mut units: Vec<UnitId>) -> Vec<UnitId> {
    let map = &game.map;
    units.sort_by_key(|id| std::cmp::Reverse(map.units().get(*id).map_or(0, |u| u.power())));
    let (mut soldiers, mut leader) = (0, false);
    units.retain(|id| {
        if map.is_commander(*id) {
            !std::mem::replace(&mut leader, true)
        } else {
            soldiers += 1;
            soldiers <= MAX_STACK
        }
    });
    units
}

/// What the other sides' units that could strike `hex` next turn are worth
/// in battle (see [`worth`]): those that can get there with a whole turn's
/// moves, roads and all; and, with fog of war, the stacks it saw near it in
/// the last rounds that are out of its sight now (what it remembers of them:
/// how many, as strong as a unit of the map's is on the whole), if they were
/// within a turn's march.
fn threat(game: &Game, side: Side, hex: Hex, sight: &Sight) -> f64 {
    let map = &game.map;
    let kinds: Vec<_> = map.units().kinds().iter().filter(|k| k.commander == 0).collect();
    let typical = kinds.iter().map(|k| k.strength).sum::<u32>() / (kinds.len() as u32).max(1);
    let march = kinds.iter().map(|k| k.moves).max().unwrap_or(0) / 2;
    let remembered: f64 = sight
        .ghosts()
        .iter()
        .filter(|g| !sight.sees(g.at) && g.side != Side::Grey && g.side != side)
        .filter(|g| g.round + REMEMBERED_ROUNDS >= game.turn() && g.at.distance(hex) <= march.max(2))
        .map(|g| g.count as f64 * worth(typical, game))
        .sum();
    remembered
        + map
            .units()
            .iter()
            .filter(|u| u.side() != side && u.side() != Side::Grey && map.contains(u.pos()))
            .filter(|u| u.pos().distance(hex) <= u.base_move_points)
            .filter(|u| {
                let mut fresh = (*u).clone();
                fresh.movement = fresh.base_move_points;
                compute_range(map, &fresh).cost(hex).is_some()
            })
            .map(|u| worth(u.power(), game))
            .sum::<f64>()
}

/// This turn's step of the stack `ids` toward `to`, as its side knows the
/// map: as far as the slowest (`lead`) gets on its cheapest way, where there
/// is room for them, and never into another side's units (an attack is
/// decided on its own odds): a move of those of them that can get there.
/// Worked out on what it sees, not left to the game (a way through the fog
/// that is shut shows itself as it goes).
fn advance(
    game: &Game,
    (side, sight): (Side, &Sight),
    (ids, lead): (&[UnitId], &Unit),
    to: Hex,
    blocked: &dyn Fn(Hex) -> bool,
) -> Option<Order> {
    let map = &game.map;
    let room = |hex: Hex| {
        let there = map.units_at(hex);
        !blocked(hex) && (there.iter().all(|id| ids.contains(id)) || there.len() + ids.len() <= MAX_STACK)
    };
    let (step, _) = game.known_step_toward(lead, to, &room, sight)?;
    if step == lead.pos() || hostile(map, side, step) {
        return None;
    }
    let coming: Vec<UnitId> = ids
        .iter()
        .copied()
        .filter(|id| map.units().get(*id).is_some_and(|u| game.known_range(u, sight).cost(step).is_some()))
        .collect();
    (!coming.is_empty()).then_some(Order::Move { units: coming, to: step })
}

/// The side's capital: where it builds its commanders (or its first city).
fn capital(game: &Game, side: Side) -> Option<CityId> {
    let map = &game.map;
    let units = map.units();
    let own = || map.cities().iter().filter(|c| c.side == side);
    let leads = |c: &&crate::state::City| {
        c.units
            .iter()
            .any(|p| units.kind_named(&p.basename).and_then(|k| units.kind(k)).is_some_and(|k| k.commander > 0))
    };
    own().find(leads).or_else(|| own().next()).map(|c| c.id)
}

impl Agent for Planner {
    fn refused(&mut self, order: &Order, _why: &str) {
        // It leaves the place be for a while; the units may do something
        // else this turn.
        let Some(round) = self.turn.map(|(round, _)| round) else { return };
        if let Order::Move { units, to } | Order::Toward { units, to } = order {
            self.blocked.push((*to, round + BLOCKED_ROUNDS));
            // A few times a turn: then they stay where they are till the next.
            self.refusals += 1;
            if self.refusals <= MAX_REFUSALS {
                self.done.retain(|id| !units.contains(id));
            }
        }
    }

    fn next_order(&mut self, game: &Game, side: Side) -> Order {
        self.decide(game, side)
    }
}

impl Planner {
    fn decide(&mut self, game: &Game, side: Side) -> Order {
        if self.turn != Some((game.turn(), side)) {
            self.turn = Some((game.turn(), side));
            self.done.clear();
            self.refusals = 0;
            self.scouted.clear();
            self.explored.clear();
            self.blocked.retain(|(_, until)| game.turn() <= *until);
            // Where the armies gather: its commander, else its biggest stack
            // out of its cities.
            let map = &game.map;
            let mine = || map.units().of_side(side).filter(|u| map.contains(u.pos()));
            self.hub = mine().find(|u| map.is_commander(u.id())).map(|u| u.pos()).or_else(|| {
                mine()
                    .filter(|u| map.city_at(u.pos()).is_none())
                    .max_by_key(|u| (map.units_at(u.pos()).len(), u.pos().row, u.pos().col))
                    .map(|u| u.pos())
            });
        }
        if let Some(order) = self.economy(game, side) {
            return order;
        }
        let map = &game.map;
        let style = self.style;
        let sight = game.sight(side);
        // Who stays home: in each city, its strongest, as many as its threat
        // calls for and at least `keep` (one more in its capital). Threat and
        // defence are weighed by what they are worth under the rules: with
        // much chance in battle, numbers more than strength.
        if self.threats.as_ref().is_none_or(|(turn, _)| *turn != (game.turn(), side)) {
            let threats = map
                .cities()
                .iter()
                .filter(|c| c.side == side)
                .map(|c| (c.id, threat(game, side, c.pos, &sight)))
                .collect();
            self.threats = Some(((game.turn(), side), threats));
        }
        let threats = self.threats.as_ref().map(|(_, t)| t.clone()).unwrap_or_default();
        let capital = capital(game, side);
        let mut staying: Vec<UnitId> = Vec::new();
        let mut short: Vec<Hex> = Vec::new();
        for city in map.cities().iter().filter(|c| c.side == side) {
            let guard = daring(&style, game).2;
            let danger =
                threats.iter().find(|(id, _)| *id == city.id).map_or(0.0, |(_, t)| *t) * f64::from(guard) / 100.0;
            let keep = style.keep + usize::from(Some(city.id) == capital);
            let mut here: Vec<&Unit> = map
                .units_at(city.pos)
                .iter()
                .filter_map(|id| map.units().get(*id))
                .filter(|u| !map.is_commander(u.id()))
                .collect();
            here.sort_by_key(|u| std::cmp::Reverse(u.power()));
            let (mut held, mut count) = (0.0, 0);
            for unit in here {
                if count >= keep && held >= danger {
                    break;
                }
                held += worth(unit.power() + city.walls, game);
                count += 1;
                staying.push(unit.id());
            }
            if held < danger || count == 0 {
                short.push(city.pos);
            }
        }
        let free: Vec<&Unit> = map
            .units()
            .of_side(side)
            .filter(|u| map.contains(u.pos()) && u.movement > 0)
            .filter(|u| !self.done.contains(&u.id()) && !staying.contains(&u.id()))
            .collect();
        if free.is_empty() {
            return Order::EndTurn;
        }
        let ranges: Vec<(UnitId, crate::state::MoveRange)> =
            free.iter().map(|u| (u.id(), game.known_range(u, &sight))).collect();
        let reaches = |id: UnitId, hex: Hex| ranges.iter().any(|(u, r)| *u == id && r.cost(hex).is_some());

        // A threatened city first: free units that can get in this turn do.
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

        // Strikes: the other sides' stacks near its cities, then the cities
        // most worth taking, then stacks in the field near its units.
        let mut targets: Vec<(Hex, u32)> = Vec::new();
        for unit in map.units().iter().filter(|u| u.side() != side && map.contains(u.pos())) {
            let near_home = map
                .cities()
                .iter()
                .any(|c| c.side == side && c.pos.distance(unit.pos()) <= unit.base_move_points / 2 + 1);
            let near_free = free.iter().any(|u| u.pos().distance(unit.pos()) <= 4);
            if (near_home || near_free) && map.city_at(unit.pos()).is_none() {
                targets.push((unit.pos(), if near_home { 300 } else { 50 }));
            }
        }
        for city in map.cities().iter().filter(|c| c.side != side && sight.knows(c.pos)) {
            targets.push((city.pos, 100 + city.income * 4));
        }
        targets.sort_by_key(|(h, value)| (std::cmp::Reverse(*value), h.row, h.col));
        targets.dedup_by_key(|(h, _)| *h);
        // (A city it cannot see may hold anyone: it goes to look first.)
        targets.retain(|(h, _)| sight.sees(*h) && !self.is_blocked(*h, game));
        for (target, _) in &targets {
            let able: Vec<UnitId> = free.iter().filter(|u| reaches(u.id(), *target)).map(|u| u.id()).collect();
            let more = able.len();
            let able = strongest_stack(game, able);
            if able.is_empty() {
                continue;
            }
            let odds = game.battle_odds(&able, *target).unwrap_or(100);
            let (dare, wear, _) = daring(&style, game);
            if odds >= dare || (more > able.len() && odds >= wear) {
                self.done.extend(able.iter().copied());
                return Order::Move { units: able, to: *target };
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
        if !frontier.is_empty() && !lost {
            if self.scout.is_none_or(|id| map.units().get(id).is_none()) {
                let fastest = free.iter().filter(|u| !map.is_commander(u.id())).max_by_key(|u| u.base_move_points);
                self.scout = fastest.map(|u| u.id());
            }
            if let Some(scout) = self.scout.and_then(|id| free.iter().copied().find(|u| u.id() == id)) {
                self.done.push(scout.id());
                if let Some(order) = self.explore(game, (side, &sight), (&[scout.id()], scout), &frontier) {
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
            // A temple this turn, while some of them are not blessed.
            if stack.iter().any(|u| u.blessing.is_none()) {
                let room = |t: Hex| {
                    let there = map.units_at(t);
                    there.iter().filter_map(|id| map.units().get(*id)).all(|u| u.side() == side)
                        && there.len() + ids.len() <= MAX_STACK
                };
                if let Some(temple) = temples
                    .iter()
                    .copied()
                    .find(|t| *t != here && all_reach(*t) && room(*t) && !self.is_blocked(*t, game))
                {
                    return Order::Move { units: ids, to: temple };
                }
            }
            let lead = stack.iter().copied().min_by_key(|u| u.movement).unwrap_or(unit);
            if lost {
                match self.explore(game, (side, &sight), (&ids, lead), &frontier) {
                    Some(order) => return order,
                    None => continue,
                }
            }
            // A small stack joins the army first, if the army is near; the
            // army's own, while small, waits for them. (Not for a city of
            // nobody's that it sees nobody in: that, it goes and takes.)
            let target = self.objective(game, (side, &sight), here, lead, ids.len());
            let empty = |t: Hex| map.city_at(t).is_some_and(|c| c.side == Side::Grey) && map.units_at(t).is_empty();
            let small = ids.len() < style.muster && !target.is_some_and(|t| sight.sees(t) && empty(t));
            if small && hub == Some(here) {
                continue;
            }
            if let Some(hub) = hub.filter(|h| *h != here && small && h.distance(here) <= 12) {
                let room = map.units_at(hub).len() + ids.len() <= MAX_STACK + 1;
                let blocked = |h: Hex| self.is_blocked(h, game);
                if let Some(order) = room.then(|| advance(game, (side, &sight), (&ids, lead), hub, &blocked)).flatten()
                {
                    return order;
                }
            }
            // Too few to go alone: in one of its cities, they wait for more.
            if small && map.city_at(here).is_some_and(|c| c.side == side) {
                continue;
            }
            let Some(target) = target else {
                // Nothing it knows of to go for: it goes to see more.
                match self.explore(game, (side, &sight), (&ids, lead), &frontier) {
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
                    .filter(|h| {
                        all_reach(*h)
                            && map
                                .units_at(*h)
                                .iter()
                                .all(|id| ids.contains(id) || map.units().get(*id).is_some_and(|u| u.side() == side))
                    })
                    .filter(|h| map.units_at(*h).len() + ids.len() <= MAX_STACK && !self.is_blocked(*h, game))
                    .min_by_key(|h| (h.distance(here), h.row, h.col));
                if let Some(look) = look {
                    return Order::Move { units: ids, to: look };
                }
                if all_reach(target) {
                    continue;
                }
                match advance(game, (side, &sight), (&ids, lead), target, &|h| self.is_blocked(h, game)) {
                    Some(order) => return order,
                    None => continue,
                }
            }
            // Near enough to strike, but not strong enough: it waits there for
            // more, out of the way.
            if all_reach(target) {
                continue;
            }
            if let Some(order) = advance(game, (side, &sight), (&ids, lead), target, &|h| self.is_blocked(h, game)) {
                return order;
            }
        }
        Order::EndTurn
    }

    /// A stack goes to see more of the map (total fog): toward the nearest
    /// edge of the land its side knows that it can stand on, away from where
    /// its other stacks went to this turn.
    fn explore(
        &mut self,
        game: &Game,
        (side, sight): (Side, &Sight),
        (ids, lead): (&[UnitId], &Unit),
        frontier: &[Hex],
    ) -> Option<Order> {
        if frontier.is_empty() {
            return None;
        }
        // The nearest it can get to, by the way over the land it knows.
        let edge: Vec<Hex> = game
            .frontier_for(lead, sight)
            .into_iter()
            .map(|(h, _)| h)
            .filter(|h| !self.scouted.iter().any(|s| s.distance(*h) <= SCOUT_SPREAD) && !self.is_blocked(*h, game))
            .collect();
        let blocked = |h: Hex| self.is_blocked(h, game);
        let (to, order) = edge
            .into_iter()
            .take(SCOUT_TRIES)
            .find_map(|to| advance(game, (side, sight), (ids, lead), to, &blocked).map(|order| (to, order)))?;
        self.scouted.push(to);
        // Those that go may go on, seeing more, a few steps a turn.
        let steps = match self.explored.iter_mut().find(|(id, _)| *id == lead.id()) {
            Some((_, n)) => {
                *n += 1;
                *n
            }
            None => {
                self.explored.push((lead.id(), 1));
                1
            }
        };
        if let (Order::Move { units, .. }, true) = (&order, steps < SCOUT_STEPS) {
            self.done.retain(|id| !units.contains(id));
        }
        Some(order)
    }
}

impl Planner {
    /// The treasury and the cities' work: one order, or none to give.
    fn economy(&mut self, game: &Game, side: Side) -> Option<Order> {
        let map = &game.map;
        let own = || map.cities().iter().filter(|c| c.side == side);
        // What its taxes leave after its units' upkeep and that of one more of
        // each kind its cities are building: what it can keep more.
        let planned: u32 = own().filter_map(|c| c.production()).map(|p| p.upkeep).sum();
        let spare = i64::from(game.income(side)) - i64::from(game.upkeep(side)) - i64::from(planned);
        if spare < 0 {
            let dearest = own().filter(|c| c.production().is_some()).max_by_key(|c| c.production().map(|p| p.upkeep));
            if let Some(city) = dearest {
                return Some(Order::Build { city: city.id, what: None });
            }
        }
        let units = map.units();
        let leads =
            |p: &UnitProd| units.kind_named(&p.basename).and_then(|k| units.kind(k)).is_some_and(|k| k.commander > 0);
        // A commander, where it can be built and kept, if it has none (two,
        // for the careful, once it holds four cities).
        let leaders = units.of_side(side).filter(|u| map.is_commander(u.id())).count();
        let wanted = if self.style.name == CAREFUL.name && own().count() >= 4 { 2 } else { 1 };
        let building_one = own().any(|c| c.production().is_some_and(&leads));
        if leaders < wanted && !building_one {
            let at = own().find_map(|city| {
                let i = city.units.iter().position(|p| leads(p) && i64::from(p.upkeep) <= spare)?;
                Some((city.id, i))
            });
            if let Some((city, i)) = at {
                return Some(Order::Build { city, what: Some(i) });
            }
        }
        // An idle city builds what is worth most, of what it can keep and pay
        // for by the time it is done; one that builds a commander it no longer
        // needs builds something else.
        let gold = i64::from(game.gold(side));
        let net = i64::from(game.income(side)) - i64::from(game.upkeep(side));
        let choose = |city: &crate::state::City| -> Option<usize> {
            let affordable = |p: &UnitProd| {
                i64::from(p.upkeep) <= spare && gold + net * i64::from(p.time.max(1)) >= i64::from(p.cost)
            };
            city.units
                .iter()
                .enumerate()
                .filter(|(_, p)| !leads(p) && affordable(p))
                .max_by(|(_, a), (_, b)| {
                    production_score(a, game, &self.style).total_cmp(&production_score(b, game, &self.style))
                })
                .map(|(i, _)| i)
        };
        let idle = own().filter(|c| c.building.is_none()).find_map(|c| choose(c).map(|i| (c.id, i)));
        if let Some((city, i)) = idle {
            return Some(Order::Build { city, what: Some(i) });
        }
        let surplus: Option<CityId> = (leaders >= wanted)
            .then(|| own().find(|c| c.production().is_some_and(&leads) && c.progress == 0).map(|c| c.id))
            .flatten();
        if let Some(city) = surplus {
            return Some(Order::Build { city, what: None });
        }
        None
    }

    /// The city most worth going for from `here`, for a stack of `size`
    /// led by `lead` (the slowest): its value (taxes, and more for the other
    /// sides' than for nobody's) against the way there and what defends it.
    fn objective(
        &self,
        game: &Game,
        (side, sight): (Side, &Sight),
        here: Hex,
        lead: &Unit,
        size: usize,
    ) -> Option<Hex> {
        let map = &game.map;
        let mut best: Option<(i64, Hex)> = None;
        let room = |hex: Hex| map.has_room(hex, lead.id());
        let known = |c: &&crate::state::City| c.side != side && sight.knows(c.pos) && !self.is_blocked(c.pos, game);
        for city in map.cities().iter().filter(known) {
            // In the fog: as many as it saw there last, or a guess.
            let defenders = if sight.sees(city.pos) {
                map.units_at(city.pos).len() as i64
            } else {
                sight.ghost_at(city.pos).map_or(2, |g| g.count as i64)
            };
            let value = 20 + i64::from(city.income) * 3 + if city.side == Side::Grey { 0 } else { 10 };
            let trouble = i64::from(here.distance(city.pos)) + 3 * (defenders + i64::from(city.walls))
                - 2 * size as i64
                + if self.style.name == BOLD.name { -i64::from(city.walls) * 2 } else { 0 };
            let score = value * 100 / (10 + trouble.max(0));
            if best.is_none_or(|(s, _)| score > s) && game.known_step_toward(lead, city.pos, &room, sight).is_some() {
                best = Some((score, city.pos));
            }
        }
        best.map(|(_, h)| h)
    }
}
