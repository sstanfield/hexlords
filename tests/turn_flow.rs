//! How a turn goes at the screen, for the person: Next stack goes to the
//! nearest stack that waits; the options (the Game menu's Options) select
//! the next stack by themselves, as a turn starts and after a move; End
//! turn says what waits, and asks first; a battle can be told in a line,
//! and a temple can bless without asking.

mod common;

use common::{assets, choose, click, pixel_of, push_widget, type_char};
use hexlords::game::Game;
use hexlords::options_window::{self, Options};
use hexlords::orders::Order;
use hexlords::session::{
    END_ANYWAY_ID, END_KEEP_ID, END_TASKS_ID, END_TURN_ID, MENU_GAME_ID, OPTIONS_ID, Session, UNIT_LINE_ID,
};
use hexlords::setup::Setup;
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{Hex, Side, UnitId};
use turbo_ferret::{Modifiers, Size, SpecialKey};

/// Red's Redhold, with soldiers in it.
const REDHOLD: Hex = Hex::new(5, 5);
/// Red's other stacks: one far along the same row, one a little south.
const EAST: Hex = Hex::new(5, 25);
const SOUTH: Hex = Hex::new(10, 6);
/// A temple, and blue's lone levy, near Redhold.
const TEMPLE: Hex = Hex::new(8, 3);
const LEVY: Hex = Hex::new(3, 8);

/// 30 x 30 grass: red's Redhold and soldiers on `EAST` and `SOUTH`; a
/// temple; blue's levy near Redhold, and Bluemoor in the far corner.
/// Soldiers go 6 hexes a turn, and cost 4 gold a turn.
fn game() -> Game {
    let mut rows = vec!["g".repeat(30); 30];
    rows[5] = format!("{}c{}", "g".repeat(5), "g".repeat(24));
    rows[8] = format!("{}t{}", "g".repeat(3), "g".repeat(26));
    rows[28] = format!("{}c{}", "g".repeat(28), "g");
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nprice 5\nupkeep 4\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\nproduce soldiers 1\ngarrison soldiers\n\
                  city 28 28\nname Bluemoor\nside blue\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let kind = game.map.units().kind_named("soldiers").unwrap();
    for hex in [EAST, SOUTH] {
        let id = game.map.new_unit_of(Side::Red, kind).unwrap();
        game.map.place_unit(id, hex).unwrap();
    }
    let levy = game.map.units().kind_named("levy").unwrap();
    let id = game.map.new_unit_of(Side::Blue, levy).unwrap();
    game.map.place_unit(id, LEVY).unwrap();
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup.gold = 100;
    game.with_setup(setup)
}

fn text(s: &Session, id: &str) -> String {
    s.screen.ui.text(common::widget(s, id)).unwrap_or_default().to_string()
}

fn session(options: Options) -> Session {
    Session::with_options(&assets(), Size::new(1024, 768), 96, game(), options).expect("the session starts")
}

fn selected_at(s: &Session) -> Option<Hex> {
    let game = &s.screen.game;
    game.selected().and_then(|id| game.map.units().get(id)).map(|u| u.pos())
}

fn click_hex(s: &mut Session, hex: Hex) {
    s.screen.view.center_on(hex);
    let p = pixel_of(s, hex);
    click(s, p);
}

fn at(s: &Session, hex: Hex) -> Vec<UnitId> {
    s.screen.game.map.units_at(hex).to_vec()
}

#[test]
fn next_goes_to_the_nearest_stack_that_waits_and_round_them_all() {
    let mut s = session(Options::manual());
    click_hex(&mut s, REDHOLD);
    assert_eq!(selected_at(&s), Some(REDHOLD));
    // The nearest first (in reading order, the east one would come first).
    type_char(&mut s, 'n');
    assert_eq!(selected_at(&s), Some(SOUTH));
    // Then the nearest it has not been to: not back to Redhold.
    type_char(&mut s, 'n');
    assert_eq!(selected_at(&s), Some(EAST));
    // Round them all: again from the nearest.
    type_char(&mut s, 'n');
    assert!(selected_at(&s).is_some_and(|h| h != EAST));
    // Shift+N: back the way it came.
    type_char(&mut s, 'N');
    assert_eq!(selected_at(&s), Some(EAST));
    type_char(&mut s, 'N');
    assert_eq!(selected_at(&s), Some(SOUTH));
}

#[test]
fn the_screen_selects_the_next_stack_as_a_turn_starts_and_after_a_move() {
    let mut s = session(Options { next_after_move: true, ..Options::default() });
    // The turn starts with the stack nearest the cursor selected (it starts
    // on Redhold).
    assert_eq!(selected_at(&s), Some(REDHOLD));
    // A move that leaves the stack moves: it stays selected.
    let soldiers = at(&s, REDHOLD);
    click_hex(&mut s, Hex::new(7, 5));
    assert_eq!(s.screen.game.map.units().get(soldiers[0]).map(|u| u.pos()), Some(Hex::new(7, 5)));
    assert_eq!(selected_at(&s), Some(Hex::new(7, 5)));
    // A move that spends the rest: on to the nearest stack that waits.
    click_hex(&mut s, Hex::new(11, 5));
    assert_eq!(s.screen.game.map.units().get(soldiers[0]).map(|u| u.movement), Some(0));
    assert_eq!(selected_at(&s), Some(SOUTH));
    // Done: the last one.
    type_char(&mut s, 'd');
    assert_eq!(selected_at(&s), Some(EAST));
    type_char(&mut s, 'd');
    assert_eq!(selected_at(&s), None, "none waits");
    // The next turn starts with one selected again.
    assert!(s.next_turn());
    assert!(selected_at(&s).is_some());
}

#[test]
fn with_the_option_off_nothing_is_selected_by_itself() {
    let mut s = session(Options::manual());
    assert_eq!(selected_at(&s), None);
    click_hex(&mut s, REDHOLD);
    click_hex(&mut s, Hex::new(7, 5));
    // (The stack moved stays selected while it can go on, either way.)
    assert_eq!(selected_at(&s), Some(Hex::new(7, 5)), "its own, kept");
    click_hex(&mut s, Hex::new(11, 5));
    assert_eq!(selected_at(&s), None, "spent: no other stack is selected");
}

#[test]
fn the_options_window_ticks_and_unticks_them() {
    let mut s = session(Options::default());
    choose(&mut s, MENU_GAME_ID, OPTIONS_ID);
    assert!(s.options_window().is_some());
    // Off at first (the owner's word, 2026-11-21: "I like to look around
    // before moving on").
    assert!(!s.options().next_after_move);
    // A click on a box (or its words) turns it on, and off again.
    push_widget(&mut s, options_window::NEXT_AFTER_MOVE_ID);
    assert!(s.options().next_after_move);
    push_widget(&mut s, options_window::QUICK_BATTLES_ID);
    assert!(s.options().quick_battles);
    push_widget(&mut s, options_window::NEXT_AFTER_MOVE_ID);
    assert!(!s.options().next_after_move);
    // Esc closes it; Cmd+, (Ctrl+,) opens it, and closes it.
    common::press(&mut s, SpecialKey::Esc);
    assert!(s.options_window().is_none());
    let mods = if cfg!(target_os = "macos") { Modifiers::SUPER } else { Modifiers::CTRL };
    s.screen.ui.key(Some(','), SpecialKey::None, mods, true);
    s.screen.ui.key(Some(','), SpecialKey::None, mods, false);
    s.pump();
    assert!(s.options_window().is_some());
    s.screen.ui.key(Some(','), SpecialKey::None, mods, true);
    s.screen.ui.key(Some(','), SpecialKey::None, mods, false);
    s.pump();
    assert!(s.options_window().is_none());
}

#[test]
fn end_turn_says_how_many_wait_and_asks_first() {
    let mut s = session(Options { next_after_move: true, ..Options::default() });
    let redhold = s.screen.game.map.city_at(REDHOLD).unwrap().id;
    s.screen.game.apply(Order::Build { city: redhold, what: Some(0) }).unwrap();
    s.pump();
    // Three stacks wait for orders.
    assert_eq!(text(&s, END_TURN_ID), "End turn (3)");
    push_widget(&mut s, END_TURN_ID);
    assert!(s.asks_end_turn());
    assert_eq!(s.screen.game.turn(), 1, "not yet");
    // Keep playing: nothing ends. Show the tasks: they open.
    push_widget(&mut s, END_KEEP_ID);
    assert!(!s.asks_end_turn());
    push_widget(&mut s, END_TURN_ID);
    push_widget(&mut s, END_TASKS_ID);
    assert!(!s.asks_end_turn() && s.tasks_window().is_some());
    // End the turn: it ends.
    push_widget(&mut s, END_TURN_ID);
    push_widget(&mut s, END_ANYWAY_ID);
    s.wait_for_turn();
    assert_eq!((s.screen.game.player(), s.screen.game.turn()), (Side::Red, 2));
    // With all of them seen to, it ends at once.
    for _ in 0..3 {
        type_char(&mut s, 'd');
    }
    assert_eq!(text(&s, END_TURN_ID), "End red's turn");
    push_widget(&mut s, END_TURN_ID);
    assert!(!s.asks_end_turn());
    s.wait_for_turn();
    assert_eq!(s.screen.game.turn(), 3);
}

#[test]
fn end_turn_warns_when_the_gold_will_not_pay_the_upkeep() {
    let mut s = session(Options { next_after_move: true, ..Options::default() });
    let redhold = s.screen.game.map.city_at(REDHOLD).unwrap().id;
    s.screen.game.apply(Order::Build { city: redhold, what: Some(0) }).unwrap();
    for _ in 0..3 {
        type_char(&mut s, 'd');
    }
    // 100 gold and Redhold's taxes pay 3 soldiers' upkeep for a while: no
    // question.
    push_widget(&mut s, END_TURN_ID);
    assert!(!s.asks_end_turn());
    s.wait_for_turn();
    // Many more soldiers than that: it says units will leave.
    let kind = s.screen.game.map.units().kind_named("soldiers").unwrap();
    for i in 0..30 {
        let id = s.screen.game.map.new_unit_of(Side::Red, kind).unwrap();
        s.screen.game.map.place_unit(id, Hex::new(20 + i / 6, 10 + i % 6)).unwrap();
        s.screen.game.apply(Order::Done { units: vec![id] }).unwrap();
    }
    for _ in 0..3 {
        type_char(&mut s, 'd');
    }
    push_widget(&mut s, END_TURN_ID);
    assert!(s.asks_end_turn());
    let lines = common::descendants(&s, hexlords::session::END_ASK_ID)
        .into_iter()
        .filter_map(|w| s.screen.ui.text(w).map(str::to_string))
        .collect::<Vec<_>>();
    assert!(lines.iter().any(|l| l.contains("will not pay the upkeep")), "{lines:?}");
}

#[test]
fn a_battle_can_be_told_in_a_line() {
    let mut s = session(Options { quick_battles: true, ..Options::manual() });
    click_hex(&mut s, REDHOLD);
    click_hex(&mut s, LEVY);
    assert!(s.battle().is_none(), "no window");
    let line = text(&s, UNIT_LINE_ID);
    assert!(line.starts_with("You attacked blue"), "{line}");
    // Until the next click or key.
    type_char(&mut s, 'n');
    assert!(!text(&s, UNIT_LINE_ID).starts_with("You attacked"));
    // Played out, as it was.
    let mut s = session(Options::manual());
    click_hex(&mut s, REDHOLD);
    click_hex(&mut s, LEVY);
    assert!(s.battle().is_some());
}

#[test]
fn a_temple_can_bless_without_asking() {
    let mut s = session(Options { always_bless: true, ..Options::manual() });
    let soldiers = at(&s, REDHOLD);
    click_hex(&mut s, REDHOLD);
    click_hex(&mut s, TEMPLE);
    assert!(s.temple().is_none(), "no question");
    let unit = s.screen.game.map.units().get(soldiers[0]).unwrap();
    assert!(unit.blessed_at(TEMPLE));
    let line = text(&s, UNIT_LINE_ID);
    assert!(line.starts_with("The temple blessed them: Soldiers +"), "{line}");
    // Asked, as it was.
    let mut s = session(Options::manual());
    click_hex(&mut s, REDHOLD);
    click_hex(&mut s, TEMPLE);
    assert!(s.temple().is_some());
}
