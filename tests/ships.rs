//! Ships and flyers: a ship goes over water and into its side's cities, and
//! carries land units (as many as its kind says; a commander besides); who
//! rides a ship that sinks goes down with it; in a port ships do not fight;
//! nobody is left in the water without a ship. A flyer goes where its prices
//! let it, and a commander with flyers and no one else flies with them.

use hexgame::game::Game;
use hexgame::orders::{Event, Order};
use hexgame::setup::Setup;
use hexgame::state::scenario::{self, Scenario};
use hexgame::state::{Hex, Side, UnitId};

const REDHOLD: Hex = Hex::new(10, 8);
const BLUEMOOR: Hex = Hex::new(10, 16);

const KINDS: &str = "\
unit militia
  strength 2
  moves    10
  enter    grass 2, city 1
unit captain
  strength  3
  moves     12
  commander 1
  enter     grass 2, city 1
unit transport
  strength 1
  moves    12
  travels  sea
  carries  6
  enter    water 2, city 2
unit warship
  strength 6
  moves    16
  travels  sea
  carries  1
  enter    water 2, city 2
unit hawks
  strength 3
  moves    16
  travels  air
  enter    grass 2, water 2, city 1
";

/// 20 x 20: grass west of column 9, a strait of water from 9 to 14, grass
/// east of it; red's Redhold on the west shore, blue's Bluemoor on the east.
/// Red plays (a person), blue passes; battles without chance.
fn game() -> Game {
    let row = format!("{}{}{}", "g".repeat(9), "w".repeat(6), "g".repeat(5));
    let mut rows = vec![row; 20];
    rows[10] = format!("{}c{}gc{}", "g".repeat(8), "w".repeat(6), "g".repeat(3));
    let terrain = format!("20\n20\n{}\n", rows.join("\n"));
    let cities = "city 10 8\nname Redhold\nside red\ncity 10 16\nname Bluemoor\nside blue\n";
    let (map, warnings) = scenario::parse(&terrain, Some(KINDS), Some(cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup.combat.chance = 0;
    game.with_setup(setup)
}

fn put(game: &mut Game, side: Side, kind: &str, at: Hex) -> UnitId {
    let kind = game.map.units().kind_named(kind).unwrap();
    let id = game.map.new_unit_of(side, kind).unwrap();
    game.map.place_unit(id, at).unwrap();
    id
}

fn moves(game: &mut Game, units: &[UnitId], to: Hex) -> Result<Vec<Event>, String> {
    game.apply(Order::Move { units: units.to_vec(), to })
}

fn alive(game: &Game, id: UnitId) -> bool {
    game.map.units().get(id).is_some()
}

#[test]
fn a_ship_goes_over_water_and_into_its_own_ports_only() {
    let mut game = game();
    let ship = put(&mut game, Side::Red, "transport", REDHOLD);
    assert!(game.select_unit(ship));
    assert!(game.can_move_to(Hex::new(10, 9)) && game.can_move_to(Hex::new(7, 12)));
    assert!(!game.can_move_to(Hex::new(10, 7)), "not onto land");
    assert!(!game.can_move_to(BLUEMOOR), "not into another side's city");
    moves(&mut game, &[ship], Hex::new(10, 14)).unwrap();
    assert_eq!(game.map.units().get(ship).unwrap().pos(), Hex::new(10, 14));
}

#[test]
fn land_units_step_aboard_ride_for_nothing_and_land() {
    let mut game = game();
    let ship = put(&mut game, Side::Red, "transport", Hex::new(10, 9));
    let men = [put(&mut game, Side::Red, "militia", REDHOLD), put(&mut game, Side::Red, "militia", REDHOLD)];
    // Aboard: a step onto the ship costs what a step into a city does, and
    // ends the move.
    assert!(game.select_unit(men[0]));
    assert!(game.can_move_to(Hex::new(10, 9)) && !game.can_move_to(Hex::new(10, 10)));
    moves(&mut game, &men, Hex::new(10, 9)).unwrap();
    assert_eq!(game.map.units().get(men[0]).unwrap().movement, 9);
    // The stack at sea: all of it is selected, and goes where the ship goes,
    // the riders paying nothing.
    game.deselect();
    game.select_hex(Hex::new(10, 9));
    assert_eq!(game.selection().len(), 3);
    assert!(game.can_move_to(Hex::new(10, 14)) && !game.can_move_to(Hex::new(10, 15)));
    moves(&mut game, &[ship, men[0], men[1]], Hex::new(10, 14)).unwrap();
    let man = game.map.units().get(men[0]).unwrap();
    assert_eq!((man.pos(), man.movement), (Hex::new(10, 14), 9));
    // Ashore, on their own points.
    moves(&mut game, &men, Hex::new(10, 15)).unwrap();
    assert_eq!(game.map.units_at(Hex::new(10, 15)).len(), 2);
}

#[test]
fn a_ship_carries_as_many_as_its_kind_says() {
    let mut game = game();
    let warship = put(&mut game, Side::Red, "warship", Hex::new(10, 9));
    let men = [put(&mut game, Side::Red, "militia", REDHOLD), put(&mut game, Side::Red, "militia", REDHOLD)];
    let captain = put(&mut game, Side::Red, "captain", REDHOLD);
    assert!(moves(&mut game, &men, Hex::new(10, 9)).is_err(), "two do not fit in a warship");
    moves(&mut game, &men[..1], Hex::new(10, 9)).unwrap();
    assert!(moves(&mut game, &men[1..], Hex::new(10, 9)).is_err(), "it is full");
    // A commander goes besides.
    moves(&mut game, &[captain], Hex::new(10, 9)).unwrap();
    assert_eq!(game.map.units_at(Hex::new(10, 9)), [warship, men[0], captain]);
    // Too many for the ships: they cannot go together.
    let other = put(&mut game, Side::Red, "militia", Hex::new(10, 9));
    game.select_hex(Hex::new(10, 9));
    assert_eq!(game.selection_problem().as_deref(), Some("the ships carry 1, not 2"));
    assert!(!game.can_move_to(Hex::new(10, 11)));
    assert!(game.map.units().get(other).is_some());
}

#[test]
fn nobody_is_left_in_the_water() {
    let mut game = game();
    let ship = put(&mut game, Side::Red, "transport", Hex::new(10, 9));
    let man = put(&mut game, Side::Red, "militia", REDHOLD);
    moves(&mut game, &[man], Hex::new(10, 9)).unwrap();
    let why = moves(&mut game, &[ship], Hex::new(10, 11)).unwrap_err();
    assert!(why.contains("leave units in the water"), "{why}");
    // A land unit cannot go onto open water by itself.
    assert!(moves(&mut game, &[man], Hex::new(10, 10)).is_err());
}

#[test]
fn who_rides_a_ship_that_sinks_goes_down_with_it() {
    let mut game = game();
    let transport = put(&mut game, Side::Red, "transport", Hex::new(10, 12));
    let men: Vec<UnitId> = (0..3).map(|_| put(&mut game, Side::Red, "militia", Hex::new(10, 12))).collect();
    let warship = put(&mut game, Side::Blue, "warship", Hex::new(10, 13));
    game.apply(Order::EndTurn).unwrap();
    // Blue's warship attacks: the militia aboard do not fight, and go down.
    let events = moves(&mut game, &[warship], Hex::new(10, 12)).unwrap();
    let battle = events.iter().find_map(|e| match e {
        Event::Battle { attackers, defenders, defenders_lost, won, .. } => {
            Some((*attackers, *defenders, *defenders_lost, *won))
        }
        _ => None,
    });
    assert_eq!(battle, Some((1, 1, 4, true)), "the transport fought alone; all four were lost");
    assert!(!alive(&game, transport) && men.iter().all(|m| !alive(&game, *m)));
    assert_eq!(game.map.units_at(Hex::new(10, 12)), [warship]);
}

#[test]
fn riders_of_ships_that_attack_and_sink_go_down_too() {
    let mut game = game();
    let transport = put(&mut game, Side::Red, "transport", Hex::new(10, 12));
    let men: Vec<UnitId> = (0..2).map(|_| put(&mut game, Side::Red, "militia", Hex::new(10, 12))).collect();
    put(&mut game, Side::Blue, "warship", Hex::new(10, 13));
    let mut group = vec![transport];
    group.extend(&men);
    let events = moves(&mut game, &group, Hex::new(10, 13)).unwrap();
    assert!(events.iter().any(|e| matches!(e, Event::Battle { won: false, .. })));
    let drowned = events.iter().find_map(|e| match e {
        Event::Drowned { side, at, dead } => Some((*side, *at, dead.len())),
        _ => None,
    });
    assert_eq!(drowned, Some((Side::Red, Hex::new(10, 12), 2)));
    assert!(game.map.units_at(Hex::new(10, 12)).is_empty());
}

#[test]
fn in_a_port_ships_do_not_fight_and_fall_with_it() {
    let mut game = game();
    // Blue's transports in Bluemoor, and nobody else: nobody defends it.
    let transport = put(&mut game, Side::Blue, "transport", BLUEMOOR);
    put(&mut game, Side::Blue, "transport", BLUEMOOR);
    let attacker = put(&mut game, Side::Red, "militia", Hex::new(10, 17));
    let events = moves(&mut game, &[attacker], BLUEMOOR).unwrap();
    let battle = events.iter().find_map(|e| match e {
        Event::Battle { defenders, defenders_lost, won, .. } => Some((*defenders, *defenders_lost, *won)),
        _ => None,
    });
    assert_eq!(battle, Some((0, 2, true)), "the ships did not fight, and were lost with the city");
    assert!(!alive(&game, transport));
    assert_eq!(game.map.city_at(BLUEMOOR).unwrap().side, Side::Red);
}

#[test]
fn flyers_cross_the_water_and_a_captain_flies_with_them() {
    let mut game = game();
    let hawks = put(&mut game, Side::Red, "hawks", REDHOLD);
    let captain = put(&mut game, Side::Red, "captain", REDHOLD);
    assert!(game.select_unit(hawks));
    assert!(game.can_move_to(Hex::new(10, 12)));
    // The captain alone walks; with the hawks, it flies.
    assert!(game.select_unit(captain));
    assert!(!game.can_move_to(Hex::new(10, 10)));
    game.deselect();
    game.select_hex(REDHOLD);
    assert_eq!(game.selection().len(), 2);
    assert!(game.can_move_to(Hex::new(10, 12)));
    moves(&mut game, &[hawks, captain], Hex::new(10, 12)).unwrap();
    // Out over the water, the hawks cannot leave it there.
    let why = moves(&mut game, &[hawks], Hex::new(10, 13)).unwrap_err();
    assert!(why.contains("leave units in the water"), "{why}");
    // The next turn, on together, over the rest of the water.
    game.apply(Order::EndTurn).unwrap();
    game.apply(Order::EndTurn).unwrap();
    moves(&mut game, &[hawks, captain], BLUEMOOR).unwrap();
    assert_eq!(game.map.city_at(BLUEMOOR).unwrap().side, Side::Red, "taken from the air");
}

#[test]
fn a_ship_sent_far_takes_its_riders_along_turn_after_turn() {
    let mut game = game();
    let ship = put(&mut game, Side::Red, "transport", Hex::new(1, 9));
    let man = put(&mut game, Side::Red, "militia", Hex::new(1, 8));
    moves(&mut game, &[man], Hex::new(1, 9)).unwrap();
    game.apply(Order::GoTo { units: vec![ship, man], to: Hex::new(19, 14) }).unwrap();
    for _ in 0..4 {
        game.apply(Order::EndTurn).unwrap();
        while game.player() != Side::Red {
            game.apply(Order::EndTurn).unwrap();
        }
    }
    assert_eq!(game.map.units_at(Hex::new(19, 14)), [ship, man]);
}
