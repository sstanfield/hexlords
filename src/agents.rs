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
use crate::state::{Hex, MAX_STACK, Side, UnitId, compute_range, step_toward};

/// What a side's agent is asked for.
pub trait Agent {
    /// The next order of `side`, whose turn it is in `game`: `Order::EndTurn`
    /// when it has done.
    fn next_order(&mut self, game: &Game, side: Side) -> Order;
}

/// The agents there are, by name (and `cmd:<program> [<argument> ...]`, a
/// program that plays: `docs/agents.md`).
pub const NAMES: [&str; 5] = ["passive", "simple", "steady", "bold", "careful"];

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
/// then the end of its turn. With fog of war the agent is shown the game as
/// its side knows it (`Game::view_for`), worked out anew for each order; the
/// orders are carried out in the game itself, where something it could not
/// see may be in the way.
pub fn play_turn(game: &mut Game, agent: &mut dyn Agent) -> TurnReport {
    let side = game.player();
    let mut report = TurnReport::default();
    for _ in 0..MAX_ORDERS {
        if game.winner().is_some() || game.player() != side {
            return report;
        }
        let view = game.view_for(side);
        let order = agent.next_order(view.as_ref().unwrap_or(game), side);
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
}

impl Simple {
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
        // what the side can keep.
        let choice = own().filter(|c| c.building.is_none()).find_map(|city| {
            let best = city
                .units
                .iter()
                .enumerate()
                .filter(|(_, p)| i64::from(p.upkeep) <= spare)
                .max_by_key(|(_, p)| (p.strength * 100 / p.time.max(1), std::cmp::Reverse(p.cost)));
            best.map(|(i, _)| (city.id, i))
        });
        if let Some((city, i)) = choice {
            return Order::Build { city, what: Some(i) };
        }
        let targets: Vec<Hex> = map.cities().iter().filter(|c| c.side != side).map(|c| c.pos).collect();
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
        let ranges: Vec<_> = free.iter().map(|u| (u.id(), compute_range(map, u))).collect();
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
            let odds = game.battle_odds(&able, *target).unwrap_or(100);
            if odds >= Simple::DARE || (more > able.len() && odds >= Simple::WEAR) {
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
            // A temple all of them can get to this turn, while some of them
            // have not been blessed: there first.
            if stack.iter().any(|u| u.blessing.is_none()) {
                let reach =
                    |hex: Hex| ids.iter().all(|id| ranges.iter().any(|(u, r)| u == id && r.cost(hex).is_some()));
                if let Some(temple) =
                    temples.iter().copied().find(|t| *t != here && reach(*t) && map.has_room(*t, ids[0]))
                {
                    return Order::Move { units: ids, to: temple };
                }
            }
            let lead = stack.iter().copied().min_by_key(|u| u.movement).unwrap_or(unit);
            let mut near = targets.clone();
            near.sort_by_key(|t| (t.distance(here), *t));
            let Some(target) = near.into_iter().find(|t| step_toward(map, lead, *t).is_some()) else { continue };
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
    let combat = game.setup().combat;
    out.push_str(&format!("battles {} chance {}\n", combat.system.name(), combat.chance));
    out.push_str(&format!("gold {} income {} upkeep {}\n", game.gold(side), game.income(side), game.upkeep(side)));
    out.push_str("terrain\n");
    out.push_str(&map.to_text());
    out.push_str("end terrain\n");
    for kind in map.units().kinds() {
        let prices: Vec<String> =
            kind.cost.prices().iter().filter(|(_, p)| *p > 0).map(|(w, p)| format!("{w} {p}")).collect();
        out.push_str(&format!(
            "kind {} strength {} moves {} upkeep {} price {} commander {} enter {}\n",
            kind.id,
            kind.strength,
            kind.moves,
            kind.upkeep,
            kind.price,
            kind.commander,
            prices.join(" ")
        ));
    }
    for city in map.cities().iter() {
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
    }
    out.push_str("end\n");
    out
}

/// An order in a line of text (`docs/agents.md`): `move <units> <row> <col>`,
/// `toward <units> <row> <col>` (units: names with commas between),
/// `goto <units> <row> <col>`, `done <units>`, `build <row> <col> <index or
/// none>`, `rally <row> <col> <row> <col>` (or `none`), `undo`, `end`.
/// `None` for an empty line or a comment (`#`).
pub fn parse_order(game: &Game, line: &str) -> Result<Option<Order>, String> {
    let words: Vec<&str> = line.split_whitespace().collect();
    let hex = |row: &str, col: &str| -> Result<Hex, String> {
        let (row, col) = (row.parse().map_err(|_| "not a row")?, col.parse().map_err(|_| "not a column")?);
        let hex = Hex::new(row, col);
        if game.map.contains(hex) { Ok(hex) } else { Err(format!("{hex} is not on the map")) }
    };
    let units = |names: &str| -> Result<Vec<UnitId>, String> {
        names.split(',').map(|name| parse_unit(name).ok_or(format!("\"{name}\" is not a unit's name"))).collect()
    };
    Ok(Some(match words[..] {
        [] => return Ok(None),
        [first, ..] if first.starts_with('#') => return Ok(None),
        ["undo"] => Order::Undo,
        ["end"] => Order::EndTurn,
        ["move", u, row, col] => Order::Move { units: units(u)?, to: hex(row, col)? },
        ["toward", u, row, col] => Order::Toward { units: units(u)?, to: hex(row, col)? },
        ["build", row, col, what] => {
            let city = game.map.city_at(hex(row, col)?).ok_or("there is no city there")?.id;
            let what = match what {
                "none" => None,
                n => Some(n.parse().map_err(|_| format!("\"{n}\" is not one of what it can build"))?),
            };
            Order::Build { city, what }
        }
        ["goto", u, row, col] => Order::GoTo { units: units(u)?, to: hex(row, col)? },
        ["done", u] => Order::Done { units: units(u)? },
        ["rally", row, col, "none"] => {
            Order::Rally { city: game.map.city_at(hex(row, col)?).ok_or("there is no city there")?.id, to: None }
        }
        ["rally", row, col, to_row, to_col] => {
            let city = game.map.city_at(hex(row, col)?).ok_or("there is no city there")?.id;
            Order::Rally { city, to: Some(hex(to_row, to_col)?) }
        }
        _ => return Err("not an order (move, toward, goto, done, build, rally, undo, end)".to_string()),
    }))
}
