//! A click on the map with a stack selected, where the person's intent
//! could be two things (the owner, 2026-11-21: "switching stacks is legit
//! but so is combining them ... a popup to ask intent"; "a popup ...
//! explaining the units' current mission and asking for permission to
//! override"; "I don't seem to be able to attack an enemy stack at one of
//! my rally points").

mod common;

use common::{assets, click, pixel_of, press, push_widget};
use hexlords::game::Game;
use hexlords::options_window::Options;
use hexlords::orders::{Event, Order, RallyPoint};
use hexlords::session::{MISSION_ASK_ID, MISSION_GO_ID, MISSION_KEEP_ID, MISSION_TAKE_ID, Session};
use hexlords::setup::Setup;
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{Hex, Side, UnitId};
use turbo_ferret::{Size, SpecialKey};

const REDHOLD: Hex = Hex::new(5, 5);
const POINT: Hex = Hex::new(10, 5);

/// 30 x 30 grass: red's Redhold, blue's Bluemoor far off. Soldiers go 6
/// hexes a turn; levies are weak.
fn game() -> Game {
    let mut rows = vec!["g".repeat(30); 30];
    rows[5] = format!("{}c{}c{}", "g".repeat(5), "g".repeat(19), "g".repeat(4));
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\ncity 5 25\nname Bluemoor\nside blue\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup.combat.chance = 0;
    game.with_setup(setup)
}

fn put(game: &mut Game, side: Side, kind: &str, hex: Hex) -> UnitId {
    let kind = game.map.units().kind_named(kind).unwrap();
    let id = game.map.new_unit_of(side, kind).unwrap();
    game.map.place_unit(id, hex).unwrap();
    id
}

fn session(game: Game, options: Options) -> Session {
    Session::with_options(&assets(), Size::new(1024, 768), 96, game, options).expect("the session starts")
}

fn click_hex(s: &mut Session, hex: Hex) {
    s.screen.view.center_on(hex);
    let p = pixel_of(s, hex);
    click(s, p);
}

fn pos(s: &Session, id: UnitId) -> Hex {
    s.screen.game.map.units().get(id).unwrap().pos()
}

/// What the window that asks says.
fn asks(s: &Session) -> Vec<String> {
    if !s.asks_mission() {
        return Vec::new();
    }
    common::descendants(s, MISSION_ASK_ID).into_iter().filter_map(|w| s.screen.ui.text(w).map(str::to_string)).collect()
}

#[test]
fn a_click_on_another_stack_in_reach_asks_join_or_take_and_enter_joins_a_stack_the_person_chose() {
    let mut game = game();
    let (a, b) = (
        put(&mut game, Side::Red, "soldiers", Hex::new(10, 5)),
        put(&mut game, Side::Red, "soldiers", Hex::new(10, 8)),
    );
    let mut s = session(game, Options::manual());
    click_hex(&mut s, Hex::new(10, 5));
    click_hex(&mut s, Hex::new(10, 8));
    let says = asks(&s);
    assert!(says.iter().any(|t| t == "Join the stack at row 10, column 8, or take that stack?"), "{says:?}");
    assert_eq!(pos(&s, a), Hex::new(10, 5), "nothing yet");
    // The person chose this stack: Enter joins.
    press(&mut s, SpecialKey::Enter);
    assert!(!s.asks_mission());
    assert_eq!((pos(&s, a), pos(&s, b)), (Hex::new(10, 8), Hex::new(10, 8)), "together");
}

#[test]
fn with_the_screens_choice_enter_takes_the_other_stack() {
    let mut game = game();
    let a = put(&mut game, Side::Red, "soldiers", Hex::new(10, 5));
    let b = put(&mut game, Side::Red, "soldiers", Hex::new(10, 8));
    // The screen selects the stack nearest the cursor (Redhold, empty: a).
    let mut s = session(game, Options::default());
    assert_eq!(s.screen.game.selection(), [a]);
    click_hex(&mut s, Hex::new(10, 8));
    assert!(s.asks_mission());
    press(&mut s, SpecialKey::Enter);
    assert_eq!(s.screen.game.selection(), [b], "taken");
    assert_eq!(pos(&s, a), Hex::new(10, 5), "nobody moved");
    // Never mind: nothing.
    click_hex(&mut s, Hex::new(10, 5));
    push_widget(&mut s, MISSION_KEEP_ID);
    assert_eq!(s.screen.game.selection(), [b]);
}

#[test]
fn defenders_at_a_rally_point_say_so_before_new_orders() {
    let mut game = game();
    game.apply(Order::RallyPoint { n: 0, point: Some(RallyPoint { at: POINT, defend: true }) }).unwrap();
    let a = put(&mut game, Side::Red, "soldiers", REDHOLD);
    let b = put(&mut game, Side::Red, "soldiers", REDHOLD);
    game.apply(Order::GoTo { units: vec![a, b], to: POINT, via: Vec::new() }).unwrap();
    let mut s = session(game, Options::manual());
    assert!(s.next_turn());
    click_hex(&mut s, POINT);
    assert_eq!(s.screen.game.selection().len(), 2);
    let to = Hex::new(12, 6);
    click_hex(&mut s, to);
    let says = asks(&s);
    assert!(says.iter().any(|t| t == "These 2 units are defending rally point 0."), "{says:?}");
    // Keep their orders: nothing.
    push_widget(&mut s, MISSION_KEEP_ID);
    assert_eq!(pos(&s, a), POINT);
    // Send them: they go.
    click_hex(&mut s, to);
    push_widget(&mut s, MISSION_GO_ID);
    assert_eq!((pos(&s, a), pos(&s, b)), (to, to));
}

#[test]
fn a_unit_on_its_way_says_where_before_new_orders() {
    let mut game = game();
    let far = Hex::new(25, 5);
    game.apply(Order::RallyPoint { n: 0, point: Some(RallyPoint { at: far, defend: false }) }).unwrap();
    let a = put(&mut game, Side::Red, "soldiers", REDHOLD);
    game.apply(Order::GoTo { units: vec![a], to: far, via: Vec::new() }).unwrap();
    let mut s = session(game, Options::manual());
    assert!(s.next_turn());
    let at = pos(&s, a);
    click_hex(&mut s, at);
    click_hex(&mut s, Hex::new(at.row, at.col + 3));
    let says = asks(&s);
    assert!(says.iter().any(|t| t == "This unit is on the way to rally point 0 (row 25, column 5)."), "{says:?}");
    press(&mut s, SpecialKey::Enter);
    assert_eq!(pos(&s, a), Hex::new(at.row, at.col + 3), "sent");
}

#[test]
fn a_click_on_a_rally_point_with_a_stack_there_asks_send_or_take() {
    let mut game = game();
    game.apply(Order::RallyPoint { n: 0, point: Some(RallyPoint { at: POINT, defend: false }) }).unwrap();
    let rallied = put(&mut game, Side::Red, "soldiers", POINT);
    let a = put(&mut game, Side::Red, "soldiers", Hex::new(12, 9));
    let mut s = session(game, Options::manual());
    click_hex(&mut s, Hex::new(12, 9));
    click_hex(&mut s, POINT);
    let says = asks(&s);
    assert!(says.iter().any(|t| t == "Send it to rally point 0, or take the stack there?"), "{says:?}");
    // Take the stack there: the rally's stack is selected, to move it out.
    push_widget(&mut s, MISSION_TAKE_ID);
    assert_eq!(s.screen.game.selection(), [rallied]);
    assert_eq!(pos(&s, a), Hex::new(12, 9));
}

#[test]
fn an_enemy_stack_on_a_rally_point_is_attacked_by_a_click() {
    let mut game = game();
    game.apply(Order::RallyPoint { n: 0, point: Some(RallyPoint { at: POINT, defend: true }) }).unwrap();
    let a = put(&mut game, Side::Red, "soldiers", Hex::new(12, 5));
    let levy = put(&mut game, Side::Blue, "levy", POINT);
    let mut s = session(game, Options::manual());
    let before = s.screen.game.history().len();
    click_hex(&mut s, Hex::new(12, 5));
    click_hex(&mut s, POINT);
    let fought = s.screen.game.history()[before..].iter().any(|r| matches!(r.event, Event::Battle { .. }));
    assert!(fought, "a battle (it stood beside the point instead)");
    assert!(s.screen.game.map.units().get(levy).is_none(), "the levy fell");
    assert_eq!(pos(&s, a), POINT);
}
