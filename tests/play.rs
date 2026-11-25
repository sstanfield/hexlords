//! What makes the game easy to play: taking back the last order (Undo), Esc
//! to drop the selection, and going from stack to stack and from city to city.
//! The rules (`Order::Undo`, `Game::next_stack`, `Game::next_city`) and the
//! screen's buttons and keys.

mod common;

use common::{assets, choose, click, pixel_of, press, type_char, widget};
use hexlords::game::Game;
use hexlords::orders::{Event, Order};
use hexlords::session::{MENU_CITIES_ID, MENU_UNITS_ID, NEXT_CITY_ID, NEXT_STACK_ID, PREV_STACK_ID, Session, UNDO_ID};
use hexlords::setup::Setup;
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{Hex, Side, UnitId};
use turbo_ferret::{Modifiers, Point, Size, SpecialKey};

const REDHOLD: Hex = Hex::new(5, 5);
const SOUTHWICK: Hex = Hex::new(15, 5);
const BLUEMOOR: Hex = Hex::new(5, 9);
const FARWATER: Hex = Hex::new(15, 15);

/// Red's Redhold with two soldiers in it and Southwick with a levy; blue's
/// Bluemoor with nobody in it, and Farwater with a levy. Red is a person,
/// blue passive, unless `setup` says otherwise.
fn game(setup: impl FnOnce(&mut Setup)) -> Game {
    let mut rows = vec!["g".repeat(20); 20];
    rows[5] = "gggggcgggcgggggggggg".to_string();
    rows[15] = "gggggcgggggggggcgggg".to_string();
    let terrain = format!("20\n20\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\nproduce levy 2\ngarrison soldiers 2\n\
                  city 15 5\nname Southwick\nside red\ngarrison levy\n\
                  city 5 9\nname Bluemoor\nside blue\nproduce levy 3\n\
                  city 15 15\nname Farwater\nside blue\ngarrison levy\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut s = Setup::solo(&game.map, Some(Side::Red), "passive");
    s.combat.chance = 0;
    setup(&mut s);
    game.with_setup(s)
}

fn at(game: &Game, hex: Hex) -> Vec<UnitId> {
    game.map.units_at(hex).to_vec()
}

#[test]
fn undo_takes_back_a_move_and_only_the_last() {
    let mut game = game(|_| {});
    let red = at(&game, REDHOLD);
    assert!(!game.can_undo());
    assert_eq!(game.apply(Order::Undo).unwrap_err(), "there is nothing to undo");
    game.apply(Order::Move { units: vec![red[0]], to: Hex::new(5, 7) }).unwrap();
    assert!(game.can_undo());
    // A refused order changes nothing, and the move can still be taken back.
    assert!(game.apply(Order::Move { units: vec![red[0]], to: Hex::new(19, 19) }).is_err());
    assert_eq!(game.apply(Order::Undo).unwrap(), [Event::Undone]);
    assert_eq!(at(&game, REDHOLD), red, "back where it was, in the stack's order");
    assert_eq!(game.map.units().get(red[0]).unwrap().movement, 12, "with its points");
    assert_eq!(game.selection(), [red[0]], "and selected, to go somewhere else");
    // Only the last order: once.
    assert!(!game.can_undo());
    assert!(game.apply(Order::Undo).is_err());
    // A stack, or toward somewhere far: the same.
    game.apply(Order::Toward { units: red.clone(), to: Hex::new(5, 18) }).unwrap();
    assert!(at(&game, REDHOLD).is_empty());
    game.apply(Order::Undo).unwrap();
    assert_eq!(at(&game, REDHOLD), red);
    // The next order forgets it.
    game.apply(Order::Move { units: vec![red[0]], to: Hex::new(5, 7) }).unwrap();
    game.apply(Order::Move { units: vec![red[1]], to: Hex::new(5, 6) }).unwrap();
    game.apply(Order::Undo).unwrap();
    assert_eq!(game.map.units().get(red[0]).unwrap().pos(), Hex::new(5, 7), "the first stays done");
    assert_eq!(game.map.units().get(red[1]).unwrap().pos(), REDHOLD);
    // The end of the turn cannot be undone, nor what came before it.
    game.apply(Order::Move { units: vec![red[1]], to: Hex::new(5, 6) }).unwrap();
    game.end_turn();
    assert!(!game.can_undo());
    assert!(game.apply(Order::Undo).is_err());
}

#[test]
fn undo_gives_a_city_taken_back_as_it_was() {
    let mut game = game(|_| {});
    let red = at(&game, REDHOLD);
    let bluemoor = game.map.city_at(BLUEMOOR).unwrap().id;
    game.map.set_production(bluemoor, Some(0));
    game.map.city_mut(bluemoor).unwrap().progress = 2;
    let events = game.apply(Order::Move { units: vec![red[0]], to: BLUEMOOR }).unwrap();
    assert!(events.iter().any(|e| matches!(e, Event::Taken { .. })));
    assert_eq!(game.map.city_at(BLUEMOOR).unwrap().side, Side::Red);
    game.apply(Order::Undo).unwrap();
    let city = game.map.city_at(BLUEMOOR).unwrap();
    assert_eq!((city.side, city.building, city.progress), (Side::Blue, Some(0), 2));
    assert!(game.map.units_at(BLUEMOOR).is_empty());
}

#[test]
fn a_battle_cannot_be_undone() {
    let mut game = game(|_| {});
    let red = at(&game, REDHOLD);
    game.apply(Order::Move { units: vec![red[0]], to: Hex::new(5, 7) }).unwrap();
    // Toward Farwater's levy, and then into it.
    let levy = at(&game, FARWATER)[0];
    game.map.move_unit(levy, Hex::new(5, 11)).unwrap();
    let events = game.apply(Order::Move { units: vec![red[1]], to: Hex::new(5, 11) }).unwrap();
    assert!(events.iter().any(|e| matches!(e, Event::Battle { .. })));
    assert!(!game.can_undo(), "chance had a say: no taking it back");
    assert!(game.apply(Order::Undo).is_err());
    assert_eq!(game.map.units().get(red[0]).unwrap().pos(), Hex::new(5, 7), "nor the move before it");
}

#[test]
fn undo_takes_back_what_a_city_was_set_to_build() {
    let mut game = game(|_| {});
    let redhold = game.map.city_at(REDHOLD).unwrap().id;
    game.apply(Order::Build { city: redhold, what: Some(0) }).unwrap();
    assert_eq!(game.map.city_at(REDHOLD).unwrap().building, Some(0));
    game.apply(Order::Undo).unwrap();
    assert_eq!(game.map.city_at(REDHOLD).unwrap().building, None);
}

#[test]
fn the_stacks_that_can_move_and_the_cities_one_after_the_other() {
    let mut game = game(|_| {});
    // In reading order, round and round: Redhold's stack, Southwick's levy.
    assert_eq!(game.next_stack(None, false), Some(REDHOLD));
    assert_eq!(game.next_stack(None, true), Some(SOUTHWICK));
    assert_eq!(game.next_stack(Some(REDHOLD), false), Some(SOUTHWICK));
    assert_eq!(game.next_stack(Some(SOUTHWICK), false), Some(REDHOLD));
    assert_eq!(game.next_stack(Some(REDHOLD), true), Some(SOUTHWICK));
    assert_eq!(game.next_stack(Some(Hex::new(10, 0)), false), Some(SOUTHWICK), "from anywhere");
    // A stack that cannot go anywhere any more is passed over.
    let levy = at(&game, SOUTHWICK)[0];
    game.map.unit_mut(levy).unwrap().movement = 1;
    assert_eq!(game.next_stack(Some(REDHOLD), false), Some(REDHOLD));
    // If none can, any of them.
    for id in at(&game, REDHOLD) {
        game.map.unit_mut(id).unwrap().movement = 0;
    }
    assert_eq!(game.next_stack(Some(REDHOLD), false), Some(SOUTHWICK));
    // The side's cities, in the map's order.
    assert_eq!(game.next_city(Hex::new(0, 0), false), Some(REDHOLD));
    assert_eq!(game.next_city(REDHOLD, false), Some(SOUTHWICK));
    assert_eq!(game.next_city(SOUTHWICK, false), Some(REDHOLD));
    assert_eq!(game.next_city(REDHOLD, true), Some(SOUTHWICK));
    // On blue's turn, blue's.
    game.end_turn();
    assert_eq!(game.next_city(REDHOLD, false), Some(BLUEMOOR));
    assert_eq!(game.next_stack(None, false), Some(FARWATER));
}

// The screen.

fn session(setup: impl FnOnce(&mut Setup)) -> Session {
    common::with_game(&assets(), Size::new(1024, 768), 96, game(setup)).unwrap()
}

fn push(s: &mut Session, name: &str) {
    let r = s.screen.area(widget(s, name));
    click(s, Point::new(r.x + r.w / 2, r.y + r.h / 2));
}

fn disabled(s: &Session, name: &str) -> bool {
    s.screen.ui.state(widget(s, name)).contains(turbo_ferret::WidgetState::DISABLED)
}

fn ctrl(s: &mut Session, ch: char) {
    let mods = if cfg!(target_os = "macos") { Modifiers::SUPER } else { Modifiers::CTRL };
    s.screen.ui.key(Some(ch), SpecialKey::None, mods, true);
    s.screen.ui.key(Some(ch), SpecialKey::None, mods, false);
    s.pump();
}

#[test]
fn esc_drops_the_selection_once_no_window_is_open() {
    let mut s = session(|_| {});
    let p = pixel_of(&s, REDHOLD);
    click(&mut s, p);
    assert_eq!(s.screen.game.selection().len(), 2);
    // The help first: Esc closes it, and the stack stays selected.
    press(&mut s, SpecialKey::F1);
    assert!(s.screen.help_open());
    press(&mut s, SpecialKey::Esc);
    assert!(!s.screen.help_open());
    assert_eq!(s.screen.game.selection().len(), 2);
    // Then the selection.
    press(&mut s, SpecialKey::Esc);
    assert!(s.screen.game.selection().is_empty());
    assert!(!s.screen.game.can_move_to(Hex::new(5, 7)));
}

#[test]
fn a_click_out_of_reach_drops_the_selection_or_takes_the_stack_there() {
    let mut s = session(|_| {});
    let p = pixel_of(&s, REDHOLD);
    click(&mut s, p);
    assert_eq!(s.screen.game.selection(), at(&s.screen.game, REDHOLD));
    // Southwick is too far for Redhold's soldiers: its levy is selected.
    // (Out of view at this zoom: chosen with the cursor and Enter.)
    s.screen.game.set_cursor(SOUTHWICK);
    press(&mut s, SpecialKey::Enter);
    assert_eq!(s.screen.game.selection(), at(&s.screen.game, SOUTHWICK));
    assert_eq!(at(&s.screen.game, REDHOLD).len(), 2, "nothing moved");
    // An empty hex out of reach: nothing is selected.
    s.screen.game.set_cursor(Hex::new(19, 19));
    press(&mut s, SpecialKey::Enter);
    assert!(s.screen.game.selection().is_empty());
}

#[test]
fn undo_on_the_screen_brings_the_stack_back_selected() {
    let mut s = session(|_| {});
    assert!(disabled(&s, UNDO_ID), "nothing to undo yet");
    let red = at(&s.screen.game, REDHOLD);
    let p = pixel_of(&s, REDHOLD);
    click(&mut s, p);
    let p = pixel_of(&s, Hex::new(5, 7));
    click(&mut s, p);
    assert_eq!(at(&s.screen.game, Hex::new(5, 7)), red);
    assert!(!disabled(&s, UNDO_ID));
    push(&mut s, UNDO_ID);
    assert_eq!(at(&s.screen.game, REDHOLD), red);
    assert_eq!(s.screen.game.selection(), red, "selected again");
    assert_eq!(s.shown(), REDHOLD, "and the panels are about them");
    assert!(disabled(&s, UNDO_ID));
    // And by key, Ctrl+Z (Cmd+Z on a Mac).
    let p = pixel_of(&s, Hex::new(5, 7));
    click(&mut s, p);
    ctrl(&mut s, 'z');
    assert_eq!(at(&s.screen.game, REDHOLD), red);
}

#[test]
fn what_an_undo_takes_back_is_no_news_for_the_others() {
    // Two people: red takes Bluemoor, thinks better of it, and blue hears
    // nothing of it.
    let mut s = session(|s| s.set_players("blue=person").unwrap());
    let begin = |s: &mut Session| push(s, hexlords::session::BEGIN_TURN_ID);
    begin(&mut s);
    let p = pixel_of(&s, REDHOLD);
    click(&mut s, p);
    let p = pixel_of(&s, BLUEMOOR);
    click(&mut s, p);
    assert_eq!(s.screen.game.map.city_at(BLUEMOOR).unwrap().side, Side::Red);
    push(&mut s, UNDO_ID);
    assert_eq!(s.screen.game.map.city_at(BLUEMOOR).unwrap().side, Side::Blue);
    s.end_turn();
    begin(&mut s);
    assert!(s.log_lines().is_empty(), "no news, no log: {:?}", s.log_lines());
}

#[test]
fn n_and_c_go_from_stack_to_stack_and_city_to_city() {
    let mut s = session(|_| {});
    let red = at(&s.screen.game, REDHOLD);
    // The button: the first stack that can move, selected and in view.
    push(&mut s, NEXT_STACK_ID);
    assert_eq!(s.screen.game.selection(), red);
    assert_eq!((s.shown(), s.screen.game.map.cursor), (REDHOLD, REDHOLD));
    // N: the next one; Shift+N: back.
    type_char(&mut s, 'n');
    assert_eq!(s.shown(), SOUTHWICK);
    assert_eq!(s.screen.game.selection(), at(&s.screen.game, SOUTHWICK));
    type_char(&mut s, 'N');
    assert_eq!(s.shown(), REDHOLD);
    // The menu does the same.
    choose(&mut s, MENU_UNITS_ID, PREV_STACK_ID);
    assert_eq!(s.shown(), SOUTHWICK);
    choose(&mut s, MENU_UNITS_ID, NEXT_STACK_ID);
    assert_eq!(s.shown(), REDHOLD);
    // Cities: C and the menu; nothing stays selected, so a click looks.
    type_char(&mut s, 'c');
    assert_eq!(s.shown(), SOUTHWICK);
    assert!(s.screen.game.selection().is_empty());
    let southwick = s.screen.game.map.city_at(SOUTHWICK).map(|c| c.id);
    assert_eq!(
        s.city()[0],
        s.screen.game.map.cities().get(southwick.unwrap()).unwrap().name,
        "red's own, in the sidebar"
    );
    choose(&mut s, MENU_CITIES_ID, NEXT_CITY_ID);
    assert_eq!(s.shown(), REDHOLD);
    type_char(&mut s, 'C');
    assert_eq!(s.shown(), SOUTHWICK);
    // The view goes there: the city is in its middle.
    let r = s.screen.map_rect();
    let p = pixel_of(&s, SOUTHWICK);
    assert!((p.x - (r.x + r.w / 2)).abs() < r.w / 4 && (p.y - (r.y + r.h / 2)).abs() < r.h / 4, "{p:?} {r:?}");
}
