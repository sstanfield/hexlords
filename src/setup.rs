//! How a game is set up before it starts (the new game screen, or the command
//! line): the mode, the order of the sides' turns, who plays each side, the
//! seed of its chance, and what becomes of a side that is out. Plain data:
//! `Game::start` plays it. `docs/gameplay.md` has the rules these choose
//! between.

use crate::state::scenario::{side_name, side_named};
use crate::state::{Map, Side};

/// The two ways to play.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum Mode {
    /// The sides play one after the other.
    Turns,
    /// All sides give their orders, then the round plays out at once. (Not
    /// playable yet.)
    Rounds,
}

/// In what order the sides take their turns.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum TurnOrder {
    /// Always the order of the seats, which the new game screen shows.
    Fixed,
    /// Drawn at the start, and again after every round.
    Drawn,
}

/// Who gives a side's orders.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Controller {
    /// A person at this screen (several take turns at it: hotseat).
    Person,
    /// The computer: an agent, by its name (`agents::by_name`).
    Computer(String),
}

/// A side in the game, and who plays it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Seat {
    pub side: Side,
    pub controller: Controller,
}

/// What becomes of a side's units when it has lost its last city. Only one
/// way so far; others may come (they go over to the side that took the last
/// city, they fight on as mercenaries, they have a few turns to take a city
/// back): `Game::put_out` is where they go.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum WhenOut {
    /// They go away.
    Disband,
}

/// A game before it starts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Setup {
    pub mode: Mode,
    pub order: TurnOrder,
    /// The sides in the game, in the order of their turns (if it is fixed).
    pub seats: Vec<Seat>,
    /// The seed of the game's chance: one seed and one set of orders always
    /// make the same game.
    pub seed: u64,
    pub when_out: WhenOut,
}

impl Setup {
    /// The sides of a map that are in a game on it: those that hold a city at
    /// the start, or, on a map where none does, those that have units; in the
    /// sides' order.
    pub fn sides_of(map: &Map) -> Vec<Side> {
        let holds = |side: Side| map.cities().iter().any(|c| c.side == side);
        let has_units = |side: Side| map.units().of_side(side).next().is_some();
        let mut sides: Vec<Side> = Side::PLAYERS.into_iter().filter(|s| holds(*s)).collect();
        if sides.is_empty() {
            sides = Side::PLAYERS.into_iter().filter(|s| has_units(*s)).collect();
        }
        sides
    }

    /// A game on `map` for one person, who plays `person` (or the first side):
    /// the computer plays the others with the agent `agent`. Turns, in a
    /// fixed order.
    pub fn solo(map: &Map, person: Option<Side>, agent: &str) -> Setup {
        let sides = Setup::sides_of(map);
        let person = person.filter(|p| sides.contains(p)).or(sides.first().copied());
        let seats = sides
            .into_iter()
            .map(|side| Seat {
                side,
                controller: if Some(side) == person {
                    Controller::Person
                } else {
                    Controller::Computer(agent.to_string())
                },
            })
            .collect();
        Setup { mode: Mode::Turns, order: TurnOrder::Fixed, seats, seed: 1, when_out: WhenOut::Disband }
    }

    /// Who plays `side`, if it is in the game.
    pub fn controller(&self, side: Side) -> Option<&Controller> {
        self.seats.iter().find(|seat| seat.side == side).map(|seat| &seat.controller)
    }

    /// How many people play.
    pub fn people(&self) -> usize {
        self.seats.iter().filter(|seat| seat.controller == Controller::Person).count()
    }

    /// Who plays which side, from words: `red=person,blue=simple` (a side,
    /// `=`, and `person` or an agent's name). The sides not named keep who
    /// they have; a side not in the game is an error.
    pub fn set_players(&mut self, spec: &str) -> Result<(), String> {
        for word in spec.split(',').map(str::trim).filter(|w| !w.is_empty()) {
            let Some((side, who)) = word.split_once('=') else {
                return Err(format!("\"{word}\": expected <side>=<person or an agent>"));
            };
            let side = side_named(side.trim()).ok_or(format!("\"{side}\" is not a side"))?;
            let seat = self.seats.iter_mut().find(|seat| seat.side == side);
            let seat = seat.ok_or(format!("{} is not in this game", side_name(side)))?;
            seat.controller = match who.trim() {
                "person" => Controller::Person,
                "" => return Err(format!("\"{word}\": who plays it?")),
                agent => Controller::Computer(agent.to_string()),
            };
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Hex, TileType};

    fn map() -> Map {
        let mut map = Map::new(20, 20, TileType::Grass).unwrap();
        for (i, side) in [Side::Blue, Side::Red].into_iter().enumerate() {
            let city = map.add_city(&format!("C{i}"), Hex::new(5, 5 + 5 * i as u16), 0).unwrap();
            map.city_mut(city).unwrap().side = side;
        }
        map.add_city("Nobody's", Hex::new(15, 5), 0).unwrap();
        map
    }

    #[test]
    fn the_sides_in_a_game_hold_cities() {
        // In the sides' order, nobody's left out.
        assert_eq!(Setup::sides_of(&map()), [Side::Red, Side::Blue]);
        // A map without a side's city: the sides with units.
        let mut bare = Map::new(20, 20, TileType::Grass).unwrap();
        assert!(Setup::sides_of(&bare).is_empty());
        let cost = bare.acquire_move_cost(crate::state::MoveCost::default());
        let unit = bare.new_unit(Side::Black, "U", 0, 1, 1, cost);
        bare.place_unit(unit, Hex::new(1, 1)).unwrap();
        assert_eq!(Setup::sides_of(&bare), [Side::Black]);
    }

    #[test]
    fn one_person_and_the_computer() {
        let setup = Setup::solo(&map(), Some(Side::Blue), "simple");
        assert_eq!(setup.controller(Side::Blue), Some(&Controller::Person));
        assert_eq!(setup.controller(Side::Red), Some(&Controller::Computer("simple".to_string())));
        assert_eq!(setup.controller(Side::Green), None);
        assert_eq!((setup.people(), setup.mode, setup.order), (1, Mode::Turns, TurnOrder::Fixed));
        // Without a side, or with one not in the game: the first.
        assert_eq!(Setup::solo(&map(), None, "simple").controller(Side::Red), Some(&Controller::Person));
        assert_eq!(Setup::solo(&map(), Some(Side::Green), "simple").controller(Side::Red), Some(&Controller::Person));
    }

    #[test]
    fn players_are_named_in_words() {
        let mut setup = Setup::solo(&map(), None, "passive");
        setup.set_players("red=simple, blue=person").unwrap();
        assert_eq!(setup.controller(Side::Red), Some(&Controller::Computer("simple".to_string())));
        assert_eq!(setup.controller(Side::Blue), Some(&Controller::Person));
        assert_eq!(setup.set_players("green=person").unwrap_err(), "green is not in this game");
        assert_eq!(setup.set_players("mauve=person").unwrap_err(), "\"mauve\" is not a side");
        assert_eq!(setup.set_players("red").unwrap_err(), "\"red\": expected <side>=<person or an agent>");
        assert_eq!(setup.set_players("red=").unwrap_err(), "\"red=\": who plays it?");
    }
}
