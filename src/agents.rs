//! The computer's players: agents. An agent is asked, again and again, for
//! the next order of the side whose turn it is, until it says the turn is
//! over; the game carries each order out (`Game::apply`) or refuses it. The
//! same agents play in the window and without one (`hexlords --headless`), so
//! games can be played many times over, to try the rules. An agent can also
//! be a program of its own, which reads the game as text and writes its
//! orders as text (`docs/agents.md`): a script, or a player driven by
//! something else altogether.
//!
//! Not a port: the C++ had no players.

use std::collections::VecDeque;
use std::io::Write;
use std::process::{Command, Stdio};

use crate::game::Game;
use crate::orders::{Event, Order, RALLY_POINTS, RallyPoint};
use crate::setup::Controller;
use crate::state::combat::System;
use crate::state::scenario::{side_name, side_named};
use crate::state::{CityId, Hex, MAX_STACK, QueueEnd, QueueItem, Sends, Side, UnitId};

/// What a side's agent is asked for.
pub trait Agent {
    /// The next order of `side`, whose turn it is in `game`: `Order::EndTurn`
    /// when it has done.
    fn next_order(&mut self, game: &Game, side: Side) -> Order;

    /// The game refused the last order it gave, and why (something it could
    /// not see may be in the way). By default it hears nothing.
    fn refused(&mut self, _order: &Order, _why: &str) {}
}

/// The agents there are, by name (and `cmd:<program> [<argument> ...]`, a
/// program that plays: `docs/agents.md`).
pub const NAMES: [&str; 5] = ["passive", "simple", "steady", "bold", "careful"];

/// After a planner's name (`steady/turns`): it plays a game of rounds as it
/// plays turns, not in its way for rounds (`planner::rounds`). For playing
/// the two against each other.
pub const TURNWISE: &str = "/turns";

/// At most this many orders in a turn: an agent that never ends its turn has
/// it ended for it.
pub const MAX_ORDERS: usize = 1000;

/// The agent of a name.
pub fn by_name(name: &str) -> Result<Box<dyn Agent>, String> {
    if let Some(command) = name.strip_prefix("cmd:") {
        let mut words = command.split_whitespace().map(str::to_string);
        let program = words.next().ok_or("cmd: needs a program")?;
        return Ok(Box::new(Program { program, args: words.collect(), queue: VecDeque::new(), turn: None }));
    }
    match name {
        "passive" => Ok(Box::new(Passive)),
        "simple" => Ok(Box::new(Simple::default())),
        name if crate::planner::style(name).is_some() => {
            Ok(Box::new(crate::planner::Planner::new(crate::planner::style(name).unwrap_or(crate::planner::STEADY))))
        }
        // (A planner that plays rounds as it plays turns: to measure the
        // planners' way with rounds against.)
        name if name.strip_suffix(TURNWISE).and_then(crate::planner::style).is_some() => {
            let style = name.strip_suffix(TURNWISE).and_then(crate::planner::style);
            Ok(Box::new(crate::planner::Planner::turnwise(style.unwrap_or(crate::planner::STEADY))))
        }
        _ => Err(format!("\"{name}\" is not an agent ({}, cmd:<program>)", NAMES.join(", "))),
    }
}

/// The agents of a game's computer sides.
pub struct Agents {
    agents: Vec<(Side, Box<dyn Agent>)>,
}

impl Agents {
    /// An agent for every side the computer plays. An error names an agent
    /// there is not.
    pub fn for_game(game: &Game) -> Result<Agents, String> {
        let mut agents = Vec::new();
        for seat in &game.setup().seats {
            if let Controller::Computer(name) = &seat.controller {
                agents.push((seat.side, by_name(name)?));
            }
        }
        Ok(Agents { agents })
    }

    /// The agent that plays `side`, if the computer plays it.
    pub fn get_mut(&mut self, side: Side) -> Option<&mut dyn Agent> {
        self.agents.iter_mut().find(|(s, _)| *s == side).map(|(_, a)| a.as_mut() as &mut dyn Agent)
    }
}

/// What came of a turn an agent played.
#[derive(Clone, Debug, Default)]
pub struct TurnReport {
    pub events: Vec<Event>,
    /// The orders the game refused, and why.
    pub refused: Vec<(Order, String)>,
}

/// A whole turn of the side whose turn it is, played by `agent`: the agent
/// gives its orders, the computer leads its marauding stacks
/// (`marauder::maraud`), then the turn ends (where its marches and patrols
/// go). The agent is shown the game as its
/// side knows it (`Game::view_for`), worked out anew for each order; the
/// orders are carried out in the game itself, where something it could not
/// see may be in the way (it hears of a refusal: `Agent::refused`).
pub fn play_turn(game: &mut Game, agent: &mut dyn Agent) -> TurnReport {
    let mut report = TurnReport::default();
    let mut given = 0;
    loop {
        let (step, over) = play_step(game, agent, given);
        given += 1;
        report.events.extend(step.events);
        report.refused.extend(step.refused);
        if over {
            return report;
        }
    }
}

/// A step of the turn of the side whose turn it is, played by `agent`, for
/// a screen that shows it as it goes (`play_turn` plays them all): the
/// agent's next order, carried out (or refused); or, when it ends its turn
/// (or has given `MAX_ORDERS`, `given` being how many it has), the
/// computer leading its marauding stacks and the turn's end. True once the
/// turn is over.
pub fn play_step(game: &mut Game, agent: &mut dyn Agent, given: usize) -> (TurnReport, bool) {
    let side = game.player();
    let mut report = TurnReport::default();
    if game.winner().is_some() {
        return (report, true);
    }
    if given < MAX_ORDERS {
        let view = game.view_for(side);
        let order = agent.next_order(&view, side);
        if order != Order::EndTurn {
            match game.apply(order.clone()) {
                Ok(events) => report.events.extend(events),
                Err(why) => {
                    agent.refused(&order, &why);
                    report.refused.push((order, why));
                }
            }
            let over = game.winner().is_some() || game.player() != side;
            return (report, over);
        }
    }
    // Its marauding stacks last (`marauder`), whoever plays the side.
    if game.winner().is_none() && game.player() == side {
        let led = crate::marauder::maraud(game, side);
        report.events.extend(led.events);
        report.refused.extend(led.refused);
    }
    if game.winner().is_none() && game.player() == side {
        report.events.extend(game.end_turn());
    }
    (report, true)
}

/// The computer's turns, one side after the other, until it is a person's
/// turn or the game is over, and at most a round of them (so that a game the
/// computer plays alone goes on a round at a time).
pub fn play_computers(game: &mut Game, agents: &mut Agents) -> TurnReport {
    let mut report = TurnReport::default();
    // (A turn each of the sides still in: those out have none.)
    let sides = game.order().iter().filter(|s| !game.out().contains(s)).count();
    for _ in 0..sides {
        if game.winner().is_some() {
            break;
        }
        let side = game.player();
        let Some(agent) = agents.get_mut(side) else { break };
        let turn = play_turn(game, agent);
        report.events.extend(turn.events);
        report.refused.extend(turn.refused);
    }
    report
}

/// Ends every turn at once.
pub struct Passive;

impl Agent for Passive {
    fn next_order(&mut self, _game: &Game, _side: Side) -> Order {
        Order::EndTurn
    }
}

/// A first opponent, and a yardstick. Its cities build what gives the most
/// strength for the turns it takes (the cheapest, of two alike), of what its
/// taxes can keep, and stop when they cannot keep more. It keeps
/// [`Simple::KEEP`] units in each of its cities, and sends the others of a
/// stack together toward the nearest city that is not its own; it attacks
/// when the odds are at least [`Simple::DARE`] in a hundred, and else waits
/// where it is for more to come.
#[derive(Default)]
pub struct Simple {
    /// The turn it is playing (round and side), and the units it has given an
    /// order this turn.
    turn: Option<(u32, Side)>,
    done: Vec<UnitId>,
    /// Units it sent to a temple this turn: they seek its blessing there.
    pilgrims: Vec<UnitId>,
    /// How many steps each unit has taken this turn to see more (total fog
    /// of war): a few, while it has the moves.
    explored: Vec<(UnitId, u32)>,
}

impl Simple {
    /// How many steps a turn a stack takes to see more, at the most.
    const SCOUT_STEPS: u32 = 3;
    /// How many units stay in each of its cities.
    pub const KEEP: usize = 2;
    /// The odds, in percent, it attacks at; and at which it attacks with
    /// more than a stack in reach, the first of several.
    pub const DARE: u32 = 60;
    pub const WEAR: u32 = 15;
}

/// Of these units, as many as attack together (a hex's worth: the
/// strongest [`MAX_STACK`] and a commander), the strongest first.
fn strongest_stack(game: &Game, mut units: Vec<UnitId>) -> Vec<UnitId> {
    let map = &game.map;
    units.sort_by_key(|id| std::cmp::Reverse(map.units().get(*id).map_or(0, |u| u.strength)));
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

impl Agent for Simple {
    fn next_order(&mut self, game: &Game, side: Side) -> Order {
        if self.turn != Some((game.turn(), side)) {
            self.turn = Some((game.turn(), side));
            self.done.clear();
            self.explored.clear();
            self.pilgrims.clear();
        }
        if let Some(order) = blessing(game, side, &mut self.pilgrims) {
            return order;
        }
        let map = &game.map;
        // What its taxes leave after the upkeep of its units and of one more
        // of each kind its cities are building: what it can keep more.
        let own = || map.cities().iter().filter(|c| c.side == side);
        let planned: u32 = own().filter_map(|c| c.production()).map(|p| p.upkeep).sum();
        let spare = i64::from(game.income(side)) - i64::from(game.upkeep(side)) - i64::from(planned);
        if spare < 0 {
            // More than it can keep: the dearest work stops.
            let dearest = own().filter(|c| c.production().is_some()).max_by_key(|c| c.production().map(|p| p.upkeep));
            if let Some(city) = dearest {
                return Order::Build { city: city.id, what: None };
            }
        }
        // A side without a commander builds one, where it can, if it can
        // keep it.
        let units = map.units();
        let leads = |p: &crate::state::UnitProd| {
            units.kind_named(&p.basename).and_then(|k| units.kind(k)).is_some_and(|k| k.commander > 0)
        };
        let led = units.of_side(side).any(|u| map.is_commander(u.id()));
        let building_one = own().any(|c| c.production().is_some_and(&leads));
        if !led && !building_one {
            let at = own().find_map(|city| {
                let i = city.units.iter().position(|p| leads(p) && i64::from(p.upkeep) <= spare)?;
                Some((city.id, i))
            });
            if let Some((city, i)) = at {
                return Order::Build { city, what: Some(i) };
            }
        }
        // An idle city builds what gives the most strength for its turns, of
        // what the side can keep (no ships: it does not carry armies by sea).
        let ship = |p: &crate::state::UnitProd| {
            units
                .kind_named(&p.basename)
                .and_then(|k| units.kind(k))
                .is_some_and(|k| k.travel == crate::state::Travel::Sea)
        };
        let choice = own().filter(|c| c.building.is_none()).find_map(|city| {
            let best = city
                .units
                .iter()
                .enumerate()
                .filter(|(_, p)| i64::from(p.upkeep) <= spare && !ship(p))
                .max_by_key(|(_, p)| (p.strength * 100 / p.time.max(1), std::cmp::Reverse(p.cost)));
            best.map(|(i, _)| (city.id, i))
        });
        if let Some((city, i)) = choice {
            return Order::Build { city, what: Some(i) };
        }
        // (What it knows of: with total fog of war, not the cities of land
        // it has never seen.)
        let sight = game.sight(side);
        let targets: Vec<Hex> =
            map.cities().iter().filter(|c| c.side != side && sight.knows(c.pos)).map(|c| c.pos).collect();
        // The units free to go: not given an order this turn, with points
        // left, and not the first KEEP of one of its cities.
        let keeps = |id: UnitId, at: Hex| {
            map.city_at(at).is_some_and(|c| c.side == side)
                && map.units_at(at).iter().take(Simple::KEEP).any(|k| *k == id)
        };
        let free: Vec<&crate::state::Unit> = map
            .units()
            .of_side(side)
            .filter(|u| !self.done.contains(&u.id()) && u.movement > 0 && !keeps(u.id(), u.pos()))
            .collect();
        if free.is_empty() {
            return Order::EndTurn;
        }
        // An attack: all the free units that can get to a target this turn,
        // from wherever they are, if together they are likely to win.
        let ranges: Vec<_> = free.iter().map(|u| (u.id(), game.known_range(u, &sight))).collect();
        // The cities first; then the other sides' stacks out in the field
        // near its free units (in the way, it may be).
        let mut field: Vec<Hex> = map
            .units()
            .iter()
            .filter(|u| u.side() != side && map.contains(u.pos()) && map.city_at(u.pos()).is_none())
            .map(|u| u.pos())
            .filter(|h| free.iter().any(|u| u.pos().distance(*h) <= 4))
            .collect();
        field.sort_by_key(|h| (h.row, h.col));
        field.dedup();
        for target in targets.iter().chain(&field) {
            let able: Vec<UnitId> =
                ranges.iter().filter(|(_, r)| r.cost(*target).is_some()).map(|(id, _)| *id).collect();
            let more = able.len();
            let able = strongest_stack(game, able);
            if able.is_empty() {
                continue;
            }
            // Likely to win; or with more than a stack to send, likely
            // enough to wear the defenders down for the next.
            let forecast = game.battle_forecast(&able, *target);
            let odds = forecast.map_or(100, |f| f.odds);
            if odds >= Simple::DARE
                || (more > able.len() && odds >= Simple::WEAR)
                || siege(game, side, *target, forecast)
            {
                self.done.extend(able.iter().copied());
                return Order::Move { units: able, to: *target };
            }
        }
        // Else each stack's free units go together toward the nearest target
        // they can get to; those already near a target wait for the others.
        let temples: Vec<Hex> = (0..map.rows())
            .flat_map(|row| (0..map.cols()).map(move |col| Hex::new(row, col)))
            .filter(|h| map.tile(*h).is_some_and(|t| t.kind == crate::state::TileType::Temple))
            .collect();
        let mut seen: Vec<Hex> = Vec::new();
        for unit in &free {
            let here = unit.pos();
            if seen.contains(&here) {
                continue;
            }
            seen.push(here);
            let stack: Vec<&crate::state::Unit> = free.iter().copied().filter(|u| u.pos() == here).collect();
            let ids: Vec<UnitId> = stack.iter().map(|u| u.id()).collect();
            self.done.extend(ids.iter().copied());
            // A temple all of them can get to this turn, that has not blessed
            // some of them: there first, and they seek its blessing.
            let reach = |hex: Hex| ids.iter().all(|id| ranges.iter().any(|(u, r)| u == id && r.cost(hex).is_some()));
            if let Some(temple) = temples.iter().copied().find(|t| {
                *t != here && stack.iter().any(|u| !u.blessed_at(*t)) && reach(*t) && map.has_room(*t, ids[0])
            }) {
                self.pilgrims.extend(ids.iter().copied());
                return Order::Move { units: ids, to: temple };
            }
            let lead = stack.iter().copied().min_by_key(|u| u.movement).unwrap_or(unit);
            let mut near = targets.clone();
            near.sort_by_key(|t| (t.distance(here), *t));
            let room = |hex: Hex| map.has_room(hex, lead.id());
            let way = |to: Hex| game.known_step_toward(lead, to, &room, &sight).is_some();
            let Some(target) = near.into_iter().find(|t| way(*t)) else {
                // Nowhere it knows of to go: it goes to see more of the map
                // (total fog of war), to the nearest edge of what it knows.
                let edge = game.frontier_for(lead, &sight);
                if let Some((to, _)) = edge.into_iter().filter(|(h, _)| *h != here).take(6).find(|(h, _)| way(*h)) {
                    // In rounds: on past the edge, as far as they get this
                    // round (one plan a round).
                    if game.rounds() {
                        let to = crate::planner::outward(game, &sight, lead, to);
                        return Order::Toward { units: ids, to };
                    }
                    let steps = self.explored.iter().filter(|(id, _)| *id == lead.id()).count() as u32;
                    self.explored.push((lead.id(), steps + 1));
                    if steps + 1 < Simple::SCOUT_STEPS {
                        self.done.retain(|id| !ids.contains(id));
                    }
                    return Order::Toward { units: ids, to };
                }
                continue;
            };
            let waits = ids.iter().all(|id| ranges.iter().any(|(u, r)| u == id && r.cost(target).is_some()));
            if !waits {
                return Order::Toward { units: ids, to: target };
            }
        }
        Order::EndTurn
    }
}

/// A program that plays: at the start of each of its turns it is run, reads
/// the game as text on its input (`describe`), and writes its orders on its
/// output, a line each (`parse_order`). What it gets wrong is said on the
/// error output, and the turn goes on without it.
struct Program {
    program: String,
    args: Vec<String>,
    queue: VecDeque<Order>,
    turn: Option<(u32, Side)>,
}

impl Program {
    fn ask(&self, game: &Game, side: Side) -> Result<Vec<String>, String> {
        let mut child = Command::new(&self.program)
            .args(&self.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| format!("{}: {e}", self.program))?;
        let text = describe(game, side);
        child.stdin.take().ok_or("no input")?.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
        let output = child.wait_with_output().map_err(|e| e.to_string())?;
        Ok(String::from_utf8_lossy(&output.stdout).lines().map(str::to_string).collect())
    }
}

impl Agent for Program {
    fn next_order(&mut self, game: &Game, side: Side) -> Order {
        if self.turn != Some((game.turn(), side)) {
            self.turn = Some((game.turn(), side));
            self.queue.clear();
            match self.ask(game, side) {
                Ok(lines) => {
                    for line in lines {
                        match parse_order(game, &line) {
                            Ok(Some(order)) => self.queue.push_back(order),
                            Ok(None) => {}
                            Err(why) => eprintln!("{}: \"{line}\": {why}", self.program),
                        }
                    }
                }
                Err(why) => eprintln!("{why}"),
            }
        }
        self.queue.pop_front().unwrap_or(Order::EndTurn)
    }
}

/// The blessing a stack sent to a temple seeks on getting there (an agent's
/// `pilgrims`, emptied as it goes): those on a temple that has not blessed
/// them all.
pub fn blessing(game: &Game, side: Side, pilgrims: &mut Vec<UnitId>) -> Option<Order> {
    let map = &game.map;
    while let Some(first) = pilgrims.first().copied() {
        let at = map.units().get(first).filter(|u| u.side() == side).map(|u| u.pos());
        let here: Vec<UnitId> =
            pilgrims.iter().copied().filter(|id| map.units().get(*id).is_some_and(|u| Some(u.pos()) == at)).collect();
        pilgrims.retain(|id| !here.contains(id) && *id != first);
        let Some(at) = at else { continue };
        let temple = map.tile(at).is_some_and(|t| t.kind == crate::state::TileType::Temple);
        if temple && here.iter().any(|id| map.units().get(*id).is_some_and(|u| !u.blessed_at(at))) {
            return Some(Order::Bless { units: here });
        }
    }
    None
}

/// A siege's wave goes at odds of `SIEGE_ODDS` percent or more, or if it is
/// likely to kill `SIEGE_KILLS` defenders: a wave that dies and kills
/// nobody (8 against 1 elite behind walls, in the simulations) only makes
/// the defenders rise in level, and the side waits for stronger units.
pub const SIEGE_ODDS: u32 = 5;
pub const SIEGE_KILLS: f64 = 1.0;
/// How many rounds the holder's units it saw and sees no more count in a
/// siege's sum (fog of war).
pub const SIEGE_MEMORY: u32 = 5;

/// True if `side` so outnumbers the holder of the city at `target` (three
/// times its units and its cities) that it takes the city in waves, each
/// likely enough to win or to kill defenders (see `SIEGE_ODDS`): a last
/// city held by a full stack behind high walls would else never fall.
/// (`forecast`: the wave's, `Game::battle_forecast`.)
pub fn siege(game: &Game, side: Side, target: Hex, forecast: Option<crate::state::combat::Forecast>) -> bool {
    if forecast.is_none_or(|f| f.odds < SIEGE_ODDS && f.kills < SIEGE_KILLS) {
        return false;
    }
    let map = &game.map;
    let Some(holder) = map.city_at(target).map(|c| c.side).filter(|s| *s != side && *s != Side::Grey) else {
        return false;
    };
    let units = |s: Side| map.units().of_side(s).count();
    let cities = |s: Side| map.cities().iter().filter(|c| c.side == s).count();
    // (With fog of war its view holds only the holder's units it sees: and
    // those it saw lately, and sees no more, count too; not those seen long
    // ago, which may be dead or anywhere: under total fog they barred every
    // siege.)
    let sight = game.sight(side);
    let lately = |round: u32| round + SIEGE_MEMORY >= game.turn();
    let unseen: usize = sight
        .ghosts()
        .iter()
        .filter(|g| g.side == holder && !sight.sees(g.at) && lately(g.round))
        .map(|g| g.count)
        .sum();
    let theirs = units(holder) + unseen;
    units(side) >= 3 * theirs.max(1) && cities(side) >= 3 * cities(holder).max(1)
}

/// A unit's name in the text: its side, its slot and the slot's generation,
/// `red.3.0`.
pub fn unit_name(id: UnitId) -> String {
    format!("{}.{}.{}", side_name(id.side), id.index, id.generation)
}

/// The unit a name in the text names (see [`unit_name`]).
pub fn parse_unit(name: &str) -> Option<UnitId> {
    let mut parts = name.split('.');
    let side = side_named(parts.next()?)?;
    let index = parts.next()?.parse().ok()?;
    let generation = parts.next()?.parse().ok()?;
    parts.next().is_none().then_some(UnitId { side, index, generation })
}

/// The game as a program that plays `side` reads it (`docs/agents.md`).
pub fn describe(game: &Game, side: Side) -> String {
    let map = &game.map;
    let mut out = format!("turn {} {}\n", game.turn(), side_name(side));
    // (In rounds: the orders to move are plans, and the round plays out
    // once every side has given its own.)
    if game.rounds() {
        out.push_str("mode rounds\n");
    }
    out.push_str(&format!("order {}\n", game.order().iter().map(|s| side_name(*s)).collect::<Vec<_>>().join(" ")));
    let combat = game.setup().combat;
    let exact = if combat.system == System::Dice { format!(" exact {}", combat.exact.name()) } else { String::new() };
    let wounds = if combat.wounds { " wounds last" } else { "" };
    let back = if combat.blows_back { " blows back" } else { "" };
    let cap = combat.cap.map_or(String::new(), |cap| format!(" cap {cap}"));
    out.push_str(&format!("battles {} chance {}{exact}{wounds}{back}{cap}\n", combat.system.name(), combat.chance));
    out.push_str(&format!("fog {}\n", game.setup().fog.word()));
    out.push_str(&format!("gold {} income {} upkeep {}\n", game.gold(side), game.income(side), game.upkeep(side)));
    // With total fog of war, a hex the side has never seen is a `?`.
    let sight = game.sight(side);
    out.push_str("terrain\n");
    for (i, line) in map.to_text().lines().enumerate() {
        let Some(row) = i.checked_sub(2) else {
            out.push_str(line);
            out.push('\n');
            continue;
        };
        let known = |(col, c): (usize, char)| if sight.knows(Hex::new(row as u16, col as u16)) { c } else { '?' };
        out.extend(line.chars().enumerate().map(known));
        out.push('\n');
    }
    out.push_str("end terrain\n");
    for kind in map.units().kinds() {
        let prices: Vec<String> =
            kind.cost.prices().iter().filter(|(_, p)| *p > 0).map(|(w, p)| format!("{w} {p}")).collect();
        out.push_str(&format!(
            "kind {} strength {} moves {} upkeep {} price {} commander {} travels {} carries {} siege {} bonus {} \
             enter {}\n",
            kind.id,
            kind.strength,
            kind.moves,
            kind.upkeep,
            kind.price,
            kind.commander,
            kind.travel.word(),
            kind.carries,
            kind.siege,
            if kind.bonus.is_empty() {
                "none".to_string()
            } else {
                kind.bonus.iter().map(|(g, n)| format!("{g}:{n}")).collect::<Vec<_>>().join(",")
            },
            prices.join(" ")
        ));
    }
    for city in map.cities().iter().filter(|c| sight.knows(c.pos)) {
        let builds = city.building.map_or("none".to_string(), |i| i.to_string());
        out.push_str(&format!(
            "city {} {} {} walls {} income {} builds {} progress {} name {}\n",
            city.pos.row,
            city.pos.col,
            side_name(city.side),
            city.walls,
            city.income,
            builds,
            city.progress,
            city.name
        ));
        if city.side == side {
            for (i, prod) in city.units.iter().enumerate() {
                out.push_str(&format!(
                    "can {} {} {} {} {} {}\n",
                    city.pos.row, city.pos.col, i, prod.basename, prod.time, prod.cost
                ));
            }
        }
    }
    for unit in map.units().iter().filter(|u| map.contains(u.pos())) {
        let kind = map.units().kind_of(unit).map_or("-", |k| k.id.as_str());
        out.push_str(&format!(
            "unit {} {} {} {} {} strength {} moves {}/{} level {}\n",
            unit_name(unit.id()),
            side_name(unit.side()),
            unit.pos().row,
            unit.pos().col,
            kind,
            unit.power(),
            unit.movement,
            unit.base_move_points,
            unit.level
        ));
        if unit.wounds > 0 {
            out.pop();
            out.push_str(&format!(" wounds {}\n", unit.wounds));
        }
    }
    out.push_str("end\n");
    out
}

/// An item of a city's queue in a word: `<what>x<count>[@<where>]`.
fn queue_item(game: &Game, word: &str) -> Result<QueueItem, String> {
    let bad = || format!("\"{word}\" is not an item (<what>x<count>[@<where>])");
    let (what, to) = word.split_once('@').map_or((word, None), |(w, t)| (w, Some(t)));
    let (what, count) = what.split_once('x').ok_or_else(bad)?;
    let (what, count) = (what.parse().map_err(|_| bad())?, count.parse().map_err(|_| bad())?);
    let sends = match to {
        None | Some("stay") => Sends::Stay,
        Some("defend") => Sends::Defend,
        Some(point) if point.starts_with("point") => {
            let (n, bless) = point["point".len()..]
                .split_once('+')
                .map_or((&point["point".len()..], false), |(n, b)| (n, b == "bless"));
            let n = n.parse::<u8>().ok().filter(|n| usize::from(*n) < RALLY_POINTS).ok_or_else(bad)?;
            Sends::Point { n, bless }
        }
        Some(route) => {
            let hex = |w: &str| -> Option<Hex> {
                let (row, col) = w.split_once('.')?;
                Some(Hex::new(row.parse().ok()?, col.parse().ok()?)).filter(|h| game.map.contains(*h))
            };
            Sends::Route(route.split(',').map(hex).collect::<Option<Vec<Hex>>>().ok_or_else(bad)?)
        }
    };
    Ok(QueueItem { what, count, sends })
}

/// An order as a line of text that `parse_order` reads as the same order
/// (`docs/agents.md`): what a saved game keeps of it (`save`). A city is
/// named by its hex, so `game` is the game the order was given in.
pub fn order_text(game: &Game, order: &Order) -> String {
    let units = |ids: &[UnitId]| match ids {
        [] => "-".to_string(),
        ids => ids.iter().map(|id| unit_name(*id)).collect::<Vec<_>>().join(","),
    };
    let hex = |h: &Hex| format!("{} {}", h.row, h.col);
    let hexes = |hs: &[Hex]| hs.iter().map(hex).collect::<Vec<_>>().join(" ");
    // (A city that is not there: a hex off the map, which no reader takes.)
    let city = |id: &CityId| game.map.cities().get(*id).map_or("65535 65535".to_string(), |c| hex(&c.pos));
    let sends = |to: &Sends| match to {
        Sends::Stay => "stay".to_string(),
        Sends::Defend => "defend".to_string(),
        Sends::Point { n, bless } => format!("point{n}{}", if *bless { "+bless" } else { "" }),
        Sends::Route(stops) => stops.iter().map(|h| format!("{}.{}", h.row, h.col)).collect::<Vec<_>>().join(","),
    };
    match order {
        Order::Undo => "undo".to_string(),
        Order::EndTurn => "end".to_string(),
        Order::Move { units: u, to } => format!("move {} {}", units(u), hex(to)),
        Order::Toward { units: u, to } => format!("toward {} {}", units(u), hex(to)),
        Order::Onward { units: u, to } => format!("onward {} {}", units(u), hex(to)),
        Order::Roam { units: u, to } => format!("roam {} {}", units(u), hex(to)),
        Order::GoTo { units: u, to, via } => {
            format!("goto {} {}", units(u), hexes(&via.iter().chain([to]).copied().collect::<Vec<_>>()))
        }
        Order::Done { units: u } => format!("done {}", units(u)),
        Order::Defend { units: u, on } => format!("{} {}", if *on { "defend" } else { "undefend" }, units(u)),
        Order::Maraud { units: u, on } => format!("{} {}", if *on { "maraud" } else { "unmaraud" }, units(u)),
        Order::Bless { units: u } => format!("bless {}", units(u)),
        Order::Disband { units: u } => format!("disband {}", units(u)),
        Order::Build { city: c, what } => {
            format!("build {} {}", city(c), what.map_or("none".to_string(), |n| n.to_string()))
        }
        Order::Walls { city: c } => format!("walls {}", city(c)),
        Order::Rally { city: c, to } => match to {
            Sends::Stay => format!("rally {} none", city(c)),
            Sends::Defend => format!("rally {} defend", city(c)),
            Sends::Point { n, bless } => format!("rally {} point {n}{}", city(c), if *bless { " bless" } else { "" }),
            Sends::Route(stops) => format!("rally {} {}", city(c), hexes(stops)),
        },
        Order::RallyPoint { n, point: None } => format!("point {n} none"),
        Order::RallyPoint { n, point: Some(point) } => {
            format!("point {n} {}{}", hex(&point.at), if point.defend { " defend" } else { "" })
        }
        Order::Queue { city: c, items, .. } if items.is_empty() => format!("queue {} none", city(c)),
        Order::Queue { city: c, items, end } => {
            let items: Vec<String> =
                items.iter().map(|item| format!("{}x{}@{}", item.what, item.count, sends(&item.sends))).collect();
            let end = match end {
                QueueEnd::Stop => String::new(),
                QueueEnd::Repeat { from, times: None } => format!(" repeat {from}"),
                QueueEnd::Repeat { from, times: Some(times) } => format!(" repeat {from} {times}"),
            };
            format!("queue {} {}{end}", city(c), items.join(" "))
        }
        Order::Patrol { units: u, waypoints, attack } => {
            let attack = attack.map_or(String::new(), |odds| format!(" attack {odds}"));
            format!("patrol {} {}{attack}", units(u), hexes(waypoints))
        }
    }
}

/// An order in a line of text (`docs/agents.md`): `move <units> <row> <col>`,
/// `toward <units> <row> <col>` (units: names with commas between),
/// `goto <units> <row> <col> [<row> <col> ...]` (stops in order, the last
/// where they end), `done <units>`, `defend <units>`, `undefend <units>`,
/// `bless <units>`, `disband <units>`, `build <row> <col> <index or none>`,
/// `rally <row> <col> <row> <col> [<row> <col> ...]` (or `none`, `defend`,
/// `point <n> [bless]`), `point <n> <row> <col> [defend]` (or
/// `point <n> none`),
/// `queue <row> <col> <item> [<item> ...] [repeat <from> [<times>]]` (an
/// item `<what>x<count>[@<where>]`, where `defend`, `point<n>`,
/// `point<n>+bless` or `<row>.<col>[,<row>.<col> ...]`; or
/// `queue <row> <col> none`), `walls <row> <col>`, `undo`, `end`;
/// `patrol <units> <row> <col> <row> <col> [<row> <col> ...] [attack <percent>]`
/// (waypoints in order), `maraud <units>`, `unmaraud <units>`,
/// `roam <units> <row> <col>`. `None`
/// for an empty line or a comment (`#`).
pub fn parse_order(game: &Game, line: &str) -> Result<Option<Order>, String> {
    let words: Vec<&str> = line.split_whitespace().collect();
    let hex = |row: &str, col: &str| -> Result<Hex, String> {
        let (row, col) = (row.parse().map_err(|_| "not a row")?, col.parse().map_err(|_| "not a column")?);
        let hex = Hex::new(row, col);
        if game.map.contains(hex) { Ok(hex) } else { Err(format!("{hex} is not on the map")) }
    };
    let units = |names: &str| -> Result<Vec<UnitId>, String> {
        // (`-`: none of them, as `order_text` writes an empty list.)
        if names == "-" {
            return Ok(Vec::new());
        }
        names.split(',').map(|name| parse_unit(name).ok_or(format!("\"{name}\" is not a unit's name"))).collect()
    };
    // A rally point's number, 0 to 9.
    let point = |n: &str| -> Result<u8, String> {
        n.parse::<u8>()
            .ok()
            .filter(|n| usize::from(*n) < RALLY_POINTS)
            .ok_or(format!("\"{n}\" is not a rally point (0 to 9)"))
    };
    // Stops: rows and columns in pairs, the last where they end.
    let route = |words: &[&str]| -> Result<(Hex, Vec<Hex>), String> {
        if words.is_empty() || words.len() % 2 != 0 {
            return Err("a route is rows and columns in pairs".to_string());
        }
        let mut stops: Vec<Hex> = words.chunks(2).map(|w| hex(w[0], w[1])).collect::<Result<_, _>>()?;
        let to = stops.pop().ok_or("no stops")?;
        Ok((to, stops))
    };
    Ok(Some(match words[..] {
        [] => return Ok(None),
        [first, ..] if first.starts_with('#') => return Ok(None),
        ["undo"] => Order::Undo,
        ["end"] => Order::EndTurn,
        ["move", u, row, col] => Order::Move { units: units(u)?, to: hex(row, col)? },
        ["toward", u, row, col] => Order::Toward { units: units(u)?, to: hex(row, col)? },
        ["onward", u, row, col] => Order::Onward { units: units(u)?, to: hex(row, col)? },
        ["build", row, col, what] => {
            let city = game.map.city_at(hex(row, col)?).ok_or("there is no city there")?.id;
            let what = match what {
                "none" => None,
                n => Some(n.parse().map_err(|_| format!("\"{n}\" is not one of what it can build"))?),
            };
            Order::Build { city, what }
        }
        ["goto", u, ref stops @ ..] if !stops.is_empty() => {
            let (to, via) = route(stops)?;
            Order::GoTo { units: units(u)?, to, via }
        }
        ["done", u] => Order::Done { units: units(u)? },
        ["defend", u] => Order::Defend { units: units(u)?, on: true },
        ["undefend", u] => Order::Defend { units: units(u)?, on: false },
        ["bless", u] => Order::Bless { units: units(u)? },
        ["disband", u] => Order::Disband { units: units(u)? },
        ["walls", row, col] => {
            Order::Walls { city: game.map.city_at(hex(row, col)?).ok_or("there is no city there")?.id }
        }
        ["rally", row, col, ref to @ ..] if !to.is_empty() => {
            let city = game.map.city_at(hex(row, col)?).ok_or("there is no city there")?.id;
            let to = match to {
                ["none"] => Sends::Stay,
                ["defend"] => Sends::Defend,
                ["point", n] => Sends::Point { n: point(n)?, bless: false },
                ["point", n, "bless"] => Sends::Point { n: point(n)?, bless: true },
                stops => {
                    let (to, via) = route(stops)?;
                    Sends::Route(via.into_iter().chain([to]).collect())
                }
            };
            Order::Rally { city, to }
        }
        ["queue", row, col, "none"] => {
            let city = game.map.city_at(hex(row, col)?).ok_or("there is no city there")?.id;
            Order::Queue { city, items: Vec::new(), end: QueueEnd::Stop }
        }
        ["queue", row, col, ref rest @ ..] if !rest.is_empty() => {
            let city = game.map.city_at(hex(row, col)?).ok_or("there is no city there")?.id;
            let (items, end) = match rest.iter().position(|w| *w == "repeat") {
                Some(i) => {
                    let number = |w: &str| w.parse::<usize>().map_err(|_| format!("\"{w}\" is not a number"));
                    let end = match &rest[i + 1..] {
                        [from] => QueueEnd::Repeat { from: number(from)?, times: None },
                        [from, times] => QueueEnd::Repeat { from: number(from)?, times: Some(number(times)? as u32) },
                        _ => return Err("repeat <from> [<times>]".to_string()),
                    };
                    (&rest[..i], end)
                }
                None => (rest, QueueEnd::Stop),
            };
            let items = items.iter().map(|w| queue_item(game, w)).collect::<Result<Vec<_>, _>>()?;
            Order::Queue { city, items, end }
        }
        ["point", n, "none"] => Order::RallyPoint { n: point(n)?, point: None },
        ["point", n, row, col] => {
            Order::RallyPoint { n: point(n)?, point: Some(RallyPoint { at: hex(row, col)?, defend: false }) }
        }
        ["point", n, row, col, "defend"] => {
            Order::RallyPoint { n: point(n)?, point: Some(RallyPoint { at: hex(row, col)?, defend: true }) }
        }
        ["patrol", u, ref rest @ ..] if !rest.is_empty() => {
            let (places, attack) = match rest {
                [places @ .., "attack", odds] => {
                    (places, Some(odds.parse::<u32>().map_err(|_| format!("\"{odds}\" is not odds in percent"))?))
                }
                places => (places, None),
            };
            if places.is_empty() || places.len() % 2 != 0 {
                return Err("a patrol's waypoints are rows and columns in pairs".to_string());
            }
            let waypoints = places.chunks(2).map(|w| hex(w[0], w[1])).collect::<Result<_, _>>()?;
            Order::Patrol { units: units(u)?, waypoints, attack }
        }
        ["maraud", u] => Order::Maraud { units: units(u)?, on: true },
        ["unmaraud", u] => Order::Maraud { units: units(u)?, on: false },
        ["roam", u, row, col] => Order::Roam { units: units(u)?, to: hex(row, col)? },
        _ => {
            return Err(
                "not an order (move, toward, onward, goto, done, defend, undefend, bless, disband, build, rally, queue, point, walls, undo, end, patrol, maraud, unmaraud, roam)".to_string()
            );
        }
    }))
}
