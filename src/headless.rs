//! A game played without a window: the computer plays every side (its
//! agents), round after round, until one side is left or the rounds run out.
//! It is how the rules are tried: many games, many seeds, agents against
//! agents (`hexgame --headless`, `docs/agents.md`).
//!
//! Not a port: the C++ had no players.

use crate::agents::{self, Agents};
use crate::game::Game;
use crate::orders::{Event, Order};
use crate::setup::Controller;
use crate::state::Side;
use crate::state::scenario::side_name;

/// How a game without a window ended.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Outcome {
    pub winner: Option<Side>,
    /// The round it ended in, or the last one played.
    pub rounds: u32,
    /// Each side's cities and units at the end, in the order of the seats.
    pub standing: Vec<(Side, usize, usize)>,
}

/// Play `game` with the computer on every side for at most `rounds` rounds;
/// `log` hears of every event, and of every order an agent got wrong. An
/// error if a person plays a side, or an agent is not known.
pub fn play(game: &mut Game, rounds: u32, log: &mut dyn FnMut(&Game, &str)) -> Result<Outcome, String> {
    if let Some(seat) = game.setup().seats.iter().find(|seat| seat.controller == Controller::Person) {
        return Err(format!(
            "{} is played by a person: without a window, the computer plays every side",
            side_name(seat.side)
        ));
    }
    let mut agents = Agents::for_game(game)?;
    while game.winner().is_none() && game.turn() <= rounds && !game.order().is_empty() {
        let report = agents::play_computers(game, &mut agents);
        for event in &report.events {
            log(game, &event_line(game, event));
        }
        for (order, why) in &report.refused {
            log(game, &format!("refused: {order:?}: {why}"));
        }
        if report.events.is_empty() && report.refused.is_empty() {
            break; // nobody plays: a game without sides
        }
    }
    Ok(outcome(game))
}

/// Where the game stands.
pub fn outcome(game: &Game) -> Outcome {
    let map = &game.map;
    let standing = game
        .setup()
        .seats
        .iter()
        .map(|seat| {
            let cities = map.cities().iter().filter(|c| c.side == seat.side).count();
            (seat.side, cities, map.units().of_side(seat.side).count())
        })
        .collect();
    Outcome { winner: game.winner(), rounds: game.turn(), standing }
}

/// The outcome in a few lines.
pub fn summary(outcome: &Outcome) -> String {
    let mut out = match outcome.winner {
        Some(side) => format!("{} wins in round {}.\n", side_name(side), outcome.rounds),
        None => format!("Nobody has won after round {}.\n", outcome.rounds.saturating_sub(1)),
    };
    for (side, cities, units) in &outcome.standing {
        out.push_str(&format!("  {}: {cities} cities, {units} units\n", side_name(*side)));
    }
    out
}

/// An event in a line, for the log.
pub fn event_line(game: &Game, event: &Event) -> String {
    let city = |id| game.map.cities().get(id).map_or("?".to_string(), |c| c.name.clone());
    match event {
        Event::Moved { unit, from, to, cost } => {
            format!("{} moves from {from} to {to} for {cost}", agents::unit_name(*unit))
        }
        Event::Taken { city: id, from, by } => {
            let from = if *from == Side::Grey { "nobody" } else { side_name(*from) };
            format!("{} takes {} from {from}", side_name(*by), city(*id))
        }
        Event::Built { city: id, unit } => format!("{} builds {}", city(*id), agents::unit_name(*unit)),
        Event::Began { side, round } => format!("round {round}: {}'s turn", side_name(*side)),
        Event::Battle { at, attacker, defender, attackers, defenders, attackers_lost, defenders_lost, won } => {
            let (a, d) = (side_name(*attacker), side_name(*defender));
            let winner = if *won { a } else { d };
            format!(
                "{a} attacks {d} at {at}, {attackers} against {defenders}: {a} loses {attackers_lost}, \
                 {d} {defenders_lost}; {winner} wins"
            )
        }
        Event::Out { side } => format!("{} is out", side_name(*side)),
        Event::Won { side } => format!("{} wins", side_name(*side)),
    }
}

/// Units' names, with commas between.
fn names(units: &[crate::state::UnitId]) -> String {
    units.iter().map(|u| agents::unit_name(*u)).collect::<Vec<_>>().join(",")
}

/// The order an agent gave, in its text form (for logs and tests).
pub fn order_line(game: &Game, order: &Order) -> String {
    match order {
        Order::Move { units, to } => format!("move {} {} {}", names(units), to.row, to.col),
        Order::Toward { units, to } => format!("toward {} {} {}", names(units), to.row, to.col),
        Order::Build { city, what } => {
            let at = game.map.cities().get(*city).map_or((0, 0), |c| (c.pos.row, c.pos.col));
            let what = what.map_or("none".to_string(), |i| i.to_string());
            format!("build {} {} {what}", at.0, at.1)
        }
        Order::EndTurn => "end".to_string(),
    }
}
