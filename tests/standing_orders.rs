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
    let c = game.map.new_unit_of(Side::Red, kind).unwrap();
    game.map.place_unit(c, Hex::new(20, 5)).unwrap();
    assert_eq!(game.next_stack(None, false), Some(REDHOLD));
    // The order before can still be taken back: Done changes nothing to take
    // back.
    game.apply(Order::Move { units: vec![b], to: Hex::new(16, 5) }).unwrap();
    game.apply(Order::Done { units: a.clone() }).unwrap();
    assert!(game.can_undo());
    assert_eq!(game.next_stack(None, false), Some(Hex::new(20, 5)), "Redhold's is passed by, and the moved one");
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
    // The riders have had their orders: among the rest.
    assert_eq!(tasks.iter().map(Task::tier).collect::<Vec<_>>(), [1, 1, 3, 3]);
    let lines: Vec<String> = tasks.iter().map(|t| hexgame::info::task_line(&game, t)).collect();
    assert_eq!(
        lines,
        [
            "Soldiers in Redhold: no orders yet",
            "Redhold: builds nothing",
            "Riders at row 6, column 5: moved, with moves left",
            "Southwick: building Soldiers, 0/2",
        ]
    );
}

/// A stack its player moved, and that stopped, has had its orders for the
/// turn, though some of it could go further: Next stack passes it by, and
/// the tasks put it with the rest, as they do a stack put aside (Done). A
/// click on its hex still selects it, to go on.
#[test]
fn a_stack_moved_or_put_aside_is_done_with_for_the_turn() {
    let mut game = game("garrison soldiers\ngarrison riders\n");
    let red = at(&game, REDHOLD);
    let kind = game.map.units().kind_named("soldiers").unwrap();
    let other = game.map.new_unit_of(Side::Red, kind).unwrap();
    game.map.place_unit(other, SOUTHWICK).unwrap();
    // Together, 6 hexes: the soldiers are spent, the riders are not.
    let there = Hex::new(5, 11);
    game.apply(Order::Move { units: red.clone(), to: there }).unwrap();
    let riders = red[1];
    assert!(game.map.units().get(riders).unwrap().movement > 0);
    assert_eq!(game.next_stack(None, false), Some(SOUTHWICK), "the moved stack is passed by");
    let task = |game: &Game, hex: Hex| {
        game.tasks(Side::Red).into_iter().find(|t| matches!(t, Task::Stack { at, .. } if *at == hex)).unwrap()
    };
    assert_eq!(task(&game, there), Task::Stack { at: there, units: 2, state: StackState::MovesLeft });
    assert_eq!(task(&game, there).tier(), 3);
    // Done: put aside, and as little pressing.
    game.apply(Order::Done { units: vec![other] }).unwrap();
    assert_eq!(task(&game, SOUTHWICK), Task::Stack { at: SOUTHWICK, units: 1, state: StackState::Done });
    assert_eq!(task(&game, SOUTHWICK).tier(), 3);
    assert!(game.tasks(Side::Red).iter().all(|t| t.tier() > 1 || matches!(t, Task::City { .. })));
    // A click on the moved stack: those of it that can go on.
    game.select_hex(there);
    assert_eq!(game.selection(), [riders]);
    game.deselect();
    // A stack that has not moved joins it: a click picks the newcomer, not
    // the riders, who have had their orders.
    let fresh = game.map.new_unit_of(Side::Red, kind).unwrap();
    game.map.place_unit(fresh, there).unwrap();
    assert_eq!(game.next_stack(None, false), Some(there));
    game.select_hex(there);
    assert_eq!(game.selection(), [fresh]);
}

/// A stack that gets where a standing order sent it at the start of the
/// turn has had no orders this turn: it waits for them. One sent there this
/// turn, and arrived, has had them.
#[test]
fn a_stack_that_arrives_on_a_standing_order_waits_for_orders() {
    let mut game = game("garrison soldiers\ngarrison riders\n");
    let red = at(&game, REDHOLD);
    let (soldiers, riders) = (red[0], red[1]);
    game.apply(Order::GoTo { units: vec![soldiers], to: FAR }).unwrap();
    // The riders, a little way, this turn.
    let near = Hex::new(5, 8);
    game.apply(Order::GoTo { units: vec![riders], to: near }).unwrap();
    assert_eq!((pos(&game, riders), goto(&game, riders)), (near, None));
    let task = |game: &Game, hex: Hex| {
        game.tasks(Side::Red).into_iter().find(|t| matches!(t, Task::Stack { at, .. } if *at == hex)).unwrap()
    };
    assert_eq!(task(&game, near), Task::Stack { at: near, units: 1, state: StackState::MovesLeft });
    round(&mut game);
    round(&mut game);
    assert_eq!((pos(&game, soldiers), goto(&game, soldiers)), (FAR, None));
    assert!(game.map.units().get(soldiers).unwrap().movement > 0);
    assert_eq!(task(&game, FAR), Task::Stack { at: FAR, units: 1, state: StackState::Waiting });
    assert_eq!(task(&game, FAR).tier(), 1);
    assert_eq!(game.next_stack(Some(near), false), Some(FAR));
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
    assert_eq!(s.unit_line(), "Go to: Soldiers cannot get there. Click another hex (Esc: never mind).");
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

// What the review found.

#[test]
fn go_to_is_taken_with_no_moves_left_and_marches_next_turn() {
    let mut game = game("garrison soldiers\n");
    let id = at(&game, REDHOLD)[0];
    game.apply(Order::Move { units: vec![id], to: Hex::new(5, 11) }).unwrap();
    assert_eq!(game.map.units().get(id).unwrap().movement, 0, "spent");
    // No moves left this turn: the order stands all the same.
    game.apply(Order::GoTo { units: vec![id], to: FAR }).unwrap();
    assert_eq!((pos(&game, id), goto(&game, id)), (Hex::new(5, 11), Some(FAR)));
    round(&mut game);
    assert!(pos(&game, id).distance(FAR) < Hex::new(5, 11).distance(FAR), "on its way");
    // A hex no unit of it can get to over the land is refused, naming it.
    game.map.tile_mut(Hex::new(20, 20)).unwrap().kind = hexgame::state::TileType::Water;
    let refused = game.apply(Order::GoTo { units: vec![id], to: Hex::new(20, 20) }).unwrap_err();
    assert_eq!(refused, "Soldiers cannot get there");
}

#[test]
fn go_to_from_several_hexes_sends_each_stack() {
    let mut game = game("garrison soldiers\n");
    let a = at(&game, REDHOLD)[0];
    let kind = game.map.units().kind_named("riders").unwrap();
    let b = game.map.new_unit_of(Side::Red, kind).unwrap();
    game.map.place_unit(b, Hex::new(5, 12)).unwrap();
    // The soldiers are already where the riders are sent: they stay, and the
    // riders go.
    let to = Hex::new(5, 5);
    game.apply(Order::GoTo { units: vec![a, b], to }).unwrap();
    assert_eq!(pos(&game, a), to);
    assert_eq!((pos(&game, b), goto(&game, b)), (to, None), "the riders went, and arrived");
}

#[test]
fn a_rally_point_is_somewhere_the_city_s_units_can_get_and_is_lost_with_the_city() {
    let mut game = game("");
    let city = game.map.city_at(REDHOLD).unwrap().id;
    game.map.tile_mut(Hex::new(20, 20)).unwrap().kind = hexgame::state::TileType::Water;
    let refused = game.apply(Order::Rally { city, to: Some(Hex::new(20, 20)) }).unwrap_err();
    assert_eq!(refused, "nothing Redhold builds can get there");
    game.apply(Order::Rally { city, to: Some(FAR) }).unwrap();
    // Blue takes it: where red sent its units is not blue's.
    let kind = game.map.units().kind_named("riders").unwrap();
    let blue = game.map.new_unit_of(Side::Blue, kind).unwrap();
    game.map.place_unit(blue, Hex::new(6, 5)).unwrap();
    game.apply(Order::EndTurn).unwrap();
    game.apply(Order::Move { units: vec![blue], to: REDHOLD }).unwrap();
    let redhold = game.map.cities().get(city).unwrap();
    assert_eq!((redhold.side, redhold.rally), (Side::Blue, None));
}

#[test]
fn undo_gives_a_stalled_stack_back_all_it_was() {
    let mut game = game("garrison soldiers\n");
    let id = at(&game, REDHOLD)[0];
    let kind = game.map.units().kind_named("soldiers").unwrap();
    let blue = game.map.new_unit_of(Side::Blue, kind).unwrap();
    game.map.place_unit(blue, Hex::new(5, 13)).unwrap();
    game.apply(Order::GoTo { units: vec![id], to: Hex::new(5, 13) }).unwrap();
    round(&mut game);
    let unit = game.map.units().get(id).unwrap();
    assert!(unit.stalled && !unit.moved);
    let here = unit.pos();
    let next = game.map.neighbours(here).find(|h| game.map.units_at(*h).is_empty()).unwrap();
    game.apply(Order::Move { units: vec![id], to: next }).unwrap();
    game.apply(Order::Undo).unwrap();
    let unit = game.map.units().get(id).unwrap();
    assert_eq!((unit.pos(), unit.goto, unit.stalled, unit.moved), (here, Some(Hex::new(5, 13)), true, false));
    assert!(game.tasks(Side::Red).iter().any(|t| matches!(t, Task::Stack { state: StackState::Stalled, .. })));
}

#[test]
fn what_a_turn_s_start_brings_is_that_turn_s_in_the_history() {
    let mut game = game("");
    let city = game.map.city_at(REDHOLD).unwrap().id;
    game.apply(Order::Build { city, what: Some(0) }).unwrap();
    round(&mut game);
    let built = game.history().iter().find(|r| matches!(r.event, Event::Built { .. })).unwrap();
    assert_eq!((built.turn, built.round), (Side::Red, 2), "red's, in the round it began");
}

#[test]
fn a_pick_does_not_outlive_its_turn() {
    let mut s = session(game("garrison soldiers\n"));
    click_hex(&mut s, REDHOLD);
    type_char(&mut s, 'g');
    assert_eq!(s.picking(), Some(Pick::GoTo));
    s.end_turn();
    assert_eq!(s.picking(), None);
}

#[test]
fn a_box_selects_the_unit_it_shows_the_leader_s_too() {
    let mut s = session(game("garrison captain\ngarrison soldiers\ngarrison riders\n"));
    let ids = at(&s.screen.game, REDHOLD);
    let named = |s: &Session, name: &str| {
        ids.iter().copied().find(|id| s.screen.game.map.units().get(*id).unwrap().title().starts_with(name)).unwrap()
    };
    let (captain, soldiers, riders) = (named(&s, "Captain"), named(&s, "Soldiers"), named(&s, "Riders"));
    click_hex(&mut s, REDHOLD);
    assert_eq!(s.screen.game.selection().len(), 3);
    // The boxes: the captain's on the left, then the soldiers, the riders.
    assert!(s.leader_icon().is_some());
    push(&mut s, "unit_1");
    assert!(!s.screen.game.selection().contains(&riders), "the riders' box leaves the riders out");
    assert!(s.screen.game.selection().contains(&soldiers));
    push(&mut s, hexgame::session::LEADER_ID);
    assert!(!s.screen.game.selection().contains(&captain), "the leader's box is the captain's");
}

#[test]
fn on_another_side_s_turn_the_screen_looks_and_orders_nothing() {
    let mut red = game("garrison soldiers\n");
    let kind = red.map.units().kind_named("soldiers").unwrap();
    let blue = red.map.new_unit_of(Side::Blue, kind).unwrap();
    red.map.place_unit(blue, Hex::new(5, 20)).unwrap();
    let mut s = session(red);
    s.end_turn();
    assert_eq!(s.screen.game.player(), Side::Blue, "the computer's turn, not played yet");
    press(&mut s, SpecialKey::Esc);
    // A click on blue's soldiers looks; it selects nothing, and a second
    // click moves nothing.
    click_hex(&mut s, Hex::new(5, 20));
    assert!(s.screen.game.selection().is_empty());
    assert!(s.stack().is_empty(), "blue's units are not shown in boxes");
    click_hex(&mut s, Hex::new(5, 18));
    assert_eq!(s.screen.game.map.units_at(Hex::new(5, 20)), [blue]);
    // N goes to red's stacks (and selects none), C to red's cities, M lists
    // red's.
    type_char(&mut s, 'n');
    assert_eq!(s.shown(), REDHOLD);
    assert!(s.screen.game.selection().is_empty());
    type_char(&mut s, 'm');
    assert!(
        s.city_list()
            .is_some_and(|l| l.cities().iter().all(|c| s.screen.game.map.cities().get(*c).unwrap().side == Side::Red))
    );
    // And red's own city's buttons give no orders while blue plays.
    press(&mut s, SpecialKey::Esc);
    click_hex(&mut s, REDHOLD);
    push(&mut s, &format!("{}{}", hexgame::city_windows::PANEL_PREFIX, hexgame::city_windows::build_id(0)));
    assert_eq!(s.screen.game.map.city_at(REDHOLD).unwrap().building, None);
}

/// The owner (2026-11-21): units marked as defending stay put, turn after
/// turn, and Next stack and the most pressing tasks pass them by; until
/// they are sent somewhere, or stood down.
#[test]
fn defenders_stay_put_and_are_passed_by_until_sent_somewhere() {
    let mut game = game("garrison soldiers 2\n");
    let a = at(&game, REDHOLD);
    let kind = game.map.units().kind_named("soldiers").unwrap();
    let b = game.map.new_unit_of(Side::Red, kind).unwrap();
    game.map.place_unit(b, SOUTHWICK).unwrap();
    let c = game.map.new_unit_of(Side::Red, kind).unwrap();
    game.map.place_unit(c, Hex::new(20, 5)).unwrap();
    // The order before can still be taken back: defending changes nothing
    // to take back.
    game.apply(Order::Move { units: vec![b], to: Hex::new(16, 5) }).unwrap();
    game.apply(Order::Defend { units: a.clone(), on: true }).unwrap();
    assert!(game.can_undo());
    assert_eq!(game.next_stack(None, false), Some(Hex::new(20, 5)), "Redhold's defenders are passed by");
    let tasks = game.tasks(Side::Red);
    let defending = Task::Stack { at: REDHOLD, units: 2, state: StackState::Defending };
    assert!(tasks.contains(&defending), "{tasks:?}");
    assert_eq!(defending.tier(), 3, "not among the pressing ones");
    // Next turn, and the one after: still defending, still passed by.
    for _ in 0..2 {
        round(&mut game);
        assert!(a.iter().all(|id| game.map.units().get(*id).unwrap().defending));
        assert_eq!(game.next_stack(None, false), Some(Hex::new(16, 5)));
    }
    // Sent somewhere, one defends no more; undone, it defends again.
    game.apply(Order::Move { units: vec![a[0]], to: Hex::new(6, 5) }).unwrap();
    assert!(!game.map.units().get(a[0]).unwrap().defending);
    game.apply(Order::Undo).unwrap();
    assert!(game.map.units().get(a[0]).unwrap().defending);
    // A standing order ends it; so does standing down.
    game.apply(Order::GoTo { units: vec![a[0]], to: FAR }).unwrap();
    assert!(!game.map.units().get(a[0]).unwrap().defending);
    game.apply(Order::Defend { units: vec![a[1]], on: false }).unwrap();
    assert!(!game.map.units().get(a[1]).unwrap().defending);
    // Not another side's; and the protocol's words.
    let blue = game.map.new_unit_of(Side::Blue, kind).unwrap();
    game.map.place_unit(blue, BLUEMOOR).unwrap();
    assert!(game.apply(Order::Defend { units: vec![blue], on: true }).is_err());
    assert_eq!(
        hexgame::agents::parse_order(&game, "defend red.0.0"),
        Ok(Some(Order::Defend { units: vec![a[0]], on: true }))
    );
    assert_eq!(
        hexgame::agents::parse_order(&game, "undefend red.0.0"),
        Ok(Some(Order::Defend { units: vec![a[0]], on: false }))
    );
}

/// F marks the selected stack as defending and goes on to the next; F on
/// defenders stands them down.
#[test]
fn f_marks_defenders_and_stands_them_down() {
    let mut red = game("garrison soldiers\n");
    let kind = red.map.units().kind_named("soldiers").unwrap();
    let other = red.map.new_unit_of(Side::Red, kind).unwrap();
    red.map.place_unit(other, SOUTHWICK).unwrap();
    let mut s = session(red);
    click_hex(&mut s, REDHOLD);
    let first = s.screen.game.selected().unwrap();
    type_char(&mut s, 'f');
    assert!(s.screen.game.map.units().get(first).unwrap().defending);
    assert_eq!(s.screen.game.selected(), Some(other), "on to the next");
    type_char(&mut s, 't');
    let lines = s.tasks_window().unwrap().lines().to_vec();
    assert!(lines.contains(&"Soldiers in Redhold: defending: stays put".to_string()), "{lines:?}");
    // F on a defender: it stands down, and stays selected.
    click_hex(&mut s, REDHOLD);
    type_char(&mut s, 'f');
    assert!(!s.screen.game.map.units().get(first).unwrap().defending);
    assert_eq!(s.screen.game.selected(), Some(first));
}

/// Selecting a hex leaves out its defenders and those put aside, unless
/// they are all it has: right after F on part of a stack, the stack was
/// selected whole again, and the next click sent the defenders off.
#[test]
fn a_hex_selected_leaves_out_its_defenders() {
    let mut game = game("garrison soldiers 4\n");
    let all = at(&game, REDHOLD);
    game.apply(Order::Defend { units: all[..2].to_vec(), on: true }).unwrap();
    game.select_hex(REDHOLD);
    assert_eq!(game.selection(), &all[2..], "the two that do not defend");
    // With F in the session: the rest are selected next, not the defenders.
    let mut s = session(self::game("garrison soldiers 4\n"));
    click_hex(&mut s, REDHOLD);
    let all = at(&s.screen.game, REDHOLD);
    s.screen.game.select_unit(all[0]);
    type_char(&mut s, 'f');
    assert!(!s.screen.game.selection().contains(&all[0]), "{:?}", s.screen.game.selection());
    // All of them defending: a click takes them all, to be sent somewhere.
    let mut game = self::game("garrison soldiers 2\n");
    let all = at(&game, REDHOLD);
    game.apply(Order::Defend { units: all.clone(), on: true }).unwrap();
    game.select_hex(REDHOLD);
    assert_eq!(game.selection(), &all[..]);
}

/// A move put aside (Done) and then taken back: the units are as they were
/// before the move, not put aside.
#[test]
fn undo_after_done_gives_the_units_back_unput_aside() {
    let mut game = game("garrison soldiers\n");
    let a = at(&game, REDHOLD);
    game.apply(Order::Move { units: a.clone(), to: Hex::new(6, 5) }).unwrap();
    game.apply(Order::Done { units: a.clone() }).unwrap();
    game.apply(Order::Undo).unwrap();
    let unit = game.map.units().get(a[0]).unwrap();
    assert_eq!((unit.pos(), unit.done), (REDHOLD, false));
}
