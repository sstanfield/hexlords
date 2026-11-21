//! Standing orders with stops on the way (the owner, 2026-11-21: "when a
//! unit appears, march to a temple, get a blessing, then march to a rally
//! point"): Go to by way of stops, a stop on a temple for its blessing, a
//! city's new units sent so, the text protocol, and Shift+click on the
//! screen.

mod common;

use common::{assets, click, pixel_of, press, push_widget, shift_click, shift_press, type_char};
use hexgame::city_windows::{CITY_RALLY_ID, PANEL_OPEN_ID};
use hexgame::game::{Game, StackState, Task};
use hexgame::orders::{Event, Order};
use hexgame::screen::Scene;
use hexgame::session::{Pick, Session};
use hexgame::setup::Setup;
use hexgame::shell::Shell;
use hexgame::state::scenario::{self, Scenario};
use hexgame::state::{Blessing, Hex, Side, TileType, UnitId};
use turbo_ferret::{Size, SpecialKey};

const REDHOLD: Hex = Hex::new(5, 5);
const TEMPLE: Hex = Hex::new(5, 8);
const FAR: Hex = Hex::new(5, 20);
const LAKE: Hex = Hex::new(20, 20);

/// 30 x 30 grass: red's Redhold with `red`, a temple 3 hexes east of it, a
/// lake far to the south east, blue's Bluemoor far to the east. Soldiers go
/// 6 hexes a turn.
fn game(red: &str) -> Game {
    let mut rows = vec!["g".repeat(30); 30];
    rows[5] = format!("{}c{}t{}c{}", "g".repeat(5), "g".repeat(2), "g".repeat(16), "g".repeat(4));
    rows[20] = format!("{}w{}", "g".repeat(20), "g".repeat(9));
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nprice 5\nenter grass 2, city 1\n";
    let cities =
        format!("city 5 5\nname Redhold\nside red\nproduce soldiers 1\n{red}city 5 25\nname Bluemoor\nside blue\n");
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(&cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    assert_eq!(game.map.tile(TEMPLE).unwrap().kind, TileType::Temple);
    assert_eq!(game.map.tile(LAKE).unwrap().kind, TileType::Water);
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup.gold = 500;
    game.with_setup(setup)
}

fn at(game: &Game, hex: Hex) -> Vec<UnitId> {
    game.map.units_at(hex).to_vec()
}

fn unit(game: &Game, id: UnitId) -> &hexgame::state::Unit {
    game.map.units().get(id).unwrap()
}

/// Red's turn ends, blue's (it does nothing) too: red's next begins.
fn round(game: &mut Game) -> Vec<Event> {
    let mut events = game.apply(Order::EndTurn).unwrap();
    events.extend(game.apply(Order::EndTurn).unwrap());
    assert_eq!(game.player(), Side::Red);
    events
}

#[test]
fn a_route_goes_by_way_of_its_stops_and_on_at_once() {
    let mut game = game("garrison soldiers\n");
    let id = at(&game, REDHOLD)[0];
    let stop = Hex::new(7, 5);
    game.apply(Order::GoTo { units: vec![id], to: FAR, via: vec![stop] }).unwrap();
    // Two hexes to the stop, and on toward the end with what is left: four.
    let soldiers = unit(&game, id);
    assert_eq!((soldiers.route.clone(), soldiers.movement), (vec![FAR], 0));
    assert_eq!(soldiers.pos().distance(stop), 4);
    assert!(!game.can_undo(), "two legs: not to be taken back");
    // It is on its way, and goes on at the start of the turns.
    let on_way = game.tasks(Side::Red).into_iter().find(|t| matches!(t, Task::Stack { .. })).unwrap();
    assert!(matches!(on_way, Task::Stack { state: StackState::Marching, .. }));
    round(&mut game);
    round(&mut game);
    let soldiers = unit(&game, id);
    assert_eq!((soldiers.pos(), soldiers.route.is_empty()), (FAR, true));
    // There: it waits for orders.
    let there = game.tasks(Side::Red).into_iter().find(|t| matches!(t, Task::Stack { .. })).unwrap();
    assert_eq!(there, Task::Stack { at: FAR, units: 1, state: StackState::Waiting });
}

#[test]
fn a_stop_on_a_temple_is_for_its_blessing() {
    let mut game = game("garrison soldiers 2\n");
    let red = at(&game, REDHOLD);
    // The temple has blessed one of them already.
    game.map.unit_mut(red[1]).unwrap().bless(TEMPLE, Blessing::Strength);
    let events = game.apply(Order::GoTo { units: red.clone(), to: FAR, via: vec![TEMPLE] }).unwrap();
    // The other is blessed, and they stop there for the turn, together.
    let blessed: Vec<UnitId> =
        events.iter().filter_map(|e| if let Event::Blessed { unit, .. } = e { Some(*unit) } else { None }).collect();
    assert_eq!(blessed, [red[0]]);
    assert!(unit(&game, red[0]).blessed_at(TEMPLE));
    assert!(red.iter().all(|id| unit(&game, *id).pos() == TEMPLE && unit(&game, *id).route == [FAR]));
    assert!(!game.can_undo(), "chance had a say");
    // On, the next turn.
    round(&mut game);
    assert!(red.iter().all(|id| unit(&game, *id).pos().distance(TEMPLE) >= 6));
    // A temple that has blessed them all is a stop like another: on at once.
    let kind = game.map.units().kind_named("soldiers").unwrap();
    let blessed_already = game.map.new_unit_of(Side::Red, kind).unwrap();
    game.map.place_unit(blessed_already, REDHOLD).unwrap();
    game.map.unit_mut(blessed_already).unwrap().bless(TEMPLE, Blessing::Strength);
    let events = game.apply(Order::GoTo { units: vec![blessed_already], to: FAR, via: vec![TEMPLE] }).unwrap();
    assert!(!events.iter().any(|e| matches!(e, Event::Blessed { .. })));
    assert_eq!(unit(&game, blessed_already).pos().distance(TEMPLE), 3);
}

#[test]
fn a_city_sends_what_it_builds_by_way_of_stops() {
    let mut game = game("");
    let city = game.map.city_at(REDHOLD).unwrap().id;
    game.apply(Order::Build { city, what: Some(0) }).unwrap();
    // Stops on the way to nowhere: refused. Stops they cannot get to too.
    assert!(game.apply(Order::Rally { city, to: None, via: vec![TEMPLE] }).is_err());
    let why = game.apply(Order::Rally { city, to: Some(FAR), via: vec![LAKE] }).unwrap_err();
    assert_eq!(why, "nothing Redhold builds can get there");
    game.apply(Order::Rally { city, to: Some(FAR), via: vec![TEMPLE] }).unwrap();
    assert_eq!(game.map.cities().get(city).unwrap().rally, [TEMPLE, FAR]);
    let line = hexgame::info::task_line(
        &game,
        &game.tasks(Side::Red).into_iter().find(|t| matches!(t, Task::City { .. })).unwrap(),
    );
    assert_eq!(
        line,
        "Redhold: building Soldiers, 0/1; new units go to the temple at row 5, column 8, then row 5, column 20"
    );
    // Built at the start of red's turn, and to the temple at once, for its
    // blessing.
    let events = round(&mut game);
    let built = events.iter().find_map(|e| if let Event::Built { unit, .. } = e { Some(*unit) } else { None });
    let built = built.expect("soldiers built");
    assert!(events.iter().any(|e| matches!(e, Event::Blessed { unit, .. } if *unit == built)));
    assert_eq!((unit(&game, built).pos(), unit(&game, built).route.clone()), (TEMPLE, vec![FAR]));
}

#[test]
fn a_route_cannot_go_where_they_cannot() {
    let mut game = game("garrison soldiers\n");
    let id = at(&game, REDHOLD)[0];
    let why = game.apply(Order::GoTo { units: vec![id], to: FAR, via: vec![LAKE] }).unwrap_err();
    assert_eq!(why, "Soldiers cannot get there");
    assert_eq!(game.check_route(Side::Red, &[id], &[TEMPLE, FAR]), Ok(()));
    assert!(game.check_route(Side::Red, &[id], &[TEMPLE, LAKE]).is_err());
    assert_eq!((unit(&game, id).pos(), unit(&game, id).route.is_empty()), (REDHOLD, true));
}

#[test]
fn the_text_protocol_says_the_stops_in_order() {
    let game = game("garrison soldiers\n");
    let id = at(&game, REDHOLD)[0];
    let city = game.map.city_at(REDHOLD).unwrap().id;
    let goto = Order::GoTo { units: vec![id], to: FAR, via: vec![TEMPLE] };
    assert_eq!(hexgame::agents::parse_order(&game, "goto red.0.0 5 8 5 20"), Ok(Some(goto.clone())));
    assert_eq!(hexgame::headless::order_line(&game, &goto), "goto red.0.0 5 8 5 20");
    let rally = Order::Rally { city, to: Some(FAR), via: vec![TEMPLE] };
    assert_eq!(hexgame::agents::parse_order(&game, "rally 5 5 5 8 5 20"), Ok(Some(rally.clone())));
    assert_eq!(hexgame::headless::order_line(&game, &rally), "rally 5 5 5 8 5 20");
    // Without stops, as before.
    let straight = Order::GoTo { units: vec![id], to: FAR, via: Vec::new() };
    assert_eq!(hexgame::agents::parse_order(&game, "goto red.0.0 5 20"), Ok(Some(straight)));
    // A row without its column.
    assert!(hexgame::agents::parse_order(&game, "goto red.0.0 5 8 5").is_err());
}

// The screen.

fn session(game: Game) -> Session {
    Session::with_game(&assets(), Size::new(1024, 768), 96, game).expect("the session starts")
}

fn click_hex(s: &mut Session, hex: Hex) {
    s.screen.view.center_on(hex);
    let p = pixel_of(s, hex);
    click(s, p);
}

fn shift_click_hex(s: &mut Session, hex: Hex) {
    s.screen.view.center_on(hex);
    let p = pixel_of(s, hex);
    shift_click(s, p);
}

#[test]
fn shift_click_after_go_to_makes_a_stop() {
    let mut s = session(game("garrison soldiers\n"));
    let id = at(&s.screen.game, REDHOLD)[0];
    click_hex(&mut s, REDHOLD);
    type_char(&mut s, 'g');
    assert!(s.unit_line().contains("(Shift+click: a stop on the way; Esc: never mind)"), "{}", s.unit_line());
    // Somewhere they cannot get to: said why, and nothing added.
    shift_click_hex(&mut s, LAKE);
    assert_eq!(s.unit_line(), "Go to: Soldiers cannot get there. Click another hex (Esc: never mind).");
    shift_click_hex(&mut s, TEMPLE);
    assert_eq!(
        s.unit_line(),
        "Go to (by way of the temple at row 5, column 8): click where they end (Shift+click: another stop)."
    );
    // Shift+Enter at the cursor: another.
    s.screen.game.set_cursor(Hex::new(8, 8));
    shift_press(&mut s, SpecialKey::Enter);
    assert!(s.unit_line().contains("then row 8, column 8"), "{}", s.unit_line());
    assert_eq!(s.picking(), Some(Pick::GoTo));
    click_hex(&mut s, FAR);
    assert_eq!(s.picking(), None);
    // To the temple, blessed, and stopped there for the turn.
    let soldiers = unit(&s.screen.game, id);
    assert_eq!((soldiers.pos(), soldiers.route.clone()), (TEMPLE, vec![Hex::new(8, 8), FAR]));
    assert!(soldiers.blessed_at(TEMPLE));
    click_hex(&mut s, TEMPLE);
    assert!(s.unit_line().ends_with("(on the way to row 8, column 8, then row 5, column 20)"), "{}", s.unit_line());
    // Esc forgets the stops picked: a new Go to starts with none.
    type_char(&mut s, 'g');
    shift_click_hex(&mut s, Hex::new(7, 8));
    press(&mut s, SpecialKey::Esc);
    type_char(&mut s, 'g');
    assert!(s.unit_line().starts_with("Go to: click the hex"), "{}", s.unit_line());
}

#[test]
fn shift_click_makes_a_stop_for_a_citys_new_units() {
    let mut s = session(game(""));
    click_hex(&mut s, REDHOLD);
    press(&mut s, SpecialKey::Esc);
    push_widget(&mut s, PANEL_OPEN_ID);
    push_widget(&mut s, CITY_RALLY_ID);
    assert!(matches!(s.picking(), Some(Pick::Rally(_))));
    shift_click_hex(&mut s, LAKE);
    assert!(s.unit_line().starts_with("New units go to: nothing Redhold builds can get there."), "{}", s.unit_line());
    shift_click_hex(&mut s, TEMPLE);
    click_hex(&mut s, FAR);
    assert_eq!(s.screen.game.map.city_at(REDHOLD).unwrap().rally, [TEMPLE, FAR]);
    let says: Vec<String> = common::descendants(&s, hexgame::city_windows::CITY_WINDOW_ID)
        .into_iter()
        .filter_map(|id| s.screen.ui.text(id).map(str::to_string))
        .collect();
    assert!(
        says.contains(&"New units go to the temple at row 5, column 8, then row 5, column 20.".to_string()),
        "{says:?}"
    );
}

/// The map outlines the stops: those picked so far, or the selected unit's
/// route; in the window too, where the game is a scene of the shell's.
#[test]
fn the_map_outlines_the_stops() {
    let mut s = session(game("garrison soldiers\n"));
    click_hex(&mut s, REDHOLD);
    assert_eq!(s.outlined(), []);
    type_char(&mut s, 'g');
    shift_click_hex(&mut s, Hex::new(7, 5));
    assert_eq!(s.outlined(), [Hex::new(7, 5)]);
    let mut shell = Shell::playing(&assets(), s);
    assert_eq!(shell.outlined(), [Hex::new(7, 5)]);
    let s = shell.session().expect("the game");
    click_hex(s, FAR);
    let id = at(&s.screen.game, Hex::new(7, 5)).first().copied();
    assert_eq!(id, None, "on its way");
    let id = s.screen.game.map.units().of_side(Side::Red).next().unwrap().id();
    let here = unit(&s.screen.game, id).pos();
    click_hex(s, here);
    assert_eq!(s.outlined(), [FAR]);
}
