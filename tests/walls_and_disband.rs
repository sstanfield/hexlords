//! The owner's rules of 2026-11-20: a city that is taken loses a level of
//! its walls, and its holder can raise them, a level a turn, for a high
//! price, up to a cap; a side can disband its units (a person, from the
//! Units menu, asked first); the computer's players, late in a game, build
//! the strongest they can keep and disband their weakest to make room.

mod common;

use common::{assets, click, pixel_of, type_char, widget};
use hexlords::city_windows::CITY_WALLS_ID;
use hexlords::disband_window::{DISBAND_NO_ID, DISBAND_YES_ID};
use hexlords::game::Game;
use hexlords::orders::{Event, Order};
use hexlords::session::Session;
use hexlords::setup::Setup;
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{Hex, Sends, Side, UnitId, WALLS_CAP, wall_price};
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
                 unit boat\nname Boat\nstrength 1\nmoves 12\nupkeep 1\ntravels sea\ncarries 2\nenter water 2, city 2\n\
                 unit captain\nname Captain\nstrength 2\nmoves 12\ncommander 1\nenter grass 2, city 1\n\
                 unit hawk\nname Hawk\nstrength 1\nmoves 12\ntravels air\nenter grass 2, water 2, city 1\n\
                 unit ram\nname Ram\nstrength 1\nmoves 6\nupkeep 1\nsiege 1\nenter grass 2, city 1\n\
                 unit ranger\nname Ranger\nstrength 2\nmoves 12\nbonus forest 2, city 1\nenter grass 2, forest 2, city 1\n\
                 unit runner\nname Runner\nstrength 1\nmoves 24\nupkeep 2\nenter grass 2, city 1\n";
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
    let lines = hexlords::info::record_lines(
        &game.map,
        &hexlords::orders::Record {
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
    let mut s = common::with_game(&assets(), Size::new(1024, 768), 96, game).unwrap();
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
    let mut s = common::with_game(&assets(), Size::new(1024, 768), 96, game).unwrap();
    click_hex(&mut s, REDHOLD);
    type_char(&mut s, 'o');
    s.frame(0);
    assert_eq!(s.city_window().unwrap().walls(), Some(("Raise the walls to 3: 75 gold", true)));
    push(&mut s, CITY_WALLS_ID);
    s.frame(0);
    assert_eq!(walls(&s.screen.game, REDHOLD), 3);
    assert_eq!(s.city_window().unwrap().walls(), Some(("The walls were raised this turn", false)));
    // Another side's city: no button (the window shut first: it is wide).
    common::press(&mut s, turbo_ferret::SpecialKey::Esc);
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
    let mut planner = hexlords::planner::Planner::new(hexlords::planner::STEADY);
    let order = hexlords::agents::Agent::next_order(&mut planner, &game.view_for(Side::Red), Side::Red);
    let Order::Disband { units } = order else { panic!("{order:?}") };
    assert_eq!(units.len(), 1);
    assert_eq!(game.map.units().get(units[0]).unwrap().title(), "Levy");
    // Once it can keep them, its idle city builds the knights.
    let mut planner = hexlords::planner::Planner::new(hexlords::planner::STEADY);
    for _ in 0..8 {
        match hexlords::agents::Agent::next_order(&mut planner, &game.view_for(Side::Red), Side::Red) {
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
    use hexlords::state::combat::walls_bonus;
    assert_eq!([0, 1, 2, 3, 4, 5, 6].map(walls_bonus), [0, 1, 1, 2, 2, 2, 3]);
}

fn new_unit(game: &mut Game, side: Side, kind: &str, at: Hex) -> UnitId {
    let kind = game.map.units().kind_named(kind).unwrap();
    let id = game.map.new_unit_of(side, kind).unwrap();
    game.map.place_unit(id, at).unwrap();
    id
}

/// A commander at sea rides with a ship or a flyer: disbanding the last of
/// them would leave him on the water (the rule a move keeps).
#[test]
fn a_commander_at_sea_keeps_his_ship_or_his_flyer() {
    let mut game = game("", "", 50);
    let sea = Hex::new(10, 5);
    let boat = new_unit(&mut game, Side::Red, "boat", sea);
    let captain = new_unit(&mut game, Side::Red, "captain", sea);
    assert!(game.apply(Order::Disband { units: vec![boat] }).is_err(), "the captain would be left on the water");
    let far = Hex::new(10, 12);
    let hawk = new_unit(&mut game, Side::Red, "hawk", far);
    let flying = new_unit(&mut game, Side::Red, "captain", far);
    assert!(game.apply(Order::Disband { units: vec![hawk] }).is_err(), "nor a captain who flies");
    // With him, they go.
    game.apply(Order::Disband { units: vec![boat, captain] }).unwrap();
    game.apply(Order::Disband { units: vec![hawk, flying] }).unwrap();
}

/// The battle kept to be watched again shows a commander's lead over the
/// units that fight, though he does not (aboard a ship at sea).
#[test]
fn a_battle_watched_again_shows_who_led_without_fighting() {
    let mut game = game("", "", 50);
    let (sea, there) = (Hex::new(10, 5), Hex::new(10, 6));
    let boat = new_unit(&mut game, Side::Red, "boat", sea);
    let captain = new_unit(&mut game, Side::Red, "captain", sea);
    new_unit(&mut game, Side::Blue, "boat", there);
    game.apply(Order::Move { units: vec![boat, captain], to: there }).unwrap();
    let record = game.history().iter().rposition(|r| matches!(r.event, Event::Battle { .. })).unwrap();
    let fight = game.fight(record).unwrap();
    let ours = fight.attackers.iter().find(|c| c.unit == boat).expect("the boat fought");
    assert_eq!((ours.led, ours.quarters), (1, 8), "1 and its captain's 1");
    assert_eq!(fight.siege, 0);
}

/// The log tells its side of the walls it raised and the units it sent
/// home.
#[test]
fn the_log_tells_of_walls_raised_and_units_disbanded() {
    let mut game = game("garrison levy\n", "", 100);
    let redhold = game.map.city_at(REDHOLD).unwrap().id;
    game.apply(Order::Walls { city: redhold }).unwrap();
    let levy = at(&game, REDHOLD)[0];
    game.apply(Order::Disband { units: vec![levy] }).unwrap();
    let lines: Vec<String> = hexlords::info::log(&game, Side::Red, 0)
        .into_iter()
        .flat_map(|(_, entries)| entries.into_iter().flat_map(|e| e.lines))
        .collect();
    assert!(lines.contains(&"You raised the walls of Redhold to 3, for 75 gold.".to_string()), "{lines:?}");
    assert!(lines.contains(&"You sent home Levy.".to_string()), "{lines:?}");
}

/// A city taken (empty) and the move taken back: its holder's rally point
/// is as it was, with its walls.
#[test]
fn an_undone_capture_gives_back_the_rally_point() {
    let mut game = game("garrison levy\n", "", 50);
    let bluemoor = game.map.city_at(BLUEMOOR).unwrap().id;
    let rally = Hex::new(7, 9);
    game.map.city_mut(bluemoor).unwrap().sends = Sends::Route(vec![rally]);
    let levy = at(&game, REDHOLD)[0];
    game.apply(Order::Toward { units: vec![levy], to: BLUEMOOR }).unwrap();
    while game.map.city_at(BLUEMOOR).unwrap().side != Side::Red {
        game.end_turn();
        game.end_turn();
        game.apply(Order::Toward { units: vec![levy], to: BLUEMOOR }).unwrap();
    }
    assert_eq!(game.map.city_at(BLUEMOOR).unwrap().sends, Sends::Stay);
    game.apply(Order::Undo).unwrap();
    let city = game.map.city_at(BLUEMOOR).unwrap();
    assert_eq!((city.side, city.sends.clone(), city.walls), (Side::Blue, Sends::Route(vec![rally]), 3));
}

/// A threatened city behind walls 1 that its defenders cannot hold: the
/// planner raises its walls (to 2, on the way to 3), if it has twice the
/// price. (It once looked a level ahead only, and 1 to 2 adds no bonus.)
#[test]
fn the_planner_raises_low_walls_a_level_at_a_time() {
    let mut game = game("garrison levy\n", "", 500);
    let redhold = game.map.city_at(REDHOLD).unwrap().id;
    game.map.city_mut(redhold).unwrap().walls = 1;
    for _ in 0..6 {
        new_unit(&mut game, Side::Blue, "knights", Hex::new(5, 7));
    }
    let mut planner = hexlords::planner::Planner::new(hexlords::planner::STEADY);
    let report = hexlords::agents::play_turn(&mut game, &mut planner);
    let raised =
        report.events.iter().any(|e| matches!(e, Event::WallsRaised { city, walls: 2, .. } if *city == redhold));
    assert!(raised, "{:?}", report.events);
    assert_eq!(game.map.city_at(REDHOLD).unwrap().walls, 2);
}

/// Late, when its taxes cannot keep what its cities build, the planner
/// sends its weakest home first (it once stopped the work at once).
#[test]
fn the_planner_late_sends_its_weakest_home_before_stopping_work() {
    let mut game = game("garrison levy 8\n", "", 200);
    for row in [2, 3] {
        for _ in 0..8 {
            new_unit(&mut game, Side::Red, "levy", Hex::new(row, 2));
        }
    }
    let redhold = game.map.city_at(REDHOLD).unwrap().id;
    let knights = game.map.city_at(REDHOLD).unwrap().units.iter().position(|p| p.basename == "knights").unwrap();
    game.apply(Order::Build { city: redhold, what: Some(knights) }).unwrap();
    assert!(game.income(Side::Red) < game.upkeep(Side::Red) + 7, "it cannot keep the knights");
    let mut planner = hexlords::planner::Planner::new(hexlords::planner::STEADY);
    let order = hexlords::agents::Agent::next_order(&mut planner, &game.view_for(Side::Red), Side::Red);
    assert!(matches!(order, Order::Disband { .. }), "{order:?}");
}

/// The whole of a widget, in the root's pixels.
fn area_of(s: &Session, id: turbo_ferret::WidgetId) -> turbo_ferret::Rect {
    turbo_ferret::Rect::from_origin_size(s.screen.ui.to_root(id, Point::ZERO), s.screen.ui.rect(id).size())
}

/// A city with nine kinds to build, its walls' button and all, fits a
/// window of 1024 x 768 (its Close button was cut off).
#[test]
fn a_city_window_with_nine_kinds_fits_the_screen() {
    let mut units = String::new();
    let mut produce = String::new();
    for i in 0..9 {
        units.push_str(&format!("unit kind{i}\nname Kind {i}\nstrength 2\nmoves 10\nprice 4\nenter grass 2, city 1\n"));
        produce.push_str(&format!("produce kind{i} 1\n"));
    }
    let mut rows = vec!["g".repeat(20); 20];
    rows[5] = "gggggcgggcgggggggggg".to_string();
    let terrain = format!("20\n20\n{}\n", rows.join("\n"));
    let cities = format!("city 5 5\nname Redhold\nside red\nwalls 2\n{produce}city 5 9\nname Bluemoor\nside blue\n");
    let (map, warnings) = scenario::parse(&terrain, Some(&units), Some(&cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup.gold = 500;
    let mut s = common::with_game(&assets(), Size::new(1024, 768), 96, game.with_setup(setup)).unwrap();
    click_hex(&mut s, REDHOLD);
    type_char(&mut s, 'o');
    s.frame(0);
    let window = s.city_window().expect("its window").window();
    let r = area_of(&s, window);
    assert!(r.y >= 0 && r.bottom() <= 768, "{r:?}");
    let close = area_of(&s, widget(&s, hexlords::city_windows::CITY_CLOSE_ID));
    assert!(close.bottom() <= r.bottom() && close.h > 0, "{close:?} in {r:?}");
}

/// The disband window asks about this turn's units: a turn passing closes
/// it; one refused says why and asks again; a Go to pick for the units it
/// sent home is over; once the game is over, X asks nothing.
#[test]
fn the_disband_window_lives_in_its_turn_and_says_why_not() {
    let game = game("garrison soldiers 2\n", "", 50);
    let mut s = common::with_game(&assets(), Size::new(1024, 768), 96, game).unwrap();
    click_hex(&mut s, REDHOLD);
    type_char(&mut s, 'x');
    assert!(s.disband().is_some());
    s.end_turn();
    s.frame(0);
    assert!(s.disband().is_none(), "the turn passed");
    // A ship at sea, with its cargo left out: refused, and said.
    let sea = Hex::new(10, 5);
    let boat = new_unit(&mut s.screen.game, Side::Red, "boat", sea);
    let red = at(&s.screen.game, REDHOLD);
    for id in &red {
        s.screen.game.map.move_unit(*id, sea).unwrap();
    }
    s.screen.game.select_unit(boat);
    type_char(&mut s, 'x');
    push(&mut s, DISBAND_YES_ID);
    let asks = s.disband().expect("it asks again");
    assert!(s.screen.game.map.units().get(boat).is_some());
    let says = |s: &Session| {
        let mut out = Vec::new();
        let mut stack = vec![widget(s, hexlords::disband_window::DISBAND_WINDOW_ID)];
        while let Some(id) = stack.pop() {
            out.extend(s.screen.ui.text(id).map(str::to_string));
            stack.extend(s.screen.ui.children(id).iter().copied());
        }
        out
    };
    assert!(says(&s).iter().any(|t| t.starts_with("Not done: those aboard would drown")), "{:?}", says(&s));
    assert_eq!(asks.units, [boat]);
    push(&mut s, DISBAND_NO_ID);
    // Go to, then X and Disband: the pick is over.
    let soldier = new_unit(&mut s.screen.game, Side::Red, "soldiers", REDHOLD);
    s.screen.game.select_unit(soldier);
    type_char(&mut s, 'g');
    assert_eq!(s.picking(), Some(hexlords::session::Pick::GoTo));
    type_char(&mut s, 'x');
    push(&mut s, DISBAND_YES_ID);
    assert!(s.screen.game.map.units().get(soldier).is_none() && s.picking().is_none());
}

/// Once the game is over, the walls are raised no more.
#[test]
fn no_walls_are_raised_once_the_game_is_over() {
    let mut game = game("garrison soldiers 3\n", "", 100);
    let farwater = game.map.city_at(Hex::new(15, 15)).unwrap().id;
    game.map.city_mut(farwater).unwrap().side = Side::Red;
    let red = at(&game, REDHOLD);
    game.apply(Order::Move { units: red, to: BLUEMOOR }).unwrap();
    assert_eq!(game.winner(), Some(Side::Red));
    let redhold = game.map.city_at(REDHOLD).unwrap().id;
    assert!(game.walls_price(Side::Red, redhold).unwrap_err().contains("over"));
}

/// Late, a siege engine is no junk: the planner keeps the one it has (it
/// once sent home each it built, and built another, again and again).
#[test]
fn the_planner_keeps_its_siege_engine() {
    let mut game = game("garrison levy 8\n", "", 200);
    for row in [2, 3] {
        for _ in 0..8 {
            new_unit(&mut game, Side::Red, "levy", Hex::new(row, 2));
        }
    }
    let ram = new_unit(&mut game, Side::Red, "ram", Hex::new(4, 2));
    let mut planner = hexlords::planner::Planner::new(hexlords::planner::STEADY);
    for _ in 0..6 {
        let order = hexlords::agents::Agent::next_order(&mut planner, &game.view_for(Side::Red), Side::Red);
        if let Order::Disband { units } = &order {
            assert!(!units.contains(&ram), "it sent its siege engine home");
        }
        if game.apply(order).is_err() {
            break;
        }
    }
    assert!(game.map.units().get(ram).is_some());
}

/// The protocol writes a kind's bonus and siege, and reads back the walls,
/// disband and bless orders.
#[test]
fn the_protocol_says_a_kinds_bonus_and_reads_the_new_orders() {
    let game = game("garrison levy\n", "", 50);
    let text = hexlords::agents::describe(&game, Side::Red);
    assert!(text.lines().any(|l| l.starts_with("kind ram ") && l.contains(" siege 1 bonus none ")), "{text}");
    assert!(text.lines().any(|l| l.starts_with("kind ranger ") && l.contains(" bonus forest:2,city:1 ")), "{text}");
    let levy = at(&game, REDHOLD)[0];
    let name = hexlords::agents::unit_name(levy);
    let redhold = game.map.city_at(REDHOLD).unwrap().id;
    let parse = |line: &str| hexlords::agents::parse_order(&game, line);
    assert_eq!(parse("walls 5 5"), Ok(Some(Order::Walls { city: redhold })));
    assert_eq!(parse(&format!("disband {name}")), Ok(Some(Order::Disband { units: vec![levy] })));
    assert_eq!(parse(&format!("bless {name}")), Ok(Some(Order::Bless { units: vec![levy] })));
}

/// With fog of war, a city out of sight keeps the walls its side saw last.
#[test]
fn a_city_in_the_fog_has_the_walls_seen_last() {
    let game = game("garrison levy\n", "", 50);
    let mut setup = game.setup().clone();
    setup.fog = hexlords::setup::Fog::On;
    let mut game = game.with_setup(setup);
    // Red sees Bluemoor (walls 3) from a scout beside it; then the scout goes.
    let scout = new_unit(&mut game, Side::Red, "levy", Hex::new(5, 7));
    game.apply(Order::Move { units: vec![scout], to: Hex::new(5, 8) }).unwrap();
    assert!(game.sight(Side::Red).sees(BLUEMOOR));
    // (What its units saw on their way stays seen until its turn ends.)
    game.map.kill_unit(scout);
    game.end_turn();
    assert!(!game.sight(Side::Red).sees(BLUEMOOR));
    let bluemoor = game.map.city_at(BLUEMOOR).unwrap().id;
    game.map.city_mut(bluemoor).unwrap().walls = 5;
    assert_eq!(game.view_for(Side::Red).map.city_at(BLUEMOOR).unwrap().walls, 3);
    assert_eq!(game.sight(Side::Red).walls_seen(BLUEMOOR), Some(3));
}

/// With fog of war, a side lays siege only to one it outnumbers counting
/// the units it saw and sees no more (it once counted only those in sight,
/// and waved at walls against an army it had seen go by).
#[test]
fn a_siege_counts_the_units_seen_before() {
    let game = game("garrison levy 8\n", "garrison levy\n", 50);
    let mut setup = game.setup().clone();
    setup.fog = hexlords::setup::Fog::On;
    let mut game = game.with_setup(setup);
    // Red holds three cities to blue's one.
    let farwater = game.map.city_at(Hex::new(15, 15)).unwrap().id;
    game.map.city_mut(farwater).unwrap().side = Side::Red;
    // A scout sees blue's four knights by Bluemoor, then is gone.
    for _ in 0..4 {
        new_unit(&mut game, Side::Blue, "knights", Hex::new(5, 12));
    }
    let scout = new_unit(&mut game, Side::Red, "levy", Hex::new(6, 10));
    game.apply(Order::Move { units: vec![scout], to: Hex::new(5, 10) }).unwrap();
    game.map.kill_unit(scout);
    game.end_turn();
    let view = game.view_for(Side::Red);
    assert!(view.map.units().of_side(Side::Blue).next().is_none(), "none in sight");
    let sure = Some(hexlords::state::combat::Forecast { odds: 100, kills: 1.0, losses: 0.0 });
    // Nine red units against five blue it knows of: no siege (9 < 3 x 5).
    assert!(!hexlords::agents::siege(&view, Side::Red, BLUEMOOR, sure));
    // Seen long ago, they count no more (under total fog, sightings a
    // hundred rounds old barred every siege).
    while game.turn() <= 1 + hexlords::agents::SIEGE_MEMORY {
        game.end_turn();
    }
    assert!(hexlords::agents::siege(&game.view_for(Side::Red), Side::Red, BLUEMOOR, sure));
}

/// The generator's walls are 0 to 6, as high as walls go in the game.
#[test]
fn the_generators_walls_go_no_higher_than_the_games() {
    let mut cfg = hexlords::mapgen::config::Config::default();
    assert!(cfg.set("walls", "3:50 6:50").is_ok());
    assert!(cfg.set("walls", "3:50 7:50").unwrap_err().contains("above 6"));
}

/// The Help menu's topics say the rules of this round: the walls' price,
/// a kind's own ground and siege engines, Defend and Disband, and what an
/// undo takes back.
#[test]
fn the_help_topics_tell_the_new_rules() {
    let game = game("", "", 50);
    let text = |id: &str| -> String {
        let (_, sections) = hexlords::guide::topic(&game, id).unwrap();
        // (Wrapped: a row with no name goes on from the one before.)
        let rows = sections.iter().flat_map(|s| s.rows.iter());
        rows.map(|(a, b)| if a.is_empty() { format!(" {b}") } else { format!("\n{a}: {b}") }).collect::<String>()
    };
    let battles = text("help_battles");
    assert!(battles.contains("Their own ground") && battles.contains("Ranger: fights +2 in forests"), "{battles}");
    assert!(battles.contains("Ram: breaks 1 of the walls it attacks"), "{battles}");
    let cities = text("help_cities");
    assert!(cities.contains("25 gold for each level") && cities.contains("loses a level"), "{cities}");
    let orders = text("help_orders");
    assert!(orders.contains("Defend: F:") && orders.contains("Disband: X:"), "{orders}");
    assert!(orders.contains("a city's work or its walls"), "{orders}");
}

/// Under total fog the planner keeps a scout looking: late, it never
/// sends that one home to make room (it sent its scout home, built
/// another, and sent that home, hundreds of times a game).
#[test]
fn the_planner_keeps_its_scout() {
    let mut game = game("garrison levy 8\n", "", 200);
    let mut setup = game.setup().clone();
    setup.fog = hexlords::setup::Fog::Total;
    game = game.with_setup(setup);
    for row in [2, 3] {
        for _ in 0..8 {
            new_unit(&mut game, Side::Red, "levy", Hex::new(row, 2));
        }
    }
    // The fastest, as weak as a levy and dearer to keep: the first to go,
    // were it not the scout.
    let runner = new_unit(&mut game, Side::Red, "runner", Hex::new(4, 2));
    let mut planner = hexlords::planner::Planner::new(hexlords::planner::STEADY);
    for _ in 0..3 {
        let report = hexlords::agents::play_turn(&mut game, &mut planner);
        let gone = report
            .events
            .iter()
            .any(|e| matches!(e, Event::Disbanded { names, .. } if names.contains(&"Runner".to_string())));
        assert!(!gone && game.map.units().get(runner).is_some(), "it sent its scout home");
        while game.player() != Side::Red {
            game.end_turn();
        }
    }
}
