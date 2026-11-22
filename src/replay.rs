//! What happened, kept so that it can be looked at again (the owner's word,
//! 2026-11-22: "It is also hard to follow all the action ... a 'replay'
//! feature of the previous turn would help. Maybe a slider on the side bar
//! that would allow the user to 'go back' in time and examine movement,
//! battles, etc. You can not give orders when 'in the past'").
//!
//! A [`Replay`] is the game as it was at some moment and what was done to
//! it since, a step at a time: an order carried out (`Game::record`), or,
//! in rounds, a beat of the round playing out (`Game::play_out`). The rules
//! are deterministic (a seed and a set of orders always make the same
//! game), so the steps carried out again on a copy of that game make what
//! happened happen again, exactly: every state in between can be had, and
//! every move and battle shown as it was. The copies are frozen
//! (`Game::freeze`): they can be looked at, not given orders.
//!
//! (The same record, from a game's start, is what a saved game can be made
//! of: the setup, the map and the orders. Not built yet.)
//!
//! Pure: no window. `Session` keeps one for each person at the screen (what
//! happened since their last turn) and shows them.

use crate::game::Game;
use crate::orders::{Event, Order};

/// Something that was done to the game.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    /// An order of the side whose turn it was, carried out.
    Order(Order),
    /// In rounds: the round that was playing out went on a beat.
    Beat,
}

/// The game as it was, and what was done to it since.
#[derive(Clone)]
pub struct Replay {
    start: Game,
    steps: Vec<Step>,
}

impl Replay {
    /// A record that begins with `game` as it is now. (What `game` keeps
    /// for whoever watches it is not the copy's.)
    pub fn begin(game: &Game) -> Replay {
        let mut start = game.clone();
        start.take_orders();
        start.record(false);
        start.watch(true);
        start.take_happened();
        start.deselect();
        Replay { start, steps: Vec::new() }
    }

    /// `step` was done next.
    pub fn push(&mut self, step: Step) {
        self.steps.push(step);
    }

    /// How many steps it has.
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// The `at`th step (from 0).
    pub fn step(&self, at: usize) -> Option<&Step> {
        self.steps.get(at)
    }

    /// The game as it was after its first `at` steps (0: as it began; all
    /// of them: as it was when the last was done), frozen.
    pub fn state(&self, at: usize) -> Game {
        let mut game = self.start.clone();
        for step in self.steps.iter().take(at) {
            take(&mut game, step);
        }
        game.take_happened();
        game.freeze(true);
        game
    }

    /// `game` is the game as it was after `at` steps (`state`): the next
    /// step is done to it, and what happened is what a screen shows
    /// (`Game::take_happened`). Nothing, past the last step.
    pub fn advance(&self, game: &mut Game, at: usize) -> Vec<(Option<usize>, Event)> {
        let Some(step) = self.steps.get(at) else { return Vec::new() };
        game.freeze(false);
        take(game, step);
        game.freeze(true);
        game.take_happened()
    }
}

/// Do `step` to `game`. (An order that was carried out then is carried out
/// now: the game is the same.)
fn take(game: &mut Game, step: &Step) {
    match step {
        Step::Order(order) => {
            let _ = game.apply(order.clone());
        }
        Step::Beat => {
            game.play_out();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::{Mode, Setup};
    use crate::state::scenario::{self, Scenario};
    use crate::state::{Hex, Side, UnitId};

    /// 20 x 20 grass, red's and blue's cities, a stack each; both the
    /// computer's (passive: the test gives the orders).
    fn game(mode: Mode) -> (Game, UnitId, UnitId) {
        let mut rows = vec!["g".repeat(20); 20];
        rows[5] = format!("{}c{}c{}", "g".repeat(2), "g".repeat(14), "g".repeat(2));
        let terrain = format!("20\n20\n{}\n", rows.join("\n"));
        let units = "unit soldiers\nname Soldiers\nstrength 5\nmoves 12\nenter grass 2, city 1\n\
                     unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n";
        let cities = "city 5 2\nname Redhold\nside red\ncity 5 17\nname Bluemoor\nside blue\n";
        let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
        let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
        let mut put = |side: Side, kind: &str, hex: Hex| {
            let kind = game.map.units().kind_named(kind).unwrap();
            let id = game.map.new_unit_of(side, kind).unwrap();
            game.map.place_unit(id, hex).unwrap();
            id
        };
        let (a, b) = (put(Side::Red, "soldiers", Hex::new(10, 5)), put(Side::Blue, "levy", Hex::new(10, 9)));
        let mut setup = Setup::solo(&game.map, None, "passive");
        setup.set_players("red=passive,blue=passive").unwrap();
        (setup.mode, setup.combat.chance) = (mode, 0);
        let mut game = game.with_setup(setup);
        game.watch(true);
        game.record(true);
        (game, a, b)
    }

    fn places(game: &Game) -> Vec<(UnitId, Hex, u32)> {
        game.map.units().iter().map(|u| (u.id(), u.pos(), u.movement)).collect()
    }

    /// Turns: the orders carried out, taken from the game, make the same
    /// game again on a copy of how it was: every state on the way, the
    /// battle and all.
    #[test]
    fn the_orders_of_a_turn_make_it_happen_again() {
        let (mut game, a, b) = game(Mode::Turns);
        let mut replay = Replay::begin(&game);
        let mut states = vec![places(&game)];
        let orders = [
            Order::Move { units: vec![a], to: Hex::new(10, 7) },
            Order::EndTurn,
            Order::Move { units: vec![b], to: Hex::new(10, 8) },
            Order::EndTurn,
            Order::Move { units: vec![a], to: Hex::new(10, 8) },
        ];
        for order in orders {
            game.apply(order).unwrap();
            states.push(places(&game));
        }
        // (An order refused is not one that was carried out.)
        assert!(game.apply(Order::Move { units: vec![b], to: Hex::new(10, 7) }).is_err());
        let taken = game.take_orders();
        assert_eq!(taken.len(), 5);
        assert!(game.take_orders().is_empty(), "taken once");
        for order in taken {
            replay.push(Step::Order(order));
        }
        assert_eq!((replay.len(), replay.is_empty()), (5, false));
        for (at, state) in states.iter().enumerate() {
            assert_eq!(&places(&replay.state(at)), state, "after {at} orders");
        }
        assert_eq!(replay.state(5).history(), game.history(), "the battle, as it went");
        assert!(game.map.units().get(b).is_none(), "red's soldiers beat blue's levy");
        // A step at a time, with what a screen shows of each.
        let mut past = replay.state(2);
        let happened = replay.advance(&mut past, 2);
        assert!(happened.iter().any(|(_, e)| matches!(e, Event::Moved { unit, .. } if *unit == b)), "{happened:?}");
        assert_eq!(places(&past), states[3]);
        assert!(replay.advance(&mut past, 9).is_empty(), "past the last step: nothing");
        // The past takes no orders; it can still be looked at.
        assert!(past.frozen());
        let refused = past.apply(Order::EndTurn);
        assert!(refused.is_err_and(|why| why.contains("orders are given in the game as it is now")));
        assert_eq!(places(&past), states[3]);
        // The game itself was never touched by any of it.
        assert_eq!(places(&game), states[5]);
        assert!(!game.frozen());
    }

    /// Rounds: from the moment the round begins to play out, each beat of
    /// it again.
    #[test]
    fn the_beats_of_a_round_make_it_play_out_again() {
        let (mut game, a, b) = game(Mode::Rounds);
        game.apply(Order::Move { units: vec![a], to: Hex::new(10, 9) }).unwrap();
        game.apply(Order::EndTurn).unwrap();
        game.apply(Order::Move { units: vec![b], to: Hex::new(12, 9) }).unwrap();
        game.apply(Order::EndTurn).unwrap();
        assert!(game.playing_out());
        let mut replay = Replay::begin(&game);
        let mut states = vec![places(&game)];
        while game.playing_out() {
            game.play_out();
            replay.push(Step::Beat);
            states.push(places(&game));
        }
        assert!(replay.len() >= 3, "{}", replay.len());
        for (at, state) in states.iter().enumerate() {
            assert_eq!(&places(&replay.state(at)), state, "after {at} beats");
        }
        let last = replay.state(replay.len());
        assert_eq!((last.turn(), last.playing_out()), (2, false), "the round played out, and the next began");
        // A frozen copy still plays its round on, a beat at a time.
        let mut past = replay.state(0);
        assert!(past.playing_out() && past.frozen());
        let happened = replay.advance(&mut past, 0);
        assert!(happened.iter().any(|(_, e)| matches!(e, Event::Moved { .. })), "{happened:?}");
        assert_eq!(places(&past), states[1]);
    }
}
