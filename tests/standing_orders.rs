//! Orders that last: a stack sent somewhere far goes there over turns (Go
//! to), and stops short of another side's units to wait for its player; a
//! city sends what it builds somewhere on its own; a stack can be put aside
//! for the turn (Done); the list of tasks says what is left to do. And a
//! captain moves as fast as the fastest of its stack.

mod common;

use common::{assets, choose, click, pixel_of, press, type_char, widget};
use hexgame::city_windows::{CITY_NO_RALLY_ID, CITY_RALLY_ID, PANEL_OPEN_ID};
use hexgame::game::{CityState, Game, StackState, Task};
use hexgame::orders::{Event, Order};
use hexgame::session::{DONE_ID, GO_TO_ID, MENU_VIEW_ID, Pick, Session, TASKS_ID};
use hexgame::setup::Setup;
use hexgame::state::scenario::{self, Scenario};
use hexgame::state::{Hex, Side, TileType, UnitId};
use hexgame::tasks_window::row_id;
use turbo_ferret::{Point, Size, SpecialKey};

const REDHOLD: Hex = Hex::new(5, 5);
const SOUTHWICK: Hex = Hex::new(15, 5);
const BLUEMOOR: Hex = Hex::new(5, 25);
const FAR: Hex = Hex::new(5, 20);

/// 30 x 30 grass: red's Redhold with `red` and Southwick with nobody in it,
/// blue's Bluemoor far to the east. Soldiers go 6 hexes a turn, riders 10.
fn game(red: &str) -> Game {
    let mut rows = vec!["g".repeat(30); 30];
    rows[5] = format!("{}c{}c{}", "g".repeat(5), "g".repeat(19), "g".repeat(4));
    rows[15] = format!("{}c{}", "g".repeat(5), "g".repeat(24));
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nprice 5\nenter grass 2, city 1\n\
                 unit riders\nname Riders\nstrength 3\nmoves 20\nprice 8\nenter grass 2, city 1\n\
                 unit captain\nname Captain\nstrength 2\nmoves 12\ncommander 1\nenter grass 2, city 1\n";
    let cities = format!(
        "city 5 5\nname Redhold\nside red\nproduce soldiers 1\n{red}city 15 5\nname Southwick\nside red\n\
         produce soldiers 2\ncity 5 25\nname Bluemoor\nside blue\n"
    );
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(&cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup.gold = 500;
    game.with_setup(setup)
}

fn at(game: &Game, hex: Hex) -> Vec<UnitId> {
    game.map.units_at(hex).to_vec()
}

fn pos(game: &Game, id: UnitId) -> Hex {
    game.map.units().get(id).unwrap().pos()
}

fn goto(game: &Game, id: UnitId) -> Option<Hex> {
    game.map.units().get(id).unwrap().goto
}

/// Red's turn ends, blue's (it does nothing) too: red's next begins.
fn round(game: &mut Game) -> Vec<Event> {
    let mut events = game.apply(Order::EndTurn).unwrap();
    events.extend(game.apply(Order::EndTurn).unwrap());
    assert_eq!(game.player(), Side::Red);
    events
}

#[test]
fn a_stack_sent_far_goes_there_over_turns() {
    let mut game = game("garrison soldiers 2\n");
    let red = at(&game, REDHOLD);
    // This turn's part at once: as far as soldiers go (6 hexes), and the
    // order stands.
    game.apply(Order::GoTo { units: red.clone(), to: FAR }).unwrap();
    let here = pos(&game, red[0]);
    assert_eq!((here.distance(REDHOLD), here.distance(FAR)), (6, 9));
    assert!(red.iter().all(|id| pos(&game, *id) == here && goto(&game, *id) == Some(FAR)));
    // It is not waiting for orders: it is on its way.
    let stack = game.tasks(Side::Red).into_iter().find(|t| matches!(t, Task::Stack { .. }));
    assert_eq!(stack, Some(Task::Stack { at: here, units: 2, state: StackState::Marching }));
    // The next turns, on its own, at the turn's start: 6 more, then there.
    let events = round(&mut game);
    assert!(events.iter().any(|e| matches!(e, Event::Moved { unit, .. } if *unit == red[0])));
    assert_eq!(pos(&game, red[0]).distance(FAR), 3);
    round(&mut game);
    assert!(red.iter().all(|id| pos(&game, *id) == FAR && goto(&game, *id).is_none()), "there; the order ends");
    // The text protocol says it, and reads it back.
    assert_eq!(
        hexgame::agents::parse_order(&game, "goto red.0.0 5 21"),
        Ok(Some(Order::GoTo { units: vec![red[0]], to: Hex::new(5, 21) }))
    );
}

#[test]
fn a_march_stops_short_of_another_sides_units_and_waits_for_orders() {
    let mut game = game("garrison soldiers\n");
    let red = at(&game, REDHOLD);
    // A blue soldier on the way, 8 hexes out.
    let kind = game.map.units().kind_named("soldiers").unwrap();
    let blue = game.map.new_unit_of(Side::Blue, kind).unwrap();
    game.map.place_unit(blue, Hex::new(5, 13)).unwrap();
    game.apply(Order::GoTo { units: red.clone(), to: Hex::new(5, 13) }).unwrap();
    assert_eq!(pos(&game, red[0]).distance(Hex::new(5, 13)), 2);
    // Next turn: the step it would take is onto blue. It does not attack: it
    // stops, keeps its order, and waits for its player.
    let events = round(&mut game);
    let stalled = events.iter().find_map(|e| match e {
        Event::Stalled { units, to, .. } => Some((units.clone(), *to)),
        _ => None,
    });
    assert_eq!(stalled, Some((red.clone(), Hex::new(5, 13))));
    let unit = game.map.units().get(red[0]).unwrap();
    let unit_at = unit.pos();
    assert!(unit.stalled && unit.goto == Some(Hex::new(5, 13)));
    assert_eq!(game.next_stack(None, false), Some(unit.pos()), "back with the stacks that wait for orders");
    assert!(game.tasks(Side::Red).iter().any(|t| matches!(t, Task::Stack { state: StackState::Stalled, .. })));
    // Its log says so.
    let record = game.history().iter().find(|r| matches!(r.event, Event::Stalled { .. })).unwrap();
    let lines = hexgame::info::record_lines(&game.map, record, Side::Red);
    let line = format!(
        "One of your units stopped at {} on the way to row 5, column 13",
        hexgame::info::place(&game.map, unit_at)
    );
    assert!(lines[0].starts_with(&line) && lines[0].ends_with("It waits for orders."), "{lines:?}");
    // Left to itself, it goes on when the way is free.
    game.map.kill_unit(blue);
    round(&mut game);
    assert_eq!(pos(&game, red[0]), Hex::new(5, 13));
    assert_eq!(goto(&game, red[0]), None);
}

#[test]
fn a_move_ends_a_standing_order_and_undo_gives_it_back() {
    let mut game = game("garrison soldiers\n");
    let id = at(&game, REDHOLD)[0];
    game.apply(Order::GoTo { units: vec![id], to: FAR }).unwrap();
    assert!(game.can_undo());
    // Taken back: home, and no order.
    game.apply(Order::Undo).unwrap();
    assert_eq!((pos(&game, id), goto(&game, id)), (REDHOLD, None));
    game.apply(Order::GoTo { units: vec![id], to: FAR }).unwrap();
    round(&mut game);
    let here = pos(&game, id);
    assert_eq!(goto(&game, id), Some(FAR));
    // (As if the march had left it moves.)
    game.map.unit_mut(id).unwrap().movement = 12;
    // A move of its own ends it ...
    let next = game.map.neighbours(here).next().unwrap();
    game.apply(Order::Move { units: vec![id], to: next }).unwrap();
    assert_eq!(goto(&game, id), None);
    // ... and taking the move back gives it back.
    game.apply(Order::Undo).unwrap();
    assert_eq!((pos(&game, id), goto(&game, id)), (here, Some(FAR)));
    // Nowhere to go: refused.
    assert!(game.apply(Order::GoTo { units: vec![id], to: Hex::new(99, 99) }).is_err());
}

#[test]
fn a_city_sends_what_it_builds_where_its_holder_says() {
    let mut game = game("");
    let city = game.map.city_at(REDHOLD).unwrap().id;
    game.apply(Order::Build { city, what: Some(0) }).unwrap();
    game.apply(Order::Rally { city, to: Some(FAR) }).unwrap();
    assert_eq!(game.map.cities().get(city).unwrap().rally, Some(FAR));
    // Built at the start of red's turn, and on its way at once.
    let events = round(&mut game);
    let built = events.iter().find_map(|e| if let Event::Built { unit, .. } = e { Some(*unit) } else { None });
    let built = built.expect("soldiers built");
    assert_eq!(goto(&game, built), Some(FAR));
    assert_eq!(pos(&game, built).distance(REDHOLD), 6);
    // Only its holder says where.
    let bluemoor = game.map.city_at(BLUEMOOR).unwrap().id;
    assert!(game.apply(Order::Rally { city: bluemoor, to: Some(FAR) }).is_err());
    // Nowhere: what it builds stays in it.
    game.apply(Order::Rally { city, to: None }).unwrap();
    assert_eq!(game.map.cities().get(city).unwrap().rally, None);
    assert_eq!(hexgame::agents::parse_order(&game, "rally 5 5 5 20"), Ok(Some(Order::Rally { city, to: Some(FAR) })));
    assert_eq!(hexgame::agents::parse_order(&game, "rally 5 5 none"), Ok(Some(Order::Rally { city, to: None })));
}

#[test]
fn done_puts_a_stack_aside_for_the_turn() {
    let mut game = game("garrison soldiers\n");
    let a = at(&game, REDHOLD);
    let kind = game.map.units().kind_named("soldiers").unwrap();
    let b = game.map.new_unit_of(Side::Red, kind).unwrap();
    game.map.place_unit(b, SOUTHWICK).unwrap();
    assert_eq!(game.next_stack(None, false), Some(REDHOLD));
    // The order before can still be taken back: Done changes nothing to take
    // back.
    game.apply(Order::Move { units: vec![b], to: Hex::new(16, 5) }).unwrap();
    game.apply(Order::Done { units: a.clone() }).unwrap();
    assert!(game.can_undo());
    assert_eq!(game.next_stack(None, false), Some(Hex::new(16, 5)), "Redhold's is passed by");
    let tasks = game.tasks(Side::Red);
    assert!(tasks.contains(&Task::Stack { at: REDHOLD, units: 1, state: StackState::Done }));
    // Another side's units are not red's to put aside.
    let kind = game.map.units().kind_named("soldiers").unwrap();
    let blue = game.map.new_unit_of(Side::Blue, kind).unwrap();
    game.map.place_unit(blue, BLUEMOOR).unwrap();
    assert!(game.apply(Order::Done { units: vec![blue] }).is_err());
    // A new turn, a new start.
    round(&mut game);
    assert!(!game.map.units().get(a[0]).unwrap().done);
    assert_eq!(hexgame::agents::parse_order(&game, "done red.0.0"), Ok(Some(Order::Done { units: a })));
}

#[test]
fn the_tasks_come_most_pressing_first() {
    let mut game = game("garrison soldiers\ngarrison riders\n");
    let ids = at(&game, REDHOLD);
    let southwick = game.map.city_at(SOUTHWICK).unwrap().id;
    game.apply(Order::Build { city: southwick, what: Some(0) }).unwrap();
    // The riders go a little way (moves left); the soldiers do not move.
    game.apply(Order::Move { units: vec![ids[1]], to: Hex::new(6, 5) }).unwrap();
    let redhold = game.map.city_at(REDHOLD).unwrap().id;
    let tasks = game.tasks(Side::Red);
    assert_eq!(
        tasks,
        [
            Task::Stack { at: REDHOLD, units: 1, state: StackState::Waiting },
            Task::City { city: redhold, state: CityState::Idle },
            Task::Stack { at: Hex::new(6, 5), units: 1, state: StackState::MovesLeft },
            Task::City { city: southwick, state: CityState::Building },
        ]
    );
    assert_eq!(tasks.iter().map(Task::tier).collect::<Vec<_>>(), [1, 1, 2, 3]);
    let lines: Vec<String> = tasks.iter().map(|t| hexgame::info::task_line(&game, t)).collect();
    assert_eq!(
        lines,
        [
            "Soldiers in Redhold: no orders yet",
            "Redhold: builds nothing",
            "Riders at row 6, column 5: can go further",
            "Southwick: building Soldiers, 0/2",
        ]
    );
}

#[test]
fn a_captain_moves_as_fast_as_the_fastest_of_its_stack() {
    let mut game = game("garrison captain\ngarrison riders\n");
    let kind = game.map.units().kind_named("captain").unwrap();
    let alone = game.map.new_unit_of(Side::Red, kind).unwrap();
    game.map.place_unit(alone, SOUTHWICK).unwrap();
    round(&mut game);
    let units = game.map.units();
    let captain = at(&game, REDHOLD).into_iter().find(|id| game.map.is_commander(*id)).unwrap();
    let riders = at(&game, REDHOLD).into_iter().find(|id| !game.map.is_commander(*id)).unwrap();
    // With riders, it rides; alone, it walks.
    assert_eq!(units.get(captain).unwrap().movement, units.get(riders).unwrap().movement);
    assert!(units.get(captain).unwrap().movement > units.get(alone).unwrap().movement);
}

// The screen.

fn session(game: Game) -> Session {
    Session::with_game(&assets(), Size::new(1024, 768), 96, game).expect("the session starts")
}

fn push(s: &mut Session, name: &str) {
    let r = s.screen.area(widget(s, name));
    click(s, Point::new(r.x + r.w / 2, r.y + r.h / 2));
}

fn click_hex(s: &mut Session, hex: Hex) {
    s.screen.view.center_on(hex);
    let p = pixel_of(s, hex);
    click(s, p);
}

#[test]
fn go_to_is_a_key_or_a_button_and_then_a_hex() {
    let mut s = session(game("garrison soldiers\n"));
    let id = at(&s.screen.game, REDHOLD)[0];
    // Nothing selected: nothing to send.
    assert!(!type_char(&mut s, 'g') || s.picking().is_none());
    assert!(s.picking().is_none());
    click_hex(&mut s, REDHOLD);
    assert_eq!(s.screen.game.selected(), Some(id));
    type_char(&mut s, 'g');
    assert_eq!(s.picking(), Some(Pick::GoTo));
    assert!(s.unit_line().starts_with("Go to: click the hex"), "{}", s.unit_line());
    // Esc: never mind; the selection stays.
    press(&mut s, SpecialKey::Esc);
    assert_eq!((s.picking(), s.screen.game.selected()), (None, Some(id)));
    // The button, then a hex they cannot get to (water): refused, and said
    // why; another pick.
    s.screen.game.map.tile_mut(Hex::new(9, 9)).unwrap().kind = TileType::Water;
    push(&mut s, GO_TO_ID);
    assert_eq!(s.picking(), Some(Pick::GoTo));
    click_hex(&mut s, Hex::new(9, 9));
    assert_eq!(s.picking(), Some(Pick::GoTo));
    assert_eq!(s.unit_line(), "Go to: there is no way there. Click another hex (Esc: never mind).");
    // A far hex (out of this turn's reach).
    click_hex(&mut s, FAR);
    let unit = s.screen.game.map.units().get(id).unwrap();
    assert_eq!((unit.goto, unit.pos().distance(REDHOLD)), (Some(FAR), 6));
    let here = unit.pos();
    assert_eq!(s.screen.game.selected(), None);
    // Selected again, its line says where it goes.
    click_hex(&mut s, here);
    assert!(s.unit_line().ends_with("(on the way to row 5, column 20)"), "{}", s.unit_line());
}

#[test]
fn done_goes_on_to_the_next_stack_and_tasks_lists_what_is_left() {
    let mut red = game("garrison soldiers\n");
    let kind = red.map.units().kind_named("soldiers").unwrap();
    let other = red.map.new_unit_of(Side::Red, kind).unwrap();
    red.map.place_unit(other, SOUTHWICK).unwrap();
    let mut s = session(red);
    click_hex(&mut s, REDHOLD);
    let first = s.screen.game.selected().unwrap();
    push(&mut s, DONE_ID);
    assert!(s.screen.game.map.units().get(first).unwrap().done);
    assert_eq!(s.screen.game.selected(), Some(other), "on to the next");
    // The list: Southwick's soldiers first, Redhold's put aside, the cities.
    type_char(&mut s, 't');
    let lines = s.tasks_window().unwrap().lines().to_vec();
    assert_eq!(lines[0], "Soldiers in Southwick: no orders yet");
    assert!(lines.contains(&"Soldiers in Redhold: put aside, with moves left".to_string()), "{lines:?}");
    // A row of a stack selects it; of a city, opens it.
    let i = lines.iter().position(|l| l.starts_with("Soldiers in Redhold")).unwrap();
    push(&mut s, &row_id(i));
    assert_eq!(s.screen.game.selected(), Some(first));
    let i = lines.iter().position(|l| l.starts_with("Southwick:")).unwrap();
    push(&mut s, &row_id(i));
    assert_eq!(s.city_window().map(|w| w.city()), s.screen.game.map.city_at(SOUTHWICK).map(|c| c.id));
    // It follows the game: D on Southwick's, and its row says so.
    press(&mut s, SpecialKey::Esc);
    assert!(s.city_window().is_none() && s.tasks_window().is_some());
    click_hex(&mut s, SOUTHWICK);
    type_char(&mut s, 'd');
    let lines = s.tasks_window().unwrap().lines().to_vec();
    assert!(lines.iter().all(|l| !l.contains("no orders")), "{lines:?}");
    // T, or the menu, closes it.
    choose(&mut s, MENU_VIEW_ID, TASKS_ID);
    assert!(s.tasks_window().is_none());
}

#[test]
fn a_city_window_sends_new_units_to_a_picked_hex() {
    let mut s = session(game("garrison soldiers\n"));
    click_hex(&mut s, REDHOLD);
    press(&mut s, SpecialKey::Esc);
    push(&mut s, PANEL_OPEN_ID);
    let says = |s: &Session| -> Vec<String> {
        common::descendants(s, hexgame::city_windows::CITY_WINDOW_ID)
            .into_iter()
            .filter_map(|id| s.screen.ui.text(id).map(str::to_string))
            .collect()
    };
    assert!(says(&s).contains(&"New units stay in the city.".to_string()));
    push(&mut s, CITY_RALLY_ID);
    assert!(matches!(s.picking(), Some(Pick::Rally(_))));
    click_hex(&mut s, Hex::new(10, 10));
    let city = s.screen.game.map.city_at(REDHOLD).unwrap();
    assert_eq!(city.rally, Some(Hex::new(10, 10)));
    assert!(says(&s).contains(&"New units go to row 10, column 10.".to_string()), "{:?}", says(&s));
    push(&mut s, CITY_NO_RALLY_ID);
    assert_eq!(s.screen.game.map.city_at(REDHOLD).unwrap().rally, None);
}
