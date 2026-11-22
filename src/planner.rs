//! The computer's players that plan ahead: one agent, in a few styles
//! (`steady`, `bold`, `careful`). Each turn, order by order, it:
//!
//! 1. keeps its treasury in hand: what its cities build is what it can keep
//!    (their upkeep within its taxes, and a share of its gold a turn: the
//!    treasury is not a hoard) and what is worth most *under the rules
//!    of this game* (with little chance in battle a strong unit is worth far
//!    more than a weak one; with much, or with stacks that fight as one, the
//!    numbers count), and it keeps a commander;
//! 2. defends: a city with other sides' units near enough to strike keeps
//!    defenders enough for the threat, and free units near it come in;
//! 3. strikes where the odds are good enough for its style (the odds are the
//!    game's own, so the rules in effect are in them), waves at walls when
//!    more follow, and stacks it can beat in the field;
//! 4. gathers the rest into armies around its commander, blessed at the
//!    temples on the way (when half of a stack has a blessing to get), and
//!    sends each toward the city most worth taking for the trouble, never
//!    to stand where a stack it could not beat can come to it (`wary`).
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
use crate::state::combat::{DEFAULT_CHANCE, MAX_CHANCE, System, walls_bonus};
use crate::state::{
    CityId, Hex, HexSide, MAX_STACK, Side, TileType, Travel, Unit, UnitId, UnitProd, compute_range, hostile,
};

mod rounds;
mod wary;

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
    /// held. (One fewer than it was, for each style and never under three:
    /// waiting is dear. With four, the steady planner won far fewer games
    /// than with three, in rounds and in turns.)
    pub muster: usize,
    /// What its cities' work weighs, in percent: speed (a unit's moves), and
    /// price (against what it is worth).
    pub speed: u32,
    pub thrift: u32,
    /// How much a stack of another side may be worth, in percent of its own
    /// stack's worth, for that stack to go where the other could come to
    /// it (`wary`).
    pub nerve: u32,
}

/// Balanced: armies around a commander, cities defended as they are
/// threatened, attacks at fair odds.
pub const STEADY: Style =
    Style { name: "steady", keep: 1, dare: 60, wear: 25, guard: 100, muster: 3, speed: 30, thrift: 100, nerve: 100 };
/// Fast and many: grabs the empty and weak cities first, few defenders,
/// attacks at lower odds.
pub const BOLD: Style =
    Style { name: "bold", keep: 1, dare: 55, wear: 18, guard: 70, muster: 3, speed: 20, thrift: 140, nerve: 130 };
/// Slow and sure: strong defenders, big armies, high odds.
pub const CAREFUL: Style =
    Style { name: "careful", keep: 1, dare: 68, wear: 30, guard: 150, muster: 4, speed: 10, thrift: 70, nerve: 80 };

pub const STYLES: [Style; 3] = [STEADY, BOLD, CAREFUL];

/// The style of a name.
pub fn style(name: &str) -> Option<Style> {
    STYLES.into_iter().find(|s| s.name == name)
}

/// Over how many turns it would spend its treasury on upkeep: its cities
/// build what its taxes keep and a share of its gold besides
/// (`Planner::reserve`). (Played out: 6, 8 and 12 win alike, 24 less.)
const SPEND: u32 = 12;

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
    /// Units it sent to a temple this turn: they seek its blessing there.
    pilgrims: Vec<UnitId>,
    /// Where its stacks went to see more of the map this turn (total fog);
    /// and the unit that goes on seeing more while there is land it has not
    /// seen, its scout.
    scouted: Vec<Hex>,
    scout: Option<UnitId>,
    /// How many steps each unit has taken this turn to see more: a few, while
    /// it has the moves (the edge of what it knows is near).
    explored: Vec<(UnitId, u32)>,
    /// How many units it sent home this turn (to make room for better).
    disbanded: usize,
    /// It plays a game of rounds as it plays turns (`<style>/turns`: to
    /// play the two against each other).
    turnwise: bool,
    /// The other sides' stacks it sees and where each could be, as last
    /// worked out (`spot`: the round, the side, and in turns the map's
    /// revision then).
    foes: Option<wary::Spotted>,
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
            pilgrims: Vec::new(),
            scouted: Vec::new(),
            scout: None,
            explored: Vec::new(),
            disbanded: 0,
            turnwise: false,
            foes: None,
        }
    }

    /// A player of `style` that plays a game of rounds as it plays turns:
    /// what the planners did in rounds before they had a way of their own
    /// for them, kept to measure that against.
    pub fn turnwise(style: Style) -> Planner {
        Planner { turnwise: true, ..Planner::new(style) }
    }

    /// True if it plays this game in its way for rounds (`rounds`).
    fn in_rounds(&self, game: &Game) -> bool {
        game.rounds() && !self.turnwise
    }

    /// What of its treasury pays for upkeep its taxes do not, a turn: a
    /// share of it (`SPEND`), so that it fields what it can pay for while
    /// the gold lasts. (Its cities built what its taxes kept, and the gold
    /// piled up: two hundred and more, with small armies.)
    fn reserve(&self, game: &Game, side: Side) -> i64 {
        i64::from(game.gold(side) / SPEND)
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
/// In rounds, how far past the edge of the known land a stack that goes to
/// see more is sent (`beyond`): about a round's march.
const SCOUT_REACH: u32 = 8;
/// Late in the game (the owner, 2026-11-20: "late game should have a lot of
/// quality stacks, not just junk units"): with this many soldiers (two
/// stacks' worth, more than it fields at once), it builds the strongest it
/// can keep; and, a few a turn, it sends home its weakest to make room for
/// one at least `DISBAND_GAP` the stronger.
const LATE_SOLDIERS: usize = 2 * MAX_STACK;
const DISBAND_GAP: u32 = 3;
const MAX_DISBANDS: usize = 4;
/// A threatened city it cannot hold with those in it raises its walls a
/// level at a time, up to this (where they make its defenders 2 stronger,
/// `walls_bonus`), if its treasury has twice the price.
const WALLS_GOAL: u32 = 3;

/// How much a unit is worth in battle under the rules in effect, against
/// one of strength 1: in lines a unit fights its duels alone, and its odds go
/// as its strength to the power the chance gives (`combat`: `k = 2 (10 -
/// chance) / 5`); in strength against strength and on the dice, a stack's
/// strengths add up.
///
/// (In plain arithmetic, with no `powf`: `k` is a fifth of a whole number,
/// so `s^k` is the fifth root of `s` to that number; the same on every
/// machine, so a seed plays the same game everywhere.)
pub fn worth(strength: u32, game: &Game) -> f64 {
    let rules = game.setup().combat;
    // No more than the game's cap lets it fight with.
    let strength = rules.cap.map_or(strength, |cap| strength.min(cap));
    // k = m / 5.
    let m = 2 * u32::from(10u8.saturating_sub(rules.chance));
    let s = f64::from(strength.max(1));
    match rules.system {
        System::Lines => fifth_root(power(s, m.max(2))),
        System::Strength => s * (1.0 + f64::from(m) / 20.0),
        // Whole stacks too, and the game's chance plays no part: as by
        // strength at the chance a game has unless it is set.
        System::Dice => s * (1.0 + f64::from(MAX_CHANCE - DEFAULT_CHANCE) / 10.0),
    }
}

/// In rounds, where a stack that goes to see more of the map by way of
/// `edge` (a hex at the edge of the land its side knows, `sight`) is sent
/// (`Order::Toward`): a stack has one plan a round, made before it sees
/// anything new, so a plan to the edge would end there with most of its
/// movement unspent. It is sent on past the edge, straight out into the
/// land not seen (`SCOUT_REACH` hexes, or as far as it is not seen): it
/// goes as far as it can over the land it comes to see (`game::rounds`).
pub fn beyond(game: &Game, sight: &Sight, edge: Hex) -> Hex {
    let (rows, cols) = (game.map.rows(), game.map.cols());
    let on_map = |(row, col): (i32, i32)| Hex::on_map(row, col, rows, cols);
    let out = HexSide::ALL.into_iter().find(|side| on_map(edge.toward(*side)).is_some_and(|h| !sight.knows(h)));
    let Some(out) = out else { return edge };
    // (Only as far as the land is not seen: a hex it has seen and cannot
    // get to, the sea say, is nowhere to be sent.)
    let mut to = edge;
    for _ in 0..SCOUT_REACH {
        match on_map(to.toward(out)).filter(|h| !sight.knows(*h)) {
            Some(next) => to = next,
            None => break,
        }
    }
    to
}

/// In rounds, where a stack led by `lead` (its slowest) that goes to see
/// more by way of `edge` is sent: past the edge ([`beyond`]) if it gets to
/// the edge this round and its way past is by the edge; else to the edge
/// itself. The game sends a stack toward land not seen by the hex it knows
/// and can get to that is nearest to it as the crow flies, and refuses if
/// the stack is as near already. Three ways a stack sent past an edge never
/// got to it, round after round, while the armies of a side that held
/// eight cities of nine looked for the ninth for a hundred rounds:
///
/// - It stood as near to what lay past the edge as the edge is (across a
///   strip not seen): refused.
/// - The edge was rounds away: it went north round a lake one round,
///   toward the hex nearest to what lay past one edge, and back south the
///   next, toward the hex nearest to what lay past another. The way to an
///   edge it knows is a way it knows: each round along it is a round
///   nearer.
/// - The hex nearest to what lay past the edge was not the edge (the far
///   side of a range of mountains it cannot enter): it went there, saw
///   nothing new, and the edge was still the nearest the round after.
pub fn outward(game: &Game, sight: &Sight, lead: &Unit, edge: Hex) -> Hex {
    if game.known_range(lead, sight).cost(edge).is_none() {
        return edge;
    }
    let far = beyond(game, sight, edge);
    // Where the game would send it: as `Game::way_toward` chooses.
    let mut tireless = lead.clone();
    tireless.movement = u32::MAX / 2;
    let range = game.known_range(&tireless, sight);
    let by = range
        .reachable()
        .filter(|(hex, _)| !hostile(&game.map, lead.side(), *hex))
        .min_by_key(|(hex, cost)| (hex.distance(far), *cost, hex.row, hex.col))
        .map(|(hex, _)| hex);
    if by == Some(edge) && edge.distance(far) < lead.pos().distance(far) { far } else { edge }
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
/// little more for its speed (and for a flyer, which no ground slows or
/// stops), for the turns and the gold it takes.
fn production_score(prod: &UnitProd, game: &Game, style: &Style) -> f64 {
    let units = game.map.units();
    let flies = units.kind_named(&prod.basename).and_then(|k| units.kind(k)).is_some_and(|k| k.travel == Travel::Air);
    let moves = f64::from(prod.movement) * if flies { FLIGHT } else { 1.0 };
    let speed = 1.0 + f64::from(style.speed) / 100.0 * moves / 10.0;
    let price = f64::from(prod.cost) + 8.0 * f64::from(prod.upkeep);
    let turns = f64::from(prod.time.max(1));
    worth(prod.strength, game) * speed / (turns * 6.0 + price * f64::from(style.thrift) / 100.0)
}

/// How much farther than its movement points say a flyer goes, to the
/// planner: rough country, rivers and the sea do not slow it.
const FLIGHT: f64 = 2.5;

/// Of these units, as many as attack together (a hex's worth: the strongest
/// [`MAX_STACK`] and a commander), the strongest first; against a city with
/// walls, its best siege engine first of all.
fn strongest_stack(game: &Game, mut units: Vec<UnitId>, target: Hex) -> Vec<UnitId> {
    let map = &game.map;
    let walled = map.city_at(target).is_some_and(|c| c.walls > 0);
    let siege = |id: &UnitId| {
        let unit = map.units().get(*id);
        if walled { unit.and_then(|u| map.units().kind_of(u)).map_or(0, |k| k.siege) } else { 0 }
    };
    let power = |id: &UnitId| map.units().get(*id).map_or(0, |u| u.fighting_power());
    units.sort_by_key(|id| (std::cmp::Reverse(siege(id)), std::cmp::Reverse(power(id))));
    // One siege engine is enough: the others go by their strength.
    if units.first().is_some_and(|id| siege(id) > 0) {
        let rest = &mut units[1..];
        rest.sort_by_key(|id| std::cmp::Reverse(power(id)));
    }
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
            .map(|u| worth(u.fighting_power(), game))
            .sum::<f64>()
}

/// This turn's step of the stack `ids` toward `to`, as its side knows the
/// map: as far as the slowest (`lead`) gets on its cheapest way, where there
/// is room for them, and never into another side's units (an attack is
/// decided on its own odds): a move of those of them that can get there.
/// Worked out on what it sees, not left to the game (a way through the fog
/// that is shut shows itself as it goes).
pub(crate) fn advance(
    game: &Game,
    (side, sight): (Side, &Sight),
    (ids, lead): (&[UnitId], &Unit),
    to: Hex,
    blocked: &dyn Fn(Hex) -> bool,
) -> Option<Order> {
    let map = &game.map;
    // Room for all of them there: a stack's worth, and one commander.
    let room = |hex: Hex| {
        let there = map.units_at(hex);
        let leaders = there.iter().chain(ids).filter(|id| map.is_commander(**id)).count();
        let fits = there.len() + ids.len() - leaders <= MAX_STACK && leaders <= 1;
        !blocked(hex) && (there.iter().all(|id| ids.contains(id)) || fits)
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
            self.pilgrims.clear();
            self.scouted.clear();
            self.explored.clear();
            self.disbanded = 0;
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
        if let Some(order) = crate::agents::blessing(game, side, &mut self.pilgrims) {
            return order;
        }
        if let Some(order) = self.economy(game, side) {
            return order;
        }
        let map = &game.map;
        let style = self.style;
        let sight = game.sight(side);
        let (staying, short) = match self.homes(game, side, &sight) {
            Ok(homes) => homes,
            Err(order) => return order,
        };
        // A game of rounds is another game (`rounds`).
        if self.in_rounds(game) {
            return self.round(game, (side, &sight), (&staying, &short));
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
            // Near its free units, or where they can strike this turn (a
            // stack they went for, in the only way to a city, waited them
            // out five hexes off).
            let near_free = free.iter().any(|u| u.pos().distance(unit.pos()) <= 4 || reaches(u.id(), unit.pos()));
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
            let able = strongest_stack(game, able, *target);
            if able.is_empty() {
                continue;
            }
            let forecast = game.battle_forecast(&able, *target);
            let odds = forecast.map_or(100, |f| f.odds);
            let (dare, wear, _) = daring(&style, game);
            if odds >= dare
                || (more > able.len() && odds >= wear)
                || crate::agents::siege(game, side, *target, forecast)
            {
                self.done.extend(able.iter().copied());
                return Order::Move { units: able, to: *target };
            }
        }

        // What is left goes somewhere, and not where it would be beaten
        // (`wary`).
        let foes = self.spot(game, side);
        let heed = |p: &Planner, order: Order| p.wary(game, (side, &sight), &foes, order);
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
            // A temple this turn that has not blessed some of them: they go,
            // and seek its blessing.
            let room = |t: Hex| {
                let there = map.units_at(t);
                there.iter().filter_map(|id| map.units().get(*id)).all(|u| u.side() == side)
                    && there.len() + ids.len() <= MAX_STACK
            };
            if let Some(temple) = temples.iter().copied().find(|t| {
                let unblessed = stack.iter().filter(|u| !u.blessed_at(*t)).count();
                *t != here
                    && unblessed > 0
                    && 2 * unblessed >= stack.len()
                    && all_reach(*t)
                    && room(*t)
                    && !self.is_blocked(*t, game)
            }) {
                match heed(self, Order::Move { units: ids.clone(), to: temple }) {
                    Some(Order::Move { units, to }) if to == temple => {
                        self.pilgrims.extend(units.iter().copied());
                        return Order::Move { units, to };
                    }
                    Some(order) => return order,
                    None => continue,
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
            // nobody's it sees and is likely to take as it is, empty or held
            // by a few: that, it goes and takes; every neutral city has a
            // garrison, and the armies waited while others took them.)
            let target = self.objective(game, (side, &sight), here, lead, ids.len());
            let takes = |t: Hex| {
                map.city_at(t).is_some_and(|c| c.side == Side::Grey)
                    && (map.units_at(t).is_empty()
                        || game.battle_forecast(&ids, t).is_some_and(|f| f.odds >= daring(&style, game).0))
            };
            let small = ids.len() < style.muster && !target.is_some_and(|t| sight.sees(t) && takes(t));
            if small && hub == Some(here) {
                continue;
            }
            if let Some(hub) = hub.filter(|h| *h != here && small && h.distance(here) <= 12) {
                let room = map.units_at(hub).len() + ids.len() <= MAX_STACK + 1;
                let blocked = |h: Hex| self.is_blocked(h, game);
                if let Some(order) = room
                    .then(|| advance(game, (side, &sight), (&ids, lead), hub, &blocked))
                    .flatten()
                    .and_then(|o| heed(self, o))
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
                    match heed(self, Order::Move { units: ids, to: look }) {
                        Some(order) => return order,
                        None => continue,
                    }
                }
                if all_reach(target) {
                    continue;
                }
                match advance(game, (side, &sight), (&ids, lead), target, &|h| self.is_blocked(h, game))
                    .and_then(|o| heed(self, o))
                {
                    Some(order) => return order,
                    None => continue,
                }
            }
            // Near enough to strike, but not strong enough: it waits there for
            // more, out of the way.
            if all_reach(target) {
                continue;
            }
            if let Some(order) = advance(game, (side, &sight), (&ids, lead), target, &|h| self.is_blocked(h, game))
                .and_then(|o| heed(self, o))
            {
                return order;
            }
        }
        Order::EndTurn
    }

    /// Its cities, before anything else: who stays home (in each city, its
    /// strongest, as many as its threat calls for and at least `keep`, one
    /// more in its capital), and the cities that are short of defenders. Or
    /// an order first: a threatened city, held by too few, behind low
    /// walls, raises them.
    fn homes(&mut self, game: &Game, side: Side, sight: &Sight) -> Result<(Vec<UnitId>, Vec<Hex>), Order> {
        let map = &game.map;
        let style = self.style;
        // Who stays home: in each city, its strongest, as many as its threat
        // calls for and at least `keep` (one more in its capital). Threat and
        // defence are weighed by what they are worth under the rules: with
        // much chance in battle, numbers more than strength.
        if self.threats.as_ref().is_none_or(|(turn, _)| *turn != (game.turn(), side)) {
            let threats = map
                .cities()
                .iter()
                .filter(|c| c.side == side)
                .map(|c| (c.id, threat(game, side, c.pos, sight)))
                .collect();
            self.threats = Some(((game.turn(), side), threats));
        }
        let threats = self.threats.as_ref().map(|(_, t)| t.clone()).unwrap_or_default();
        // A threatened city, held by too few, behind low walls: it raises
        // them, if the treasury is well off (walls are dear).
        for (id, danger) in &threats {
            let Some(city) = map.cities().get(*id).filter(|c| c.walls < WALLS_GOAL) else { continue };
            let held: f64 = map
                .units_at(city.pos)
                .iter()
                .filter_map(|u| map.units().get(*u))
                .map(|u| worth(u.fighting_power() + walls_bonus(city.walls), game))
                .sum();
            let danger = danger * f64::from(daring(&style, game).2) / 100.0;
            if danger > 0.0 && held < danger {
                if let Ok(price) = game.walls_price(side, city.id) {
                    if game.gold(side) >= 2 * price {
                        return Err(Order::Walls { city: city.id });
                    }
                }
            }
        }
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
            here.sort_by_key(|u| std::cmp::Reverse(u.fighting_power()));
            let (mut held, mut count) = (0.0, 0);
            for unit in here {
                if count >= keep && held >= danger {
                    break;
                }
                held += worth(unit.fighting_power() + walls_bonus(city.walls), game);
                count += 1;
                staying.push(unit.id());
            }
            if held < danger || count == 0 {
                short.push(city.pos);
            }
        }
        Ok((staying, short))
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
        let mut edge: Vec<Hex> = game
            .frontier_for(lead, sight)
            .into_iter()
            .map(|(h, _)| h)
            .filter(|h| !self.scouted.iter().any(|s| s.distance(*h) <= SCOUT_SPREAD) && !self.is_blocked(*h, game))
            .collect();
        // All of it: those of the stack that go at other prices (riders
        // with foot, who do not climb where foot does) can get there too.
        // (A commander goes as those it leads.) A stack of riders and foot
        // was sent, round after round, where only the foot could go: the
        // game refused, and what it had claimed to look at nobody else
        // went to see.
        let map = &game.map;
        let mut tables = vec![lead.move_cost];
        for unit in ids.iter().filter_map(|id| map.units().get(*id)).filter(|u| !map.is_commander(u.id())) {
            if !tables.contains(&unit.move_cost) {
                tables.push(unit.move_cost);
                let theirs: Vec<Hex> = game.frontier_for(unit, sight).into_iter().map(|(h, _)| h).collect();
                edge.retain(|h| theirs.contains(h));
            }
        }
        let blocked = |h: Hex| self.is_blocked(h, game);
        let (to, order) = edge
            .into_iter()
            .take(SCOUT_TRIES)
            .find_map(|to| advance(game, (side, sight), (ids, lead), to, &blocked).map(|order| (to, order)))?;
        self.scouted.push(to);
        // In rounds: on past the edge, as far as it gets this round (one
        // plan a round).
        if game.rounds() {
            let far = outward(game, sight, lead, to);
            let out = if self.is_blocked(far, game) { to } else { far };
            return Some(Order::Toward { units: ids.to_vec(), to: out });
        }
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
        let spare =
            i64::from(game.income(side)) - i64::from(game.upkeep(side)) - i64::from(planned) + self.reserve(game, side);
        // (When it cannot keep what its cities build: late, it sends its
        // weakest home first, below; else it stops the dearest work.)
        let units = map.units();
        let leads =
            |p: &UnitProd| units.kind_named(&p.basename).and_then(|k| units.kind(k)).is_some_and(|k| k.commander > 0);
        // (It does not carry armies by sea yet: it builds no ships.)
        let ship = |p: &UnitProd| {
            units.kind_named(&p.basename).and_then(|k| units.kind(k)).is_some_and(|k| k.travel == Travel::Sea)
        };
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
        // Late (see `LATE_SOLDIERS`), it makes room for the strongest an idle
        // city could build, were there upkeep for it (or for what its cities
        // build now, when it cannot keep that): it sends home its weakest,
        // not the last in a city, nor aboard a ship.
        let gold = i64::from(game.gold(side));
        let net = i64::from(game.income(side)) - i64::from(game.upkeep(side));
        let soldiers: Vec<&Unit> =
            units.of_side(side).filter(|u| map.contains(u.pos()) && !map.is_commander(u.id())).collect();
        let late = soldiers.len() >= LATE_SOLDIERS;
        // (Its scout, or under total fog the one that would be: its fastest.)
        let explorer = (game.setup().fog == crate::setup::Fog::Total)
            .then(|| soldiers.iter().max_by_key(|u| (u.base_move_points, u.id().index)).map(|u| u.id()))
            .flatten();
        let scouts = [self.scout, explorer];
        if late && self.disbanded < MAX_DISBANDS {
            let could = |p: &&UnitProd| {
                !leads(p)
                    && !ship(p)
                    && gold + (net.max(0) + i64::from(p.upkeep)) * i64::from(p.time.max(1)) >= i64::from(p.cost)
            };
            let wish = own()
                .filter(|c| c.building.is_none())
                .flat_map(|c| c.units.iter())
                .filter(could)
                .max_by_key(|p| (p.strength, std::cmp::Reverse(p.upkeep)))
                .filter(|p| i64::from(p.upkeep) > spare)
                .or_else(|| own().filter_map(|c| c.production()).filter(|_| spare < 0).max_by_key(|p| p.strength));
            let alone = |u: &Unit| {
                map.city_at(u.pos()).is_some() && map.units_at(u.pos()).iter().filter(|id| id.side == side).count() <= 1
            };
            let at_sea = |u: &Unit| map.tile(u.pos()).is_some_and(|t| t.kind == TileType::Water);
            let junk = wish.and_then(|p| {
                soldiers
                    .iter()
                    .filter(|u| u.power() + DISBAND_GAP <= p.strength && !alone(u) && !at_sea(u))
                    // (Not a ship, nor a siege engine: weak, but not junk; it
                    // once sent home each it had just built for the walls.
                    // Nor its scout, which it would then replace.)
                    .filter(|u| units.kind_of(u).is_some_and(|k| k.travel != Travel::Sea && k.siege == 0))
                    .filter(|u| !scouts.contains(&Some(u.id())))
                    .min_by_key(|u| (u.power(), std::cmp::Reverse(u.upkeep), u.id().index))
            });
            if let Some(unit) = junk {
                self.disbanded += 1;
                return Some(Order::Disband { units: vec![unit.id()] });
            }
        }
        if spare < 0 {
            let dearest = own().filter(|c| c.production().is_some()).max_by_key(|c| c.production().map(|p| p.upkeep));
            if let Some(city) = dearest {
                return Some(Order::Build { city: city.id, what: None });
            }
        }
        // An idle city builds what is worth most, of what it can keep and pay
        // for by the time it is done (late, the strongest of it); one that
        // builds a commander it no longer needs builds something else.
        let side_best =
            own().flat_map(|c| c.units.iter()).filter(|p| !leads(p) && !ship(p)).map(|p| p.strength).max().unwrap_or(0);
        let choose = |city: &crate::state::City| -> Option<usize> {
            let affordable = |p: &UnitProd| {
                i64::from(p.upkeep) <= spare && gold + net * i64::from(p.time.max(1)) >= i64::from(p.cost)
            };
            let score = |p: &UnitProd| production_score(p, game, &self.style);
            // Late, nothing it would send home for the best its cities build:
            // a city that builds only that waits (one built militia while
            // another sent militia home for knights, again and again; built
            // when there was room, they were sent home a few hundred times a
            // game, for few more wins).
            let worth_keeping = |p: &UnitProd| !late || p.strength + DISBAND_GAP > side_best;
            city.units
                .iter()
                .enumerate()
                .filter(|(_, p)| !leads(p) && !ship(p) && affordable(p) && worth_keeping(p))
                .max_by(|(_, a), (_, b)| {
                    let first = if late { a.strength.cmp(&b.strength) } else { std::cmp::Ordering::Equal };
                    first.then(score(a).total_cmp(&score(b)))
                })
                .map(|(i, _)| i)
        };
        // Late, a siege engine for the walls, where one can be built and
        // kept, if it has none and is building none.
        if late {
            let breaks =
                |p: &UnitProd| units.kind_named(&p.basename).and_then(|k| units.kind(k)).is_some_and(|k| k.siege > 0);
            let has = units.of_side(side).any(|u| units.kind_of(u).is_some_and(|k| k.siege > 0))
                || own().any(|c| c.production().is_some_and(&breaks));
            let at = own().filter(|c| c.building.is_none()).find_map(|c| {
                let i = c.units.iter().position(|p| {
                    breaks(p)
                        && i64::from(p.upkeep) <= spare
                        && gold + net * i64::from(p.time.max(1)) >= i64::from(p.cost)
                })?;
                Some((c.id, i))
            });
            if let (false, Some((city, i))) = (has, at) {
                return Some(Order::Build { city, what: Some(i) });
            }
        }
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
            let walls = i64::from(2 * walls_bonus(city.walls));
            let trouble = i64::from(here.distance(city.pos)) + 3 * (defenders + walls) - 2 * size as i64
                + if self.style.name == BOLD.name { -walls * 2 } else { 0 };
            let score = value * 100 / (10 + trouble.max(0));
            if best.is_none_or(|(s, _)| score > s) && game.known_step_toward(lead, city.pos, &room, sight).is_some() {
                best = Some((score, city.pos));
            }
        }
        if best.is_none() {
            // No way to any city it knows of: another side's stack stands in
            // the only way (on a bridge, in a pass), and a way never goes
            // through a stack. It goes for the nearest such stack it sees,
            // to strike it when it can (a side once waited for ever, with
            // gold piling up, while one held the bridge to the last city).
            let mut stacks: Vec<Hex> = map
                .units()
                .iter()
                .filter(|u| u.side() != side && u.side() != Side::Grey && map.contains(u.pos()))
                .map(|u| u.pos())
                .filter(|h| map.city_at(*h).is_none() && sight.sees(*h) && !self.is_blocked(*h, game))
                .collect();
            stacks.sort_by_key(|h| (h.distance(here), h.row, h.col));
            stacks.dedup();
            return stacks.into_iter().find(|h| game.known_step_toward(lead, *h, &room, sight).is_some());
        }
        best.map(|(_, h)| h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::scenario::{self, Scenario};

    /// The bug hunt (2026-11-21): the planner sent a stack with a captain
    /// onto a hex with another captain, which the game refuses (one
    /// commander to a hex); its most common refusal.
    #[test]
    fn a_stack_with_a_commander_is_not_sent_onto_another_commander() {
        let terrain = format!("20\n20\n{}\n", vec!["g".repeat(20); 20].join("\n"));
        let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nenter grass 2\n\
                     unit captain\nname Captain\nstrength 2\nmoves 12\ncommander 1\nenter grass 2\n";
        let (map, warnings) = scenario::parse(&terrain, Some(units), None).unwrap();
        let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
        let put = |game: &mut Game, kind: &str, hex: Hex| {
            let kind = game.map.units().kind_named(kind).unwrap();
            let id = game.map.new_unit_of(Side::Red, kind).unwrap();
            game.map.place_unit(id, hex).unwrap();
            id
        };
        let (from, there) = (Hex::new(5, 5), Hex::new(5, 7));
        let stack = [put(&mut game, "captain", from), put(&mut game, "soldiers", from)];
        put(&mut game, "captain", there);
        let sight = game.sight(Side::Red);
        let lead = game.map.units().get(stack[1]).unwrap().clone();
        let order = advance(&game, (Side::Red, &sight), (&stack, &lead), there, &|_| false);
        assert!(!matches!(order, Some(Order::Move { to, .. }) if to == there), "{order:?}");
    }

    /// In rounds a scout is sent on past the edge of the land its side
    /// knows, straight out, as far as the land is not seen.
    #[test]
    fn a_scout_is_sent_past_the_edge_into_the_land_not_seen() {
        let mut rows = vec!["g".repeat(30); 30];
        rows[15] = format!("{}c{}", "g".repeat(15), "g".repeat(14));
        let terrain = format!("30\n30\n{}\n", rows.join("\n"));
        let cities = "city 15 15\nname Redhold\nside red\n";
        let (map, warnings) = scenario::parse(&terrain, None, Some(cities)).unwrap();
        let game = Game::from_scenario(Scenario { map, assets: None, warnings });
        let mut setup = crate::setup::Setup::solo(&game.map, Some(Side::Red), "passive");
        setup.fog = crate::setup::Fog::Total;
        let game = game.with_setup(setup);
        let (home, sight) = (Hex::new(15, 15), game.sight(Side::Red));
        let edge = sight.frontier(&game.map);
        assert!(!edge.is_empty(), "there is land not seen");
        for hex in edge {
            let to = beyond(&game, &sight, hex);
            assert!(!sight.knows(to) && hex.distance(to) == SCOUT_REACH, "{hex:?} to {to:?}");
            assert!(to.distance(home) > hex.distance(home), "outward: {hex:?} to {to:?}");
        }
        // With nothing unseen beside it, a hex is where it is sent.
        assert_eq!(beyond(&game, &Sight::all(), home), home);
    }

    /// A stack that goes to see more is sent past an edge only if it gets
    /// to the edge this round and its way past is by the edge (see
    /// [`outward`]): else to the edge itself, along the way it knows.
    #[test]
    fn a_stack_is_sent_past_an_edge_only_when_that_gets_it_to_the_edge() {
        let terrain = format!("30\n30\n{}\n", vec!["g".repeat(30); 30].join("\n"));
        let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nenter grass 2\n";
        let (map, warnings) = scenario::parse(&terrain, Some(units), None).unwrap();
        let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
        let here = Hex::new(13, 15);
        let kind = game.map.units().kind_named("soldiers").unwrap();
        let id = game.map.new_unit_of(Side::Red, kind).unwrap();
        game.map.place_unit(id, here).unwrap();
        let lead = game.map.units().get(id).unwrap().clone();
        // The band of land it stands on, and one to the north that it
        // knows too, joined to its own in the west; between them three
        // rows it has not seen, and to the south nothing it has.
        let known = |h: Hex| (12..=14).contains(&h.row) || h.row == 8 || (h.col <= 3 && (9..=11).contains(&h.row));
        let sight = Sight::by_hand(&game.map, &|_| true, Some(&known), Vec::new());
        // The edge to the south, a step off: what lies past it is nearest
        // by the edge, and there it is sent.
        let south = Hex::new(14, 15);
        let far = beyond(&game, &sight, south);
        assert!(!sight.knows(far) && south.distance(far) == SCOUT_REACH);
        assert_eq!(outward(&game, &sight, &lead, south), far);
        // An edge along its band that it does not get to this round (six
        // hexes are its round): the edge itself, by the way it knows.
        let along = Hex::new(14, 25);
        assert!(game.known_range(&lead, &sight).cost(along).is_none());
        assert_eq!(outward(&game, &sight, &lead, along), along);
        // The edge to the north, a step off: what lies past it is nearer
        // to the other band, which it can get to round by the west. Sent
        // past the edge it would go there, and see nothing new: the edge
        // itself.
        let north = Hex::new(12, 15);
        let far = beyond(&game, &sight, north);
        assert!(!sight.knows(far) && north.distance(far) < here.distance(far), "{far:?}");
        assert!((0..30).any(|col| Hex::new(8, col).distance(far) < north.distance(far)));
        assert_eq!(outward(&game, &sight, &lead, north), north);
    }

    /// The review (2026-11-21): the planner weighed a wounded unit as whole
    /// (wounds that last), in its stacks and in the threats it sees.
    #[test]
    fn a_wounded_unit_is_weighed_as_it_fights() {
        let terrain = format!("20\n20\n{}\n", vec!["g".repeat(20); 20].join("\n"));
        let units = "unit soldiers\nname Soldiers\nstrength 4\nmoves 12\nenter grass 2\n\
                     unit knight\nname Knight\nstrength 6\nmoves 12\nenter grass 2\n";
        let (map, warnings) = scenario::parse(&terrain, Some(units), None).unwrap();
        let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
        let put = |game: &mut Game, side: Side, kind: &str, hex: Hex| {
            let kind = game.map.units().kind_named(kind).unwrap();
            let id = game.map.new_unit_of(side, kind).unwrap();
            game.map.place_unit(id, hex).unwrap();
            id
        };
        let mut units: Vec<UnitId> =
            (0..MAX_STACK).map(|i| put(&mut game, Side::Red, "soldiers", Hex::new(3, i as u16))).collect();
        let knight = put(&mut game, Side::Red, "knight", Hex::new(4, 4));
        units.push(knight);
        let foe = put(&mut game, Side::Blue, "knight", Hex::new(10, 11));
        let target = Hex::new(10, 10);
        let sight = game.sight(Side::Red);
        assert!(strongest_stack(&game, units.clone(), target).contains(&knight), "whole, the strongest");
        let whole = threat(&game, Side::Red, target, &sight);
        game.map.unit_mut(knight).unwrap().wounds = 1;
        game.map.unit_mut(foe).unwrap().wounds = 1;
        assert!(!strongest_stack(&game, units, target).contains(&knight), "wounded, a 3: left out");
        assert!(threat(&game, Side::Red, target, &sight) < whole, "a wounded foe is less of a threat");
    }
}
