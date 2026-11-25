//! A city's production queue (the owner, 2026-11-21: "if I take a city I
//! can set it to produce 3 Light Infantry and put them in defend mode for a
//! garrison, then Hawk Riders to rally 0 with a blessing on the way, then
//! cavalry to rally 1, then repeat the last two"): items in turn, each so
//! many of a kind sent somewhere; what it does after the last; a city that
//! builds nothing on purpose is no task that needs orders.

mod common;

use common::{assets, pixel_of, push_widget};
use hexlords::agents::parse_order;
use hexlords::city_windows::{
    BUILD_NOTHING_ID, CITY_DEFEND_ID, QUEUE_FEWER_ID, QUEUE_MORE_TIMES_ID, QUEUE_REPEAT_ID, QUEUE_STOP_ID, build_id,
    point_id, queue_item_id, queue_less_id, queue_more_id, queue_remove_id,
};
use hexlords::game::{CityState, Game, Task};
use hexlords::headless::order_line;
use hexlords::orders::{Event, Order, RallyPoint};
use hexlords::session::Session;
use hexlords::setup::Setup;
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{CityId, Hex, QueueEnd, QueueItem, Sends, Side, UnitId};
use turbo_ferret::Size;

const REDHOLD: Hex = Hex::new(5, 5);
const POINT: Hex = Hex::new(5, 9);

/// 30 x 30 grass: red's Redhold, which builds soldiers (0), riders (1) and
/// scouts (2), a turn each; blue's Bluemoor far to the east. Red's rally
/// point 0 four hexes east of Redhold.
fn game() -> Game {
    let mut rows = vec!["g".repeat(30); 30];
    rows[5] = format!("{}c{}c{}", "g".repeat(5), "g".repeat(19), "g".repeat(4));
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nenter grass 2, city 1\n\
                 unit riders\nname Riders\nstrength 3\nmoves 20\nenter grass 2, city 1\n\
                 unit scouts\nname Scouts\nstrength 1\nmoves 20\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\nproduce soldiers 1\nproduce riders 1\nproduce scouts 1\n\
                  city 5 25\nname Bluemoor\nside blue\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup.gold = 500;
    let mut game = game.with_setup(setup);
    game.apply(Order::RallyPoint { n: 0, point: Some(RallyPoint { at: POINT, defend: false }) }).unwrap();
    game
}

fn redhold(game: &Game) -> CityId {
    game.map.city_at(REDHOLD).unwrap().id
}

fn item(what: usize, count: u32, sends: Sends) -> QueueItem {
    QueueItem { what, count, sends }
}

/// Red's turn ends, blue's too: red's next begins. The units Redhold built.
fn round(game: &mut Game) -> Vec<UnitId> {
    let mut events = game.apply(Order::EndTurn).unwrap();
    events.extend(game.apply(Order::EndTurn).unwrap());
    events.iter().filter_map(|e| if let Event::Built { unit, .. } = e { Some(*unit) } else { None }).collect()
}

fn kind(game: &Game, id: UnitId) -> String {
    game.map.units().get(id).unwrap().name.clone()
}

fn city_task(game: &Game) -> Option<CityState> {
    game.tasks(Side::Red).into_iter().find_map(|t| match t {
        Task::City { state, .. } => Some(state),
        Task::Stack { .. } => None,
    })
}

#[test]
fn a_queue_builds_its_items_in_turn_and_sends_each_where_it_says() {
    let mut game = game();
    let city = redhold(&game);
    let items = vec![item(0, 2, Sends::Defend), item(1, 1, Sends::Point { n: 0, bless: false })];
    game.apply(Order::Queue { city, items, end: QueueEnd::Stop }).unwrap();
    let mut made = Vec::new();
    for _ in 0..3 {
        made.extend(round(&mut game));
    }
    let kinds: Vec<String> = made.iter().map(|id| kind(&game, *id)).collect();
    assert_eq!(kinds, ["Soldiers", "Soldiers", "Riders"]);
    let unit = |id: UnitId| game.map.units().get(id).unwrap();
    assert!(unit(made[0]).defending && unit(made[1]).defending, "a garrison");
    assert_eq!(unit(made[2]).route, [POINT], "to the rally point, at the turn's end");
    round(&mut game);
    let unit = |id: UnitId| game.map.units().get(id).unwrap();
    assert_eq!(unit(made[2]).pos(), POINT);
    // At its end it stops: the city builds nothing, and needs orders.
    let c = game.map.cities().get(city).unwrap();
    assert!(c.queue.is_empty() && c.building.is_none());
    assert_eq!(city_task(&game), Some(CityState::Idle));
    assert!(round(&mut game).is_empty());
}

#[test]
fn a_queue_repeats_from_an_item_so_many_times_or_for_ever() {
    let mut game = game();
    let city = redhold(&game);
    let items = vec![item(0, 1, Sends::Stay), item(1, 1, Sends::Stay), item(2, 1, Sends::Stay)];
    let end = QueueEnd::Repeat { from: 1, times: Some(1) };
    game.apply(Order::Queue { city, items: items.clone(), end }).unwrap();
    let mut made = Vec::new();
    for _ in 0..6 {
        made.extend(round(&mut game));
    }
    let kinds: Vec<String> = made.iter().map(|id| kind(&game, *id)).collect();
    assert_eq!(kinds, ["Soldiers", "Riders", "Scouts", "Riders", "Scouts"], "then it stops");
    // For ever: round and round (to the rally point, so that the city does
    // not fill up).
    let away = Sends::Point { n: 0, bless: false };
    let items: Vec<QueueItem> = items.into_iter().map(|i| QueueItem { sends: away.clone(), ..i }).collect();
    let end = QueueEnd::Repeat { from: 1, times: None };
    game.apply(Order::Queue { city, items, end }).unwrap();
    let mut made = Vec::new();
    for _ in 0..7 {
        made.extend(round(&mut game));
    }
    let kinds: Vec<String> = made.iter().map(|id| kind(&game, *id)).collect();
    assert_eq!(kinds, ["Soldiers", "Riders", "Scouts", "Riders", "Scouts", "Riders", "Scouts"]);
}

#[test]
fn a_queue_changed_goes_on_from_where_it_was() {
    let mut game = game();
    let city = redhold(&game);
    let items = vec![item(0, 3, Sends::Stay), item(1, 1, Sends::Stay)];
    game.apply(Order::Queue { city, items, end: QueueEnd::Stop }).unwrap();
    round(&mut game);
    let queue = |game: &Game| game.map.cities().get(city).unwrap().queue.clone();
    assert_eq!((queue(&game).at, queue(&game).made), (0, 1));
    // The same first item (where they go changed): on from the second of three.
    let items = vec![item(0, 3, Sends::Defend), item(2, 1, Sends::Stay)];
    game.apply(Order::Queue { city, items, end: QueueEnd::Stop }).unwrap();
    assert_eq!((queue(&game).at, queue(&game).made), (0, 1));
    assert_eq!(game.map.cities().get(city).unwrap().sends, Sends::Defend, "the item's");
    // Another first item: from the start.
    let items = vec![item(1, 2, Sends::Stay)];
    game.apply(Order::Queue { city, items, end: QueueEnd::Stop }).unwrap();
    assert_eq!((queue(&game).at, queue(&game).made), (0, 0));
    assert_eq!(game.map.cities().get(city).unwrap().building, Some(1));
}

#[test]
fn nothing_on_purpose_is_no_task_and_build_takes_the_queue_away() {
    let mut game = game();
    let city = redhold(&game);
    assert_eq!(city_task(&game), Some(CityState::Idle), "never told");
    game.apply(Order::Build { city, what: None }).unwrap();
    assert_eq!(city_task(&game), Some(CityState::Resting));
    assert!(game.tasks(Side::Red).iter().all(|t| t.tier() != 1 || matches!(t, Task::Stack { .. })));
    // A queue; then Build: the queue is gone, and Undo gives it back.
    let items = vec![item(0, 1, Sends::Stay), item(1, 1, Sends::Stay)];
    game.apply(Order::Queue { city, items, end: QueueEnd::Stop }).unwrap();
    assert_eq!(city_task(&game), Some(CityState::Building));
    game.apply(Order::Build { city, what: Some(2) }).unwrap();
    assert!(game.map.cities().get(city).unwrap().queue.is_empty());
    game.apply(Order::Undo).unwrap();
    assert_eq!(game.map.cities().get(city).unwrap().queue.items.len(), 2);
}

#[test]
fn a_queue_must_make_sense() {
    let mut game = game();
    let city = redhold(&game);
    let mut refused = |items: Vec<QueueItem>, end: QueueEnd| game.apply(Order::Queue { city, items, end }).is_err();
    assert!(refused(vec![item(0, 0, Sends::Stay)], QueueEnd::Stop), "none of it");
    assert!(refused(vec![item(9, 1, Sends::Stay)], QueueEnd::Stop), "it cannot build that");
    assert!(refused(vec![item(0, 1, Sends::Point { n: 5, bless: false })], QueueEnd::Stop), "no point 5");
    assert!(refused(vec![item(0, 1, Sends::Stay)], QueueEnd::Repeat { from: 1, times: None }), "no item 1");
    assert!(refused(vec![item(0, 1, Sends::Stay); 9], QueueEnd::Stop), "eight at most");
    let bluemoor = game.map.city_at(Hex::new(5, 25)).unwrap().id;
    let theirs = Order::Queue { city: bluemoor, items: vec![item(0, 1, Sends::Stay)], end: QueueEnd::Stop };
    assert!(game.apply(theirs).is_err(), "not red's");
}

#[test]
fn the_text_protocol_sets_a_queue() {
    let game = game();
    let city = redhold(&game);
    let orders = [
        (
            "queue 5 5 0x3@defend 1x1@point0+bless 2x1@point0 repeat 1",
            Order::Queue {
                city,
                items: vec![
                    item(0, 3, Sends::Defend),
                    item(1, 1, Sends::Point { n: 0, bless: true }),
                    item(2, 1, Sends::Point { n: 0, bless: false }),
                ],
                end: QueueEnd::Repeat { from: 1, times: None },
            },
        ),
        (
            "queue 5 5 0x1 1x2@7.7,9.9 repeat 0 3",
            Order::Queue {
                city,
                items: vec![item(0, 1, Sends::Stay), item(1, 2, Sends::Route(vec![Hex::new(7, 7), Hex::new(9, 9)]))],
                end: QueueEnd::Repeat { from: 0, times: Some(3) },
            },
        ),
        ("queue 5 5 none", Order::Queue { city, items: Vec::new(), end: QueueEnd::Stop }),
    ];
    for (line, order) in orders {
        assert_eq!(parse_order(&game, line), Ok(Some(order.clone())), "{line}");
        assert_eq!(order_line(&game, &order), line, "and back");
    }
    assert!(parse_order(&game, "queue 5 5 soldiers").is_err());
}

#[test]
fn a_sides_queues_are_its_own() {
    let mut game = game();
    let city = redhold(&game);
    game.apply(Order::Queue { city, items: vec![item(0, 2, Sends::Defend)], end: QueueEnd::Stop }).unwrap();
    assert!(game.view_for(Side::Blue).map.cities().get(city).unwrap().queue.is_empty());
    assert!(!game.view_for(Side::Red).map.cities().get(city).unwrap().queue.is_empty());
}

// The screen.

fn session() -> Session {
    common::with_game(&assets(), Size::new(1024, 768), 96, game()).expect("the session starts")
}

fn open_city(s: &mut Session) {
    s.screen.view.center_on(REDHOLD);
    let p = pixel_of(s, REDHOLD);
    common::click(s, p);
    common::type_char(s, 'o');
    s.frame(0);
}

/// Click a button of the window, then let a frame lay it out again.
fn press(s: &mut Session, id: &str) {
    push_widget(s, id);
    s.frame(0);
}

fn queue(s: &Session) -> hexlords::state::Queue {
    s.screen.game.map.city_at(REDHOLD).unwrap().queue.clone()
}

#[test]
fn the_city_window_makes_a_queue_item_by_item() {
    let mut s = session();
    open_city(&mut s);
    // The soldiers' box: a queue of one, chosen, that repeats for ever (as
    // building with no queue did); one more of it; they defend.
    press(&mut s, &build_id(0));
    assert_eq!(queue(&s).end, QueueEnd::Repeat { from: 0, times: None });
    press(&mut s, &queue_more_id(0));
    press(&mut s, &queue_more_id(0));
    press(&mut s, CITY_DEFEND_ID);
    assert_eq!(queue(&s).items, [item(0, 3, Sends::Defend)]);
    // Riders to rally point 0 (the new item is the one chosen), a blessing
    // on the way; scouts, which go where the riders go; and one fewer of
    // the soldiers.
    press(&mut s, &build_id(1));
    press(&mut s, &point_id(0));
    press(&mut s, hexlords::city_windows::CITY_BLESS_ID);
    press(&mut s, &build_id(2));
    press(&mut s, &queue_less_id(0));
    let point = Sends::Point { n: 0, bless: true };
    assert_eq!(queue(&s).items, [item(0, 2, Sends::Defend), item(1, 1, point.clone()), item(2, 1, point)]);
    // Repeat from the second: choose it, then Repeat; twice, then for ever.
    press(&mut s, &queue_item_id(1));
    let says = |s: &Session| -> Vec<String> {
        common::descendants(s, hexlords::city_windows::CITY_WINDOW_ID)
            .into_iter()
            .filter_map(|w| s.screen.ui.text(w).map(str::to_string))
            .collect()
    };
    assert!(says(&s).contains(&"Repeat from item 2".to_string()), "the button says which");
    press(&mut s, QUEUE_REPEAT_ID);
    assert_eq!(queue(&s).end, QueueEnd::Repeat { from: 1, times: None });
    // A line over the second item says it repeats from there.
    assert!(says(&s).contains(&"Repeats from here, for ever:".to_string()), "{:?}", says(&s));
    press(&mut s, QUEUE_MORE_TIMES_ID);
    press(&mut s, QUEUE_MORE_TIMES_ID);
    assert_eq!(queue(&s).end, QueueEnd::Repeat { from: 1, times: Some(2) });
    press(&mut s, QUEUE_FEWER_ID);
    press(&mut s, QUEUE_FEWER_ID);
    assert_eq!(queue(&s).end, QueueEnd::Repeat { from: 1, times: None });
    // Out with the last; Stop.
    press(&mut s, &queue_remove_id(2));
    press(&mut s, QUEUE_STOP_ID);
    assert_eq!(queue(&s).items.len(), 2);
    assert_eq!(queue(&s).end, QueueEnd::Stop);
    // Nothing: no queue, and the city rests.
    press(&mut s, BUILD_NOTHING_ID);
    let city = s.screen.game.map.city_at(REDHOLD).unwrap();
    assert!(city.queue.is_empty() && city.resting);
}

#[test]
fn send_to_for_an_item_makes_way_for_the_map_and_comes_back() {
    let mut s = session();
    open_city(&mut s);
    press(&mut s, &build_id(0));
    press(&mut s, &build_id(1));
    press(&mut s, &queue_item_id(0));
    press(&mut s, hexlords::city_windows::CITY_RALLY_ID);
    assert!(s.city_window().is_none(), "out of the way");
    let to = Hex::new(9, 5);
    s.screen.view.center_on(to);
    let p = pixel_of(&s, to);
    common::click(&mut s, p);
    assert_eq!(queue(&s).items[0].sends, Sends::Route(vec![to]), "the first item's");
    assert_eq!(queue(&s).items[1].sends, Sends::Stay);
    assert_eq!(s.city_window().and_then(|w| w.item()), Some(0), "back, the same item chosen");
}
