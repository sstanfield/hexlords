//! What the review of the screen found (2026-11-21), each held to what the
//! docs say: the first click of a turn, the sidebar and a queue, a pick
//! when the stack changes, a rally point's flag in reach, End turn's
//! question shut by its own button, a city window that made way for a pick,
//! the item chosen when another is taken out, a city order refused, a
//! battle on the way of a Go to.

mod common;

use common::{assets, click, pixel_of, press, push_widget, shift_click, type_char};
use hexgame::city_windows::{
    CITY_RALLY_ID, PANEL_PREFIX, build_id, point_id, queue_add_id, queue_item_id, queue_remove_id,
};
use hexgame::game::Game;
use hexgame::options_window::Options;
use hexgame::orders::{Order, RallyPoint};
use hexgame::session::{END_ASK_ID, END_TURN_ID, Session};
use hexgame::setup::{Fog, Setup};
use hexgame::state::scenario::{self, Scenario};
use hexgame::state::{Hex, QueueEnd, QueueItem, Sends, Side, TileType, UnitId};
use turbo_ferret::{Size, SpecialKey};

const REDHOLD: Hex = Hex::new(5, 5);
/// Red's two stacks of soldiers (6 hexes a turn), and a lake.
const A: Hex = Hex::new(10, 5);
const B: Hex = Hex::new(16, 5);
const LAKE: Hex = Hex::new(20, 20);

/// 30 x 30 grass with a lake: red's Redhold (soldiers, riders) and stacks
/// on `A` and `B`; blue's Bluemoor far to the east.
fn game(fog: Fog) -> Game {
    let mut rows = vec!["g".repeat(30); 30];
    rows[5] = format!("{}c{}c{}", "g".repeat(5), "g".repeat(19), "g".repeat(4));
    rows[20] = format!("{}w{}", "g".repeat(20), "g".repeat(9));
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nenter grass 2, city 1\n\
                 unit riders\nname Riders\nstrength 3\nmoves 20\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\nproduce soldiers 1\nproduce riders 1\n\
                  city 5 25\nname Bluemoor\nside blue\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
    assert_eq!(game.map.tile(LAKE).unwrap().kind, TileType::Water);
    for hex in [A, B] {
        put(&mut game, Side::Red, "soldiers", hex);
    }
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    (setup.gold, setup.fog) = (500, fog);
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

/// Click a button, then let a frame lay the screen out again.
fn press_widget(s: &mut Session, id: &str) {
    push_widget(s, id);
    s.frame(0);
}

/// A click that starts the person's turn (their log open) is theirs: the
/// screen does not first select a stack for them and give it the click.
#[test]
fn a_click_that_starts_the_turn_is_not_given_to_a_stack_the_screen_selects() {
    let mut game = game(Fog::Off);
    // A march of A's stops short at the end of red's turn: news for red.
    let blocker = put(&mut game, Side::Blue, "levy", Hex::new(10, 12));
    let a = game.map.units_at(A)[0];
    game.apply(Order::GoTo { units: vec![a], to: Hex::new(10, 12), via: Vec::new() }).unwrap();
    let mut s = session(game, Options::default());
    // (None of its moves are left at the end of this turn; at the end of
    // the next, it stops short.)
    assert!(s.next_turn());
    s.end_turn();
    s.wait_for_turn();
    assert!(!s.log_lines().is_empty(), "the log opened");
    assert!(s.screen.game.map.units().get(blocker).is_some());
    let b = s.screen.game.map.units_at(B)[0];
    let a_at = pos(&s, a);
    // A click on the map (its corner, beside the log): the turn starts, and
    // no stack is given an order.
    let map = s.screen.map_rect();
    click(&mut s, turbo_ferret::Point::new(map.x + 20, map.y + map.h - 20));
    assert!(s.log_lines().is_empty(), "the turn started");
    assert_eq!((pos(&s, a), pos(&s, b)), (a_at, B), "nobody moved");
    let routes = |id: UnitId| s.screen.game.map.units().get(id).unwrap().route.clone();
    assert!(routes(b).is_empty(), "nor was sent anywhere");
}

/// The sidebar's buttons are about the item a city builds now: a kind
/// there changes that item, and the queue stays.
#[test]
fn the_sidebar_keeps_a_citys_queue() {
    let mut game = game(Fog::Off);
    let city = game.map.city_at(REDHOLD).unwrap().id;
    let items = vec![
        QueueItem { what: 0, count: 2, sends: Sends::Stay },
        QueueItem { what: 0, count: 1, sends: Sends::Defend },
    ];
    game.apply(Order::Queue { city, items, end: QueueEnd::Repeat { from: 0, times: None } }).unwrap();
    let mut s = session(game, Options::manual());
    click_hex(&mut s, REDHOLD);
    press(&mut s, SpecialKey::Esc);
    s.frame(0);
    press_widget(&mut s, &format!("{PANEL_PREFIX}{}", build_id(1)));
    let queue = s.screen.game.map.city_at(REDHOLD).unwrap().queue.clone();
    assert_eq!(queue.items.len(), 2, "the queue stays");
    assert_eq!(queue.items[0].what, 1, "the item it builds now builds riders");
}

/// A pick under way (Go to's stops) is over when Next stack goes to
/// another stack.
#[test]
fn next_stack_ends_a_pick() {
    let mut s = session(game(Fog::Off), Options::manual());
    click_hex(&mut s, A);
    type_char(&mut s, 'g');
    s.screen.view.center_on(Hex::new(12, 9));
    let p = pixel_of(&s, Hex::new(12, 9));
    shift_click(&mut s, p);
    type_char(&mut s, 'n');
    assert_eq!(s.picking(), None);
    assert!(hexgame::screen::Scene::outlined(&s).is_empty(), "its stops are forgotten");
}

/// A click on a rally point's flag within reach sends the stack there
/// bound to it: it defends there, as the point says.
#[test]
fn a_rally_flag_in_reach_binds_the_stack_to_its_point() {
    let mut game = game(Fog::Off);
    let flag = Hex::new(10, 8);
    game.apply(Order::RallyPoint { n: 0, point: Some(RallyPoint { at: flag, defend: true }) }).unwrap();
    let a = game.map.units_at(A)[0];
    let mut s = session(game, Options::manual());
    click_hex(&mut s, A);
    click_hex(&mut s, flag);
    let unit = s.screen.game.map.units().get(a).unwrap();
    assert_eq!((unit.pos(), unit.defending), (flag, true));
}

/// End turn's question shut by its own close button asks again the next
/// time.
#[test]
fn end_turn_asks_again_after_its_window_was_shut() {
    let mut s = session(game(Fog::Off), Options { ask_end_turn: true, ..Options::manual() });
    push_widget(&mut s, END_TURN_ID);
    assert!(s.asks_end_turn());
    let window = common::widget(&s, END_ASK_ID);
    s.screen.ui.close_window(window);
    s.pump();
    assert!(!s.asks_end_turn());
    push_widget(&mut s, END_TURN_ID);
    assert!(s.asks_end_turn(), "it asks again");
    assert_eq!(s.screen.game.turn(), 1);
}

/// A city's window that made way for a pick, never made, does not come
/// back turns later when another pick is let go.
#[test]
fn a_city_window_that_made_way_does_not_come_back_much_later() {
    let mut s = session(game(Fog::Off), Options::manual());
    click_hex(&mut s, REDHOLD);
    type_char(&mut s, 'o');
    s.frame(0);
    press_widget(&mut s, CITY_RALLY_ID);
    assert!(s.city_window().is_none(), "out of the way");
    assert!(s.next_turn());
    click_hex(&mut s, A);
    type_char(&mut s, 'g');
    press(&mut s, SpecialKey::Esc);
    assert!(s.city_window().is_none());
}

/// The item chosen in a city's window is the same item after another
/// before it is taken out.
#[test]
fn the_item_chosen_stays_chosen_when_one_before_it_goes() {
    let mut s = session(game(Fog::Off), Options::manual());
    click_hex(&mut s, REDHOLD);
    type_char(&mut s, 'o');
    s.frame(0);
    press_widget(&mut s, &queue_add_id(0));
    press_widget(&mut s, &queue_add_id(1));
    press_widget(&mut s, &queue_add_id(0));
    press_widget(&mut s, &queue_item_id(1));
    press_widget(&mut s, &queue_remove_id(0));
    let city = s.screen.game.map.city_at(REDHOLD).unwrap();
    let chosen = s.city_window().and_then(|w| w.item()).unwrap();
    assert_eq!(city.queue.items[chosen].what, 1, "still the riders");
}

/// A city's order the game refuses says why, under the stack.
#[test]
fn a_city_order_refused_says_why() {
    let mut game = game(Fog::Off);
    game.apply(Order::RallyPoint { n: 1, point: Some(RallyPoint { at: LAKE, defend: false }) }).unwrap();
    let mut s = session(game, Options::manual());
    click_hex(&mut s, REDHOLD);
    press(&mut s, SpecialKey::Esc);
    s.frame(0);
    press_widget(&mut s, &format!("{PANEL_PREFIX}{}", point_id(1)));
    assert_eq!(s.screen.game.map.city_at(REDHOLD).unwrap().sends, Sends::Stay);
    assert_eq!(s.unit_line(), "The city cannot do that: nothing Redhold builds can get there.");
}

/// With fog of war, a click beyond reach whose way runs into another
/// side's units nobody saw is a battle there: told, as a move's is.
#[test]
fn a_battle_on_the_way_of_a_go_to_is_told() {
    let mut game = game(Fog::On);
    // Three hexes south of A, out of its sight (two), on its way south.
    put(&mut game, Side::Blue, "levy", Hex::new(13, 5));
    let mut s = session(game, Options::manual());
    click_hex(&mut s, A);
    click_hex(&mut s, Hex::new(28, 5));
    assert!(s.battle().is_some(), "the battle's window");
}
