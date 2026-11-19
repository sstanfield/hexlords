//! The computer's players: agents. An agent is asked, again and again, for
//! the next order of the side whose turn it is, until it says the turn is
//! over; the game carries each order out (`Game::apply`) or refuses it. The
//! same agents play in the window and without one (`hexgame --headless`), so
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
use crate::orders::{Event, Order};
use crate::setup::Controller;
use crate::state::scenario::{side_name, side_named};
use crate::state::{Hex, Side, UnitId, step_toward};

/// What a side's agent is asked for.
pub trait Agent {
    /// The next order of `side`, whose turn it is in `game`: `Order::EndTurn`
    /// when it has done.
    fn next_order(&mut self, game: &Game, side: Side) -> Order;
}

/// The agents there are, by name (and `cmd:<program> [<argument> ...]`, a
/// program that plays: `docs/agents.md`).
pub const NAMES: [&str; 2] = ["passive", "simple"];

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

/// A whole turn of the side whose turn it is, played by `agent`: its orders,
/// then the end of its turn.
pub fn play_turn(game: &mut Game, agent: &mut dyn Agent) -> TurnReport {
    let side = game.player();
    let mut report = TurnReport::default();
    for _ in 0..MAX_ORDERS {
        if game.winner().is_some() || game.player() != side {
            return report;
        }
        let order = agent.next_order(game, side);
        if order == Order::EndTurn {
            break;
        }
        match game.apply(order.clone()) {
            Ok(events) => report.events.extend(events),
            Err(why) => report.refused.push((order, why)),
        }
    }
    if game.winner().is_none() {
        report.events.extend(game.end_turn());
    }
    report
}

/// The computer's turns, one side after the other, until it is a person's
/// turn or the game is over, and at most a round of them (so that a game the
/// computer plays alone goes on a round at a time).
pub fn play_computers(game: &mut Game, agents: &mut Agents) -> TurnReport {
    let mut report = TurnReport::default();
    for _ in 0..game.order().len() {
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

/// A first opponent, and a yardstick: its cities build the cheapest of what
/// they can; one unit stays in each of its cities; the others go for the
/// nearest city they can get to that is not theirs and has nobody in it.
#[derive(Default)]
pub struct Simple {
    /// The turn it is playing (round and side), and the units it has given an
    /// order this turn.
    turn: Option<(u32, Side)>,
    done: Vec<UnitId>,
}

impl Agent for Simple {
    fn next_order(&mut self, game: &Game, side: Side) -> Order {
        if self.turn != Some((game.turn(), side)) {
            self.turn = Some((game.turn(), side));
            self.done.clear();
        }
        let map = &game.map;
        let idle = map.cities().iter().find(|c| c.side == side && c.building.is_none() && !c.units.is_empty());
        if let Some(city) = idle {
            let cheapest = city.units.iter().enumerate().min_by_key(|(_, p)| (p.cost, p.time)).map(|(i, _)| i);
            return Order::Build { city: city.id, what: cheapest };
        }
        let mut targets: Vec<Hex> =
            map.cities().iter().filter(|c| c.side != side && map.units_at(c.pos).is_empty()).map(|c| c.pos).collect();
        for unit in map.units().of_side(side) {
            if self.done.contains(&unit.id()) || unit.movement == 0 {
                continue;
            }
            self.done.push(unit.id());
            let here = unit.pos();
            let keeps =
                map.city_at(here).is_some_and(|c| c.side == side) && map.units_at(here).first() == Some(&unit.id());
            if keeps {
                continue;
            }
            targets.sort_by_key(|t| (t.distance(here), *t));
            if let Some(target) = targets.iter().copied().find(|t| step_toward(map, unit, *t).is_some()) {
                return Order::Toward { unit: unit.id(), to: target };
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
    out.push_str(&format!("order {}\n", game.order().iter().map(|s| side_name(*s)).collect::<Vec<_>>().join(" ")));
    out.push_str("terrain\n");
    out.push_str(&map.to_text());
    out.push_str("end terrain\n");
    for kind in map.units().kinds() {
        let prices: Vec<String> =
            kind.cost.prices().iter().filter(|(_, p)| *p > 0).map(|(w, p)| format!("{w} {p}")).collect();
        out.push_str(&format!(
            "kind {} strength {} moves {} upkeep {} price {} enter {}\n",
            kind.id,
            kind.strength,
            kind.moves,
            kind.upkeep,
            kind.price,
            prices.join(" ")
        ));
    }
    for city in map.cities().iter() {
        let builds = city.building.map_or("none".to_string(), |i| i.to_string());
        out.push_str(&format!(
            "city {} {} {} walls {} builds {} progress {} name {}\n",
            city.pos.row,
            city.pos.col,
            side_name(city.side),
            city.walls,
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
            "unit {} {} {} {} {} strength {} moves {}/{}\n",
            unit_name(unit.id()),
            side_name(unit.side()),
            unit.pos().row,
            unit.pos().col,
            kind,
            unit.strength,
            unit.movement,
            unit.base_move_points
        ));
    }
    out.push_str("end\n");
    out
}

/// An order in a line of text (`docs/agents.md`): `move <unit> <row> <col>`,
/// `toward <unit> <row> <col>`, `build <row> <col> <index or none>`, `end`.
/// `None` for an empty line or a comment (`#`).
pub fn parse_order(game: &Game, line: &str) -> Result<Option<Order>, String> {
    let words: Vec<&str> = line.split_whitespace().collect();
    let hex = |row: &str, col: &str| -> Result<Hex, String> {
        let (row, col) = (row.parse().map_err(|_| "not a row")?, col.parse().map_err(|_| "not a column")?);
        let hex = Hex::new(row, col);
        if game.map.contains(hex) { Ok(hex) } else { Err(format!("{hex} is not on the map")) }
    };
    let unit = |name: &str| parse_unit(name).ok_or(format!("\"{name}\" is not a unit's name"));
    Ok(Some(match words[..] {
        [] => return Ok(None),
        [first, ..] if first.starts_with('#') => return Ok(None),
        ["end"] => Order::EndTurn,
        ["move", u, row, col] => Order::Move { unit: unit(u)?, to: hex(row, col)? },
        ["toward", u, row, col] => Order::Toward { unit: unit(u)?, to: hex(row, col)? },
        ["build", row, col, what] => {
            let city = game.map.city_at(hex(row, col)?).ok_or("there is no city there")?.id;
            let what = match what {
                "none" => None,
                n => Some(n.parse().map_err(|_| format!("\"{n}\" is not one of what it can build"))?),
            };
            Order::Build { city, what }
        }
        _ => return Err("not an order (move, toward, build, end)".to_string()),
    }))
}
