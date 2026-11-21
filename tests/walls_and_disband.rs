//! The owner's rules of 2026-11-20: a city that is taken loses a level of
//! its walls, and its holder can raise them, a level a turn, for a high
//! price, up to a cap; a side can disband its units (a person, from the
//! Units menu, asked first); the computer's players, late in a game, build
//! the strongest they can keep and disband their weakest to make room.

mod common;

use common::{assets, click, pixel_of, type_char, widget};
use hexgame::city_windows::CITY_WALLS_ID;
use hexgame::disband_window::{DISBAND_NO_ID, DISBAND_YES_ID};
use hexgame::game::Game;
use hexgame::orders::{Event, Order};
use hexgame::session::Session;
use hexgame::setup::Setup;
use hexgame::state::scenario::{self, Scenario};
use hexgame::state::{Hex, Side, UnitId, WALLS_CAP, wall_price};
use turbo_ferret::{Point, Size};

const REDHOLD: Hex = Hex::new(5, 5);
const BLUEMOOR: Hex = Hex::new(5, 9);

/// Red's Redhold (walls 2) with `red`; blue's Bluemoor (walls 3) with
/// `blue`; and a city of each besides, far off, so that taking one puts
/// nobody out. Red starts with `gold`.
fn game(red: &str, blue: &str, gold: u32) -> Game {
    let mut rows = vec!["g".repeat(20); 20];
    rows[5] = "gggggcgggcgggggggggg".to_string();
    rows[15] = "gggggcgggggggggcgggg".to_string();
    rows[10] = "wwwwwwwwwwwwwwwwwwww".to_string();
    let terrain = format!("20\n20\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nupkeep 3\nprice 6\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 12\nupkeep 1\nprice 2\nenter grass 2, city 1\n\
                 unit knights\nname Knights\nstrength 6\nmoves 12\nupkeep 7\nprice 14\nenter grass 2, city 1\n\
                 unit boat\nname Boat\nstrength 1\nmoves 12\nupkeep 1\ntravels sea\ncarries 2\nenter water 2, city 2\n";
    let cities = format!(
        "city 5 5\nname Redhold\nside red\nwalls 2\nincome 20\nproduce levy 1\nproduce knights 2\n{red}\
         city 15 5\nname Southwick\nside red\n\
         city 5 9\nname Bluemoor\nside blue\nwalls 3\n{blue}city 15 15\nname Farwater\nside blue\n"
    );
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(&cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut s = Setup::solo(&game.map, Some(Side::Red), "passive");
    (s.combat.chance, s.gold) = (0, gold);
    game.with_setup(s)
}

fn at(game: &Game, hex: Hex) -> Vec<UnitId> {
    game.map.units_at(hex).to_vec()
}

fn walls(game: &Game, hex: Hex) -> u32 {
    game.map.city_at(hex).unwrap().walls
}

#[test]
fn a_city_taken_loses_a_level_of_its_walls() {
    let mut game = game("garrison soldiers 3\n", "", 50);
    let red = at(&game, REDHOLD);
    let city = game.map.city_at(BLUEMOOR).unwrap().id;
    let events = game.apply(Order::Move { units: red.clone(), to: BLUEMOOR }).unwrap();
    assert!(events.contains(&Event::Taken { city, from: Side::Blue, by: Side::Red, walls: Some(2) }), "{events:?}");
    assert_eq!(walls(&game, BLUEMOOR), 2);
    // Taken back with the move: its walls stand again.
    game.apply(Order::Undo).unwrap();
    assert_eq!((walls(&game, BLUEMOOR), game.map.city_at(BLUEMOOR).unwrap().side), (3, Side::Blue));
    // The log says so.
    let lines = hexgame::info::record_lines(
        &game.map,
        &hexgame::orders::Record {
            round: 1,
            turn: Side::Red,
            event: Event::Taken { city, from: Side::Blue, by: Side::Red, walls: Some(2) },
        },
        Side::Red,
    );
    assert_eq!(lines, ["You took Bluemoor from blue. Its walls fell to 2."]);
}

#[test]
fn a_holder_raises_its_walls_once_a_turn_for_a_high_price() {
    assert_eq!((wall_price(0), wall_price(2), wall_price(5)), (25, 75, 150));
    let mut game = game("", "", 100);
    let redhold = game.map.city_at(REDHOLD).unwrap().id;
    let bluemoor = game.map.city_at(BLUEMOOR).unwrap().id;
    assert_eq!(game.walls_price(Side::Red, redhold), Ok(75));
    let events = game.apply(Order::Walls { city: redhold }).unwrap();
    assert_eq!(events, [Event::WallsRaised { city: redhold, side: Side::Red, walls: 3, price: 75 }]);
    assert_eq!((walls(&game, REDHOLD), game.gold(Side::Red)), (3, 25));
    // Once a turn.
    assert_eq!(game.apply(Order::Walls { city: redhold }).unwrap_err(), "its walls were raised this turn");
    // Taken back: the walls and the gold.
    game.apply(Order::Undo).unwrap();
    assert_eq!((walls(&game, REDHOLD), game.gold(Side::Red)), (2, 100));
    game.apply(Order::Walls { city: redhold }).unwrap();
    // Only the holder's.
    assert!(game.apply(Order::Walls { city: bluemoor }).unwrap_err().contains("only blue"));
    // Next turn, again; not without the gold.
    let reds: Vec<_> = game.map.cities().iter().filter(|c| c.side == Side::Red).map(|c| c.id).collect();
    for city in reds {
        game.map.city_mut(city).unwrap().income = 0;
    }
    while game.player() != Side::Red || game.turn() == 1 {
        game.end_turn();
    }
    assert_eq!(game.gold(Side::Red), 25);
    assert_eq!(game.apply(Order::Walls { city: redhold }).unwrap_err(), "that takes 100 gold");
    // Not above the cap.
    game.map.city_mut(redhold).unwrap().walls = WALLS_CAP;
    assert_eq!(game.apply(Order::Walls { city: redhold }).unwrap_err(), "its walls are as high as walls go");
}

#[test]
fn units_disbanded_are_gone_with_their_upkeep() {
    let mut game = game("garrison soldiers 2\ngarrison levy\n", "garrison levy\n", 50);
    let red = at(&game, REDHOLD);
    let upkeep = game.upkeep(Side::Red);
    let events = game.apply(Order::Disband { units: red[..2].to_vec() }).unwrap();
    assert_eq!(events, [Event::Disbanded { side: Side::Red, names: vec!["Soldiers".into(), "Soldiers".into()] }]);
    assert!(red[..2].iter().all(|id| game.map.units().get(*id).is_none()));
    assert_eq!(at(&game, REDHOLD), [red[2]]);
    assert_eq!(game.upkeep(Side::Red), upkeep - 6);
    // Not another side's; and no undo.
    let blue = at(&game, BLUEMOOR);
    assert!(game.apply(Order::Disband { units: blue }).unwrap_err().contains("blue's"));
    assert!(!game.can_undo());
}

#[test]
fn a_ship_at_sea_goes_home_only_with_those_it_carries() {
    let mut game = game("garrison soldiers 2\n", "", 50);
    let sea = Hex::new(10, 5);
    let red = at(&game, REDHOLD);
    let boat = game.map.new_unit_of(Side::Red, game.map.units().kind_named("boat").unwrap()).unwrap();
    game.map.place_unit(boat, sea).unwrap();
    for id in &red {
        game.map.move_unit(*id, sea).unwrap();
    }
    assert_eq!(
        game.apply(Order::Disband { units: vec![boat] }).unwrap_err(),
        "those aboard would drown: the ship cannot go without them"
    );
    // With them, or once they are gone, it can.
    game.apply(Order::Disband { units: vec![red[0]] }).unwrap();
    assert!(game.apply(Order::Disband { units: vec![boat] }).is_err());
    game.apply(Order::Disband { units: vec![boat, red[1]] }).unwrap();
    assert!(game.map.units_at(sea).is_empty());
}

fn click_hex(s: &mut Session, hex: Hex) {
    s.screen.view.center_on(hex);
    let p = pixel_of(s, hex);
    click(s, p);
}

fn push(s: &mut Session, name: &str) {
    let r = s.screen.area(widget(s, name));
    click(s, Point::new(r.x + r.w / 2, r.y + r.h / 2));
}

/// The owner: "Add a disband option to remove units." X (or Disband in the
/// Units menu) asks, naming each unit and its upkeep; Keep them changes
/// nothing, Disband sends them home.
#[test]
fn x_asks_and_disbands_the_selected_units() {
    let game = game("garrison soldiers 2\n", "", 50);
    let red = at(&game, REDHOLD);
    let mut s = Session::with_game(&assets(), Size::new(1024, 768), 96, game).unwrap();
    click_hex(&mut s, REDHOLD);
    type_char(&mut s, 'x');
    let asks = s.disband().expect("it asks");
    assert_eq!(asks.lines, ["Soldiers: upkeep 3 a turn", "Soldiers: upkeep 3 a turn"]);
    push(&mut s, DISBAND_NO_ID);
    assert!(s.disband().is_none());
    assert_eq!(at(&s.screen.game, REDHOLD), red);
    type_char(&mut s, 'x');
    push(&mut s, DISBAND_YES_ID);
    assert!(s.disband().is_none());
    assert!(at(&s.screen.game, REDHOLD).is_empty());
    // With nothing selected, X does nothing.
    type_char(&mut s, 'x');
    assert!(s.disband().is_none());
}

/// The city's window has a button that raises its walls, with their price;
/// greyed once they were raised this turn.
#[test]
fn the_city_window_raises_the_walls() {
    let game = game("", "", 100);
    let mut s = Session::with_game(&assets(), Size::new(1024, 768), 96, game).unwrap();
    click_hex(&mut s, REDHOLD);
    type_char(&mut s, 'o');
    s.frame(0);
    assert_eq!(s.city_window().unwrap().walls(), Some(("Raise the walls to 3: 75 gold", true)));
    push(&mut s, CITY_WALLS_ID);
    s.frame(0);
    assert_eq!(walls(&s.screen.game, REDHOLD), 3);
    assert_eq!(s.city_window().unwrap().walls(), Some(("The walls were raised this turn", false)));
    // Another side's city: no button.
    click_hex(&mut s, BLUEMOOR);
    type_char(&mut s, 'o');
    s.frame(0);
    assert_eq!(s.city_window().unwrap().walls(), None);
}

/// Late in a game (two stacks' worth of soldiers), the planner sends home
/// its weakest to make room for the strongest a city of its can build, when
/// it cannot keep that one besides.
#[test]
fn the_planner_disbands_its_weakest_to_make_room_for_the_strong() {
    let mut game = game("garrison levy 8\n", "garrison levy\n", 200);
    let levy = game.map.units().kind_named("levy").unwrap();
    for row in [2, 3] {
        for _ in 0..8 {
            let id = game.map.new_unit_of(Side::Red, levy).unwrap();
            game.map.place_unit(id, Hex::new(row, 2)).unwrap();
        }
    }
    // Taxes 20, upkeep 24 a turn: it cannot keep the knights besides.
    assert!(game.income(Side::Red) < game.upkeep(Side::Red) + 7);
    let mut planner = hexgame::planner::Planner::new(hexgame::planner::STEADY);
    let order = hexgame::agents::Agent::next_order(&mut planner, &game.view_for(Side::Red), Side::Red);
    let Order::Disband { units } = order else { panic!("{order:?}") };
    assert_eq!(units.len(), 1);
    assert_eq!(game.map.units().get(units[0]).unwrap().title(), "Levy");
    // Once it can keep them, its idle city builds the knights.
    let mut planner = hexgame::planner::Planner::new(hexgame::planner::STEADY);
    for _ in 0..8 {
        match hexgame::agents::Agent::next_order(&mut planner, &game.view_for(Side::Red), Side::Red) {
            order @ Order::Disband { .. } => {
                game.apply(order).unwrap();
            }
            Order::Build { what, .. } => {
                let city = game.map.city_at(REDHOLD).unwrap();
                assert_eq!(what.map(|i| city.units[i].basename.as_str()), Some("knights"));
                return;
            }
            other => panic!("{other:?}"),
        }
    }
    panic!("it never built the knights");
}

/// The owner's table for walls in battle: nothing behind walls 0, 1 behind
/// 1 or 2, 2 behind 3 to 5, 3 behind 6.
#[test]
fn walls_make_defenders_stronger_by_steps() {
    use hexgame::state::combat::walls_bonus;
    assert_eq!([0, 1, 2, 3, 4, 5, 6].map(walls_bonus), [0, 1, 1, 2, 2, 2, 3]);
}
