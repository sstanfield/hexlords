//! What the review of the rules found (2026-11-21), each held to what the
//! docs say: Done on a march that stopped short, a queue whose rally point
//! is taken away, Defend and undo, a queue edited, a city taken and taken
//! back, too few points for a step.

use hexgame::game::{CityState, Game, Task};
use hexgame::orders::{Event, Order};
use hexgame::setup::Setup;
use hexgame::state::scenario::{self, Scenario};
use hexgame::state::{Hex, QueueEnd, QueueItem, Sends, Side, UnitId};

const REDHOLD: Hex = Hex::new(5, 5);
const BLUEMOOR: Hex = Hex::new(5, 25);
const WEST: Hex = Hex::new(10, 5);
const MIDDLE: Hex = Hex::new(10, 11);

fn game() -> Game {
    let mut rows = vec!["g".repeat(30); 30];
    rows[5] = format!("{}c{}c{}", "g".repeat(5), "g".repeat(19), "g".repeat(4));
    rows[20] = format!("{}c{}", "g".repeat(25), "g".repeat(4));
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nprice 5\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n\
                 unit knights\nname Knights\nstrength 9\nmoves 12\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\nproduce soldiers 1\n\
                  city 5 25\nname Bluemoor\nside blue\nproduce soldiers 1\nproduce levy 1\n\
                  city 20 25\nname Bluefort\nside blue\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup.combat.chance = 0;
    setup.gold = 500;
    game.with_setup(setup)
}

fn put(game: &mut Game, side: Side, kind: &str, hex: Hex, n: usize) -> Vec<UnitId> {
    let kind = game.map.units().kind_named(kind).unwrap();
    (0..n)
        .map(|_| {
            let id = game.map.new_unit_of(side, kind).unwrap();
            game.map.place_unit(id, hex).unwrap();
            id
        })
        .collect()
}

fn stack_task(game: &Game, at: Hex) -> Option<Task> {
    game.tasks(Side::Red).into_iter().find(|t| matches!(t, Task::Stack { at: a, .. } if *a == at))
}

/// A Go to whose way is shut stalls; Done puts it aside: Next stack and
/// the tasks (and End turn's count) alike.
#[test]
fn done_takes_a_stalled_march_out_of_the_tasks_that_need_orders() {
    let mut game = game();
    let red = put(&mut game, Side::Red, "soldiers", WEST, 2);
    let target = Hex::new(10, 8);
    put(&mut game, Side::Blue, "knights", target, 1);
    let events = game.apply(Order::GoTo { units: red.clone(), to: target, via: Vec::new() }).unwrap();
    assert!(events.iter().any(|e| matches!(e, Event::Stalled { .. })), "{events:?}");
    let at = game.map.units().get(red[0]).unwrap().pos();
    assert_eq!(game.waiting_stacks(Side::Red), [at]);
    // Done.
    game.apply(Order::Done { units: red.clone() }).unwrap();
    assert!(game.waiting_stacks(Side::Red).is_empty(), "Next stack passes it by");
    let task = stack_task(&game, at).unwrap();
    assert_eq!(task.tier(), 3, "the tasks still count it: {task:?}");
}

/// The same for a march that stopped short as a turn ended: D, and End
/// turn no longer counts it.
#[test]
fn done_takes_out_a_march_that_stalled_as_a_turn_ended() {
    let mut game = game();
    let red = put(&mut game, Side::Red, "soldiers", WEST, 1);
    let far = Hex::new(10, 20);
    game.apply(Order::GoTo { units: red.clone(), to: far, via: Vec::new() }).unwrap();
    // Blue knights stand right where it goes next.
    put(&mut game, Side::Blue, "knights", far, 1);
    game.apply(Order::EndTurn).unwrap();
    game.apply(Order::EndTurn).unwrap();
    // Keep ending turns until it stalls.
    for _ in 0..3 {
        if game.map.units().get(red[0]).unwrap().stalled {
            break;
        }
        game.apply(Order::EndTurn).unwrap();
        game.apply(Order::EndTurn).unwrap();
    }
    let unit = game.map.units().get(red[0]).unwrap();
    assert!(unit.stalled, "it stalled");
    let at = unit.pos();
    game.apply(Order::Done { units: red.clone() }).unwrap();
    assert!(game.waiting_stacks(Side::Red).is_empty());
    assert_eq!(stack_task(&game, at).unwrap().tier(), 3, "{:?}", game.tasks(Side::Red));
}

/// A queue item sent to rally point 0; point 0 is taken away: the item's
/// units stay, and the queue can still be changed.
#[test]
fn a_queue_whose_rally_point_is_taken_away_can_still_be_changed() {
    use hexgame::orders::RallyPoint;
    let mut game = game();
    let redhold = game.map.city_at(REDHOLD).unwrap().id;
    game.apply(Order::RallyPoint { n: 0, point: Some(RallyPoint { at: Hex::new(5, 9), defend: false }) }).unwrap();
    let items = vec![
        QueueItem { what: 0, count: 1, sends: Sends::Stay },
        QueueItem { what: 0, count: 2, sends: Sends::Point { n: 0, bless: false } },
    ];
    game.apply(Order::Queue { city: redhold, items, end: QueueEnd::Stop }).unwrap();
    game.apply(Order::RallyPoint { n: 0, point: None }).unwrap();
    let c = game.map.cities().get(redhold).unwrap();
    let mut items = c.queue.items.clone();
    items[0].count = 2;
    let done = game.apply(Order::Queue { city: redhold, items, end: QueueEnd::Stop });
    assert!(done.is_ok(), "{done:?}");
}

/// Red moves a stack (it can be taken back), then puts a patrol to defend
/// (F): that ended a standing order, so Undo takes back nothing (it would
/// have taken back the move, and the patrol stayed lost).
#[test]
fn defend_that_ends_a_patrol_takes_away_the_undo_of_the_order_before() {
    let mut game = game();
    let patrol = put(&mut game, Side::Red, "soldiers", WEST, 1);
    game.apply(Order::Patrol { units: patrol.clone(), waypoints: vec![WEST, MIDDLE], attack: None }).unwrap();
    let other = put(&mut game, Side::Red, "soldiers", Hex::new(20, 5), 1);
    game.apply(Order::Move { units: other.clone(), to: Hex::new(20, 7) }).unwrap();
    game.apply(Order::Defend { units: patrol.clone(), on: true }).unwrap();
    // Defend ended a patrol, which an undo of the move would not give
    // back: the move is no longer to be taken back.
    assert!(!game.can_undo());
    assert!(game.apply(Order::Undo).is_err());
    assert_eq!(game.map.units().get(other[0]).unwrap().pos(), Hex::new(20, 7));
}

/// A stack moved, then sent (Go to) at a stack in the way: its player is
/// done with it this turn, in the tasks as for Next stack.
#[test]
fn a_stack_moved_then_stalled_is_done_with_for_the_turn() {
    let mut game = game();
    let red = put(&mut game, Side::Red, "soldiers", WEST, 1);
    game.apply(Order::Move { units: red.clone(), to: Hex::new(10, 7) }).unwrap();
    put(&mut game, Side::Blue, "knights", Hex::new(10, 9), 1);
    let events = game.apply(Order::GoTo { units: red.clone(), to: Hex::new(10, 9), via: Vec::new() }).unwrap();
    assert!(events.iter().any(|e| matches!(e, Event::Stalled { .. })), "{events:?}");
    let at = Hex::new(10, 7);
    let task = stack_task(&game, at).unwrap();
    assert_eq!(task.tier() == 1, game.waiting_stacks(Side::Red).contains(&at), "they disagree: {task:?}");
}

/// Redhold's queue: soldiers, then levies... it builds the second item; the
/// first (done) is taken out: it goes on with the item it builds.
#[test]
fn removing_a_finished_item_keeps_the_one_being_built() {
    let mut game = game();
    let red = game.map.city_at(REDHOLD).unwrap().id;
    // Redhold builds only soldiers (0); use counts to tell items apart.
    let items = vec![
        QueueItem { what: 0, count: 1, sends: Sends::Stay },
        QueueItem { what: 0, count: 3, sends: Sends::Defend },
    ];
    game.apply(Order::Queue { city: red, items, end: QueueEnd::Stop }).unwrap();
    // Two rounds: the first item's soldier, then one of the second's three.
    for _ in 0..2 {
        game.apply(Order::EndTurn).unwrap();
        game.apply(Order::EndTurn).unwrap();
    }
    let q = game.map.cities().get(red).unwrap().queue.clone();
    assert_eq!((q.at, q.made), (1, 1));
    // The first item out.
    let items = vec![q.items[1].clone()];
    game.apply(Order::Queue { city: red, items, end: QueueEnd::Stop }).unwrap();
    let q = game.map.cities().get(red).unwrap().queue.clone();
    assert_eq!((q.at, q.made), (0, 1), "it goes on with the item it was building, one of three made");
}

mod common;

fn click_hex(s: &mut hexgame::session::Session, hex: Hex) {
    s.screen.view.center_on(hex);
    let p = common::pixel_of(s, hex);
    common::click(s, p);
}

/// P, a click on the last waypoint: the window asks how it fights. A second
/// click on the map (the same hex, as if to confirm) while it asks: no
/// order (nothing is selected while it asks).
#[test]
fn a_click_while_the_patrol_window_asks_gives_no_order() {
    let mut game = game();
    let red = put(&mut game, Side::Red, "soldiers", WEST, 2);
    let mut s = common::with_game(&common::assets(), turbo_ferret::Size::new(1024, 768), 96, game).unwrap();
    click_hex(&mut s, WEST);
    common::type_char(&mut s, 'p');
    let far = Hex::new(10, 25);
    click_hex(&mut s, far);
    assert!(s.patrol_window().is_some(), "it asks");
    // A hex near the map's corner, away from the window.
    let r = s.screen.map_rect();
    let p = turbo_ferret::Point::new(r.x + r.w / 6, r.y + r.h / 2);
    common::click(&mut s, p);
    let u = s.screen.game.map.units().get(red[0]).unwrap();
    assert!(u.route.is_empty(), "sent by Go to while the window asked");
}

/// Red takes Bluemoor (empty) and takes the move back: Bluemoor has its
/// queue back.
#[test]
fn undoing_a_city_taken_gives_back_its_queue() {
    let mut game = game();
    let red = put(&mut game, Side::Red, "soldiers", Hex::new(5, 22), 1);
    game.apply(Order::EndTurn).unwrap();
    assert_eq!(game.player(), Side::Blue);
    let bluemoor = game.map.city_at(BLUEMOOR).unwrap().id;
    let items = vec![
        QueueItem { what: 1, count: 3, sends: Sends::Defend },
        QueueItem { what: 0, count: 1, sends: Sends::Stay },
    ];
    game.apply(Order::Queue { city: bluemoor, items, end: QueueEnd::Repeat { from: 0, times: None } }).unwrap();
    let before = game.map.cities().get(bluemoor).unwrap().clone();
    assert_eq!(before.queue.items.len(), 2);
    game.apply(Order::EndTurn).unwrap();
    assert_eq!(game.player(), Side::Red);
    let events = game.apply(Order::Move { units: red.clone(), to: BLUEMOOR }).unwrap();
    assert!(events.iter().any(|e| matches!(e, Event::Taken { .. })), "{events:?}");
    assert!(game.can_undo());
    game.apply(Order::Undo).unwrap();
    let after = game.map.cities().get(bluemoor).unwrap();
    assert_eq!(after.side, Side::Blue);
    assert_eq!(after.building, before.building);
    assert_eq!(after.queue, before.queue, "the queue is given back");
}

/// The same for a city its holder rests on purpose: after the undo it rests
/// still (it is no city that needs orders).
#[test]
fn undoing_a_city_taken_gives_back_its_rest() {
    let mut game = game();
    let red = put(&mut game, Side::Red, "soldiers", Hex::new(5, 22), 1);
    game.apply(Order::EndTurn).unwrap();
    let bluemoor = game.map.city_at(BLUEMOOR).unwrap().id;
    game.apply(Order::Build { city: bluemoor, what: None }).unwrap();
    assert!(game.map.cities().get(bluemoor).unwrap().resting);
    game.apply(Order::EndTurn).unwrap();
    game.apply(Order::Move { units: red.clone(), to: BLUEMOOR }).unwrap();
    game.apply(Order::Undo).unwrap();
    let after = game.map.cities().get(bluemoor).unwrap();
    let state = game.tasks(Side::Blue).into_iter().find_map(|t| match t {
        Task::City { city, state } if city == bluemoor => Some(state),
        _ => None,
    });
    assert!(after.resting, "resting is given back: {state:?}");
    assert_eq!(state, Some(CityState::Resting));
}

/// A patrol given to a stack with a point left (too few for a step) does
/// not stall: it goes on next time, needing nobody.
#[test]
fn a_patrol_with_too_few_points_for_a_step_does_not_stall() {
    let mut game = game();
    let red = put(&mut game, Side::Red, "soldiers", Hex::new(5, 11), 1);
    // Five hexes of grass (10) and into Redhold (1): 1 point left.
    game.apply(Order::Move { units: red.clone(), to: Hex::new(5, 6) }).unwrap();
    game.apply(Order::Move { units: red.clone(), to: REDHOLD }).unwrap();
    assert_eq!(game.map.units().get(red[0]).unwrap().movement, 1);
    let events = game
        .apply(Order::Patrol { units: red.clone(), waypoints: vec![REDHOLD, Hex::new(5, 11)], attack: None })
        .unwrap();
    assert!(!events.iter().any(|e| matches!(e, Event::Stalled { .. })), "nothing is in the way: {events:?}");
    assert_ne!(stack_task(&game, REDHOLD).unwrap().tier(), 1, "it needs nobody");
}

/// The same for a Go to (a click beyond reach) with a point left.
#[test]
fn a_go_to_with_too_few_points_for_a_step_does_not_stall() {
    let mut game = game();
    let red = put(&mut game, Side::Red, "soldiers", Hex::new(5, 11), 1);
    game.apply(Order::Move { units: red.clone(), to: Hex::new(5, 6) }).unwrap();
    game.apply(Order::Move { units: red.clone(), to: REDHOLD }).unwrap();
    let events = game.apply(Order::GoTo { units: red.clone(), to: Hex::new(5, 15), via: Vec::new() }).unwrap();
    assert!(!events.iter().any(|e| matches!(e, Event::Stalled { .. })), "nothing is in the way: {events:?}");
    assert_ne!(stack_task(&game, REDHOLD).unwrap().tier(), 1, "it needs nobody");
}
