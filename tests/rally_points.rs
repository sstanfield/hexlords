//! Rally points (the owner, 2026-11-21: "managing all the new units, getting
//! them blessed and rallying them to new or reinforcing old stacks is what I
//! don't love"): a side's ten points, where its cities send their new units
//! and Go to sends stacks; a full point has them stand beside it; a point
//! can have them defend there; a city's units can seek a temple near the way
//! first, or stay in the city and defend it.

mod common;

use hexgame::agents::parse_order;
use hexgame::game::{Game, Task};
use hexgame::headless::order_line;
use hexgame::orders::{Event, Order, RallyPoint};
use hexgame::setup::Setup;
use hexgame::state::scenario::{self, Scenario};
use hexgame::state::{Hex, MAX_STACK, Sends, Side, TileType, UnitId};

const REDHOLD: Hex = Hex::new(5, 5);
/// A rally point five hexes south of Redhold, a temple two past it, and a
/// point five hexes north (the temple is far off its way).
const POINT: Hex = Hex::new(10, 5);
const TEMPLE: Hex = Hex::new(12, 5);
const NORTH: Hex = Hex::new(0, 5);

/// 30 x 30 grass: red's Redhold (building soldiers, a turn each), the
/// temple, blue's Bluemoor far to the east. Soldiers go 6 hexes a turn.
fn game() -> Game {
    let mut rows = vec!["g".repeat(30); 30];
    rows[5] = format!("{}c{}c{}", "g".repeat(5), "g".repeat(19), "g".repeat(4));
    rows[12] = format!("{}t{}", "g".repeat(5), "g".repeat(24));
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nprice 5\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\nproduce soldiers 1\ncity 5 25\nname Bluemoor\nside blue\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    assert_eq!(game.map.tile(TEMPLE).unwrap().kind, TileType::Temple);
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup.gold = 500;
    let mut game = game.with_setup(setup);
    let redhold = game.map.city_at(REDHOLD).unwrap().id;
    game.apply(Order::Build { city: redhold, what: Some(0) }).unwrap();
    game
}

/// New red soldiers on `hex`.
fn soldiers(game: &mut Game, hex: Hex, n: usize) -> Vec<UnitId> {
    let kind = game.map.units().kind_named("soldiers").unwrap();
    (0..n)
        .map(|_| {
            let id = game.map.new_unit_of(Side::Red, kind).unwrap();
            game.map.place_unit(id, hex).unwrap();
            id
        })
        .collect()
}

fn unit(game: &Game, id: UnitId) -> &hexgame::state::Unit {
    game.map.units().get(id).unwrap()
}

fn point(game: &mut Game, n: u8, at: Hex, defend: bool) {
    game.apply(Order::RallyPoint { n, point: Some(RallyPoint { at, defend }) }).unwrap();
}

/// Red's turn ends, blue's (it does nothing) too: red's next begins.
fn round(game: &mut Game) -> Vec<Event> {
    let mut events = game.apply(Order::EndTurn).unwrap();
    events.extend(game.apply(Order::EndTurn).unwrap());
    assert_eq!(game.player(), Side::Red);
    events
}

#[test]
fn units_sent_to_a_full_rally_point_stand_beside_it_a_stack_at_a_time() {
    let mut game = game();
    point(&mut game, 0, POINT, false);
    assert_eq!(game.rally_point_at(Side::Red, POINT), Some(0));
    soldiers(&mut game, POINT, MAX_STACK);
    // Two more, sent to the point: bound to it; it is full, so they stand
    // beside it.
    let two = soldiers(&mut game, REDHOLD, 2);
    game.apply(Order::GoTo { units: two.clone(), to: POINT, via: Vec::new() }).unwrap();
    let beside = unit(&game, two[0]).pos();
    assert_eq!(POINT.distance(beside), 1, "beside it");
    assert!(two.iter().all(|id| unit(&game, *id).pos() == beside), "together");
    assert!(two.iter().all(|id| unit(&game, *id).route.is_empty() && unit(&game, *id).bound.is_none()), "there");
    // Two more go to the same place, while it has room: the stacks fill one
    // at a time.
    let more = soldiers(&mut game, REDHOLD, 2);
    game.apply(Order::GoTo { units: more.clone(), to: POINT, via: Vec::new() }).unwrap();
    assert_eq!(unit(&game, more[0]).pos(), beside);
    assert_eq!(game.map.units_at(beside).len(), 4);
}

#[test]
fn a_rally_point_can_have_them_defend_there() {
    let mut game = game();
    point(&mut game, 1, POINT, true);
    let one = soldiers(&mut game, REDHOLD, 1);
    game.apply(Order::GoTo { units: one.clone(), to: POINT, via: Vec::new() }).unwrap();
    let u = unit(&game, one[0]);
    assert_eq!((u.pos(), u.defending, u.bound), (POINT, true, None));
    // Defenders wait for nothing: they are no task that needs orders.
    round(&mut game);
    let task = game.tasks(Side::Red).into_iter().find(|t| matches!(t, Task::Stack { at, .. } if *at == POINT));
    assert_eq!(task.map(|t| t.tier()), Some(3));
}

#[test]
fn a_moved_rally_point_takes_its_units_along_and_one_taken_away_stops_them() {
    let mut game = game();
    let far = Hex::new(25, 5);
    point(&mut game, 0, far, false);
    let one = soldiers(&mut game, REDHOLD, 1);
    game.apply(Order::GoTo { units: one.clone(), to: far, via: Vec::new() }).unwrap();
    assert_eq!(unit(&game, one[0]).bound, Some(0));
    let redhold = game.map.city_at(REDHOLD).unwrap().id;
    game.apply(Order::Rally { city: redhold, to: Sends::Point { n: 0, bless: false } }).unwrap();
    // Moved: they go to its new place.
    point(&mut game, 0, Hex::new(20, 10), false);
    assert_eq!(unit(&game, one[0]).route, [Hex::new(20, 10)]);
    // Taken away: they stop and wait for orders; the city keeps its units.
    game.apply(Order::RallyPoint { n: 0, point: None }).unwrap();
    let u = unit(&game, one[0]);
    assert!(u.route.is_empty() && u.bound.is_none());
    assert_eq!(game.map.cities().get(redhold).unwrap().sends, Sends::Stay);
    // A city cannot send its units to a point that is not set.
    let refused = game.apply(Order::Rally { city: redhold, to: Sends::Point { n: 0, bless: false } });
    assert_eq!(refused, Err("rally point 0 is not set".to_string()));
}

#[test]
fn a_city_sends_its_new_units_to_a_rally_point_by_a_temple_near_the_way() {
    let mut game = game();
    point(&mut game, 0, POINT, false);
    let redhold = game.map.city_at(REDHOLD).unwrap().id;
    game.apply(Order::Rally { city: redhold, to: Sends::Point { n: 0, bless: true } }).unwrap();
    let events = round(&mut game);
    let built = events.iter().find_map(|e| if let Event::Built { unit, .. } = e { Some(*unit) } else { None });
    let new = built.expect("a unit was built");
    // To the temple first (two hexes past the point: near enough), seven
    // hexes off; blessed there the next turn, and on to the point the turn
    // after.
    assert_eq!(unit(&game, new).route, [TEMPLE, POINT]);
    round(&mut game);
    let u = unit(&game, new);
    assert!(u.blessed_at(TEMPLE), "blessed on the way");
    assert_eq!((u.pos(), u.route.clone(), u.bound), (TEMPLE, vec![POINT], Some(0)));
    round(&mut game);
    assert_eq!(unit(&game, new).pos(), POINT);
}

#[test]
fn a_temple_far_off_the_way_is_passed_by() {
    let mut game = game();
    point(&mut game, 2, NORTH, false);
    let redhold = game.map.city_at(REDHOLD).unwrap().id;
    game.apply(Order::Rally { city: redhold, to: Sends::Point { n: 2, bless: true } }).unwrap();
    let events = round(&mut game);
    let new = events.iter().find_map(|e| if let Event::Built { unit, .. } = e { Some(*unit) } else { None }).unwrap();
    let u = unit(&game, new);
    assert_eq!((u.pos(), u.blessings.len(), u.bound), (NORTH, 0, None), "straight there");
}

#[test]
fn a_city_can_keep_its_new_units_to_defend_it() {
    let mut game = game();
    let redhold = game.map.city_at(REDHOLD).unwrap().id;
    game.apply(Order::Rally { city: redhold, to: Sends::Defend }).unwrap();
    round(&mut game);
    let garrison = game.map.units_at(REDHOLD).to_vec();
    assert_eq!(garrison.len(), 1);
    assert!(unit(&game, garrison[0]).defending);
    assert!(game.waiting_stacks(Side::Red).is_empty(), "nobody need see to it");
}

#[test]
fn the_text_protocol_sets_rally_points_and_sends_units_there() {
    let game = game();
    let redhold = game.map.city_at(REDHOLD).unwrap().id;
    let orders = [
        ("point 3 10 5 defend", Order::RallyPoint { n: 3, point: Some(RallyPoint { at: POINT, defend: true }) }),
        ("point 3 10 5", Order::RallyPoint { n: 3, point: Some(RallyPoint { at: POINT, defend: false }) }),
        ("point 3 none", Order::RallyPoint { n: 3, point: None }),
        ("rally 5 5 point 3 bless", Order::Rally { city: redhold, to: Sends::Point { n: 3, bless: true } }),
        ("rally 5 5 point 3", Order::Rally { city: redhold, to: Sends::Point { n: 3, bless: false } }),
        ("rally 5 5 defend", Order::Rally { city: redhold, to: Sends::Defend }),
        ("rally 5 5 none", Order::Rally { city: redhold, to: Sends::Stay }),
        ("rally 5 5 12 5 10 5", Order::Rally { city: redhold, to: Sends::Route(vec![TEMPLE, POINT]) }),
    ];
    for (line, order) in orders {
        assert_eq!(parse_order(&game, line), Ok(Some(order.clone())), "{line}");
        assert_eq!(order_line(&game, &order), line, "and back");
    }
    assert!(parse_order(&game, "point 10 1 1").is_err(), "0 to 9");
}

#[test]
fn a_sides_rally_points_are_its_own() {
    let mut game = game();
    point(&mut game, 0, POINT, false);
    assert_eq!(game.view_for(Side::Red).rally_point(Side::Red, 0).map(|p| p.at), Some(POINT));
    assert_eq!(game.view_for(Side::Blue).rally_point(Side::Red, 0), None, "blue does not know it");
    // Nor can a point go where the map ends.
    let off = game.apply(Order::RallyPoint { n: 1, point: Some(RallyPoint { at: Hex::new(40, 40), defend: false }) });
    assert!(off.is_err());
}

// The screen.

mod screen {
    use super::*;
    use crate::common::{self, assets, click, pixel_of, push_widget, type_char};
    use hexgame::city_windows::{CITY_BLESS_ID, CITY_RALLY_ID, PANEL_PREFIX, point_id};
    use hexgame::rally_window::{defend_id, place_id, remove_id};
    use hexgame::screen::Scene;
    use hexgame::session::{Pick, Session};
    use turbo_ferret::{Size, SpecialKey};

    fn session(game: Game) -> Session {
        common::with_game(&assets(), Size::new(1024, 768), 96, game).expect("the session starts")
    }

    fn click_hex(s: &mut Session, hex: Hex) {
        s.screen.view.center_on(hex);
        let p = pixel_of(s, hex);
        click(s, p);
    }

    #[test]
    fn the_window_places_a_point_with_a_click_on_the_map_and_its_flag_shows() {
        let mut s = session(game());
        type_char(&mut s, 'r');
        assert_eq!(s.rally_window().map(|w| w.line(0).to_string()), Some("0: not set".to_string()));
        push_widget(&mut s, &place_id(0));
        assert_eq!(s.picking(), Some(Pick::RallyPoint(0)));
        assert!(s.unit_line().starts_with("Rally point 0: click the hex"), "{}", s.unit_line());
        // The window makes way for the map while it picks; a click puts the
        // point there, and the window is back.
        assert!(s.rally_window().is_none());
        click_hex(&mut s, POINT);
        assert_eq!(s.screen.game.rally_point(Side::Red, 0).map(|p| p.at), Some(POINT));
        assert_eq!(s.flags(), [(POINT, 0)]);
        assert_eq!(s.rally_window().map(|w| w.line(0).to_string()), Some("0: row 10, column 5".to_string()));
        // Defend there: its box.
        push_widget(&mut s, &defend_id(0));
        assert_eq!(s.screen.game.rally_point(Side::Red, 0).map(|p| p.defend), Some(true));
        // Removed: no flag.
        push_widget(&mut s, &remove_id(0));
        assert_eq!(s.screen.game.rally_point(Side::Red, 0), None);
        assert!(s.flags().is_empty());
        press_esc(&mut s);
        assert!(s.rally_window().is_none());
    }

    fn press_esc(s: &mut Session) {
        common::press(s, SpecialKey::Esc);
    }

    #[test]
    fn a_city_sends_its_new_units_to_a_point_by_its_button_or_its_flag() {
        let mut game = game();
        point(&mut game, 0, POINT, false);
        point(&mut game, 4, NORTH, false);
        let mut s = session(game);
        click_hex(&mut s, REDHOLD);
        press_esc(&mut s);
        // The sidebar has a button for each point that is set.
        push_widget(&mut s, &format!("{PANEL_PREFIX}{}", point_id(4)));
        let sends = |s: &Session| s.screen.game.map.city_at(REDHOLD).unwrap().sends.clone();
        assert_eq!(sends(&s), Sends::Point { n: 4, bless: false });
        // A blessing on the way: its box (shown now: a frame lays it out).
        s.frame(0);
        push_widget(&mut s, &format!("{PANEL_PREFIX}{CITY_BLESS_ID}"));
        assert_eq!(sends(&s), Sends::Point { n: 4, bless: true });
        assert!(s.city().is_empty() && s.city_panel().is_some());
        // Send to..., then the flag: to that point.
        push_widget(&mut s, &format!("{PANEL_PREFIX}{CITY_RALLY_ID}"));
        click_hex(&mut s, POINT);
        assert_eq!(sends(&s), Sends::Point { n: 0, bless: false });
    }

    #[test]
    fn go_to_onto_a_flag_sends_the_stack_to_the_point() {
        let mut game = game();
        point(&mut game, 0, Hex::new(25, 5), false);
        let one = soldiers(&mut game, REDHOLD, 1);
        let mut s = session(game);
        click_hex(&mut s, REDHOLD);
        type_char(&mut s, 'g');
        click_hex(&mut s, Hex::new(25, 5));
        assert_eq!(unit(&s.screen.game, one[0]).bound, Some(0));
        let at = unit(&s.screen.game, one[0]).pos();
        click_hex(&mut s, at);
        assert!(s.unit_line().contains("on the way to rally point 0 (row 25, column 5)"), "{}", s.unit_line());
    }
}
