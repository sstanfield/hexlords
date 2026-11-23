//! What the review of 2026-11-23 found at the screen, after waypoints in
//! rounds and the tasks' counts: a Tasks row selects the units it counts
//! (not the garrison beside them); nothing is offered after a temple that
//! stops the stack; Go to keeps a plan's waypoints as a click does; the
//! status line counts what the plan leaves; a click on the black sends a
//! planned stack to look; a click on the plan's end leaves the line true;
//! the tasks say where a planned stack goes; the ringed way follows the
//! unit boxes.

mod common;

use common::{assets, click, pixel_of, press, push_widget, type_char, widget};
use hexlords::game::Game;
use hexlords::options_window::Options;
use hexlords::orders::Order;
use hexlords::screen::Scene;
use hexlords::session::{self, END_TURN_ID, MISSION_ASK_ID, Session};
use hexlords::setup::{Fog, Mode, Setup};
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{Hex, Side, TileType, UnitId};
use hexlords::tasks_window::row_id;
use turbo_ferret::{Size, SpecialKey};

/// 30 x 30 grass, Redhold (5, 5) red's, Bluemoor (24, 24) blue's; red a
/// person, blue passive; no chance in battle. Soldiers and levies go six
/// hexes of grass (12 points, 2 a hex).
fn game(mode: Mode, fog: Fog, place: impl FnOnce(&mut Game)) -> Game {
    let mut rows = vec!["g".repeat(30); 30];
    rows[5] = format!("{}c{}", "g".repeat(5), "g".repeat(24));
    rows[24] = format!("{}c{}", "g".repeat(24), "g".repeat(5));
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 5\nmoves 12\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\ncity 24 24\nname Bluemoor\nside blue\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
    place(&mut game);
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    (setup.mode, setup.fog, setup.combat.chance) = (mode, fog, 0);
    game.with_setup(setup)
}

fn put(game: &mut Game, side: Side, kind: &str, hex: Hex) -> UnitId {
    let kind = game.map.units().kind_named(kind).unwrap();
    let id = game.map.new_unit_of(side, kind).unwrap();
    game.map.place_unit(id, hex).unwrap();
    id
}

fn session_of(game: Game) -> Session {
    Session::with_options(&assets(), Size::new(1024, 768), 96, game, Options::manual()).expect("the session starts")
}

fn click_hex(s: &mut Session, hex: Hex) {
    s.screen.view.center_on(hex);
    let p = pixel_of(s, hex);
    click(s, p);
    s.frame(0);
}

fn unit(s: &Session, id: UnitId) -> &hexlords::state::Unit {
    s.screen.game.map.units().get(id).unwrap()
}

fn button(s: &Session) -> String {
    s.screen.ui.text(widget(s, END_TURN_ID)).unwrap_or("").to_string()
}

/// The tasks' rows, the window opened if it is not.
fn tasks(s: &mut Session) -> Vec<String> {
    if s.tasks_window().is_none() {
        s.toggle_tasks();
    }
    s.frame(0);
    s.tasks_window().unwrap().lines().to_vec()
}

/// What a window says, its lines in a row.
fn says(s: &Session, window: &str) -> String {
    let texts = common::descendants(s, window).into_iter().filter_map(|w| s.screen.ui.text(w).map(str::to_string));
    texts.filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" / ")
}

/// Orders done, and the round plays out at the screen (frames 40 ms apart)
/// until the next round's orders are the person's.
fn play_round(s: &mut Session) {
    let round = s.screen.game.turn();
    push_widget(s, END_TURN_ID);
    let mut t = 0;
    while t < 120_000 && !(s.screen.game.turn() > round && button(s).starts_with("Orders done")) {
        t += 40;
        s.frame(t);
        if s.battle().is_some_and(|b| b.done()) {
            push_widget(s, hexlords::battle_window::BATTLE_CLOSE_ID);
        }
    }
    assert!(s.screen.game.turn() > round, "the round played out");
}

const FROM: Hex = Hex::new(10, 10);
/// Three hexes north of `FROM`.
const VIA: Hex = Hex::new(7, 10);

/// A row of the tasks says how many units it is about; its click selects
/// those, not the garrison beside them.
#[test]
fn a_tasks_row_selects_the_units_it_counts() {
    let (mut garrison, mut field) = (Vec::new(), Vec::new());
    let city = Hex::new(5, 5);
    let g = game(Mode::Turns, Fog::Off, |g| {
        for _ in 0..3 {
            garrison.push(put(g, Side::Red, "levy", city));
        }
        for _ in 0..2 {
            field.push(put(g, Side::Red, "soldiers", Hex::new(5, 7)));
        }
    });
    let mut s = session_of(g);
    s.screen.game.apply(Order::Defend { units: garrison.clone(), on: true }).unwrap();
    // The two soldiers come into the city (join its stack: Enter), with
    // moves left; the person looks elsewhere (Esc), and comes back to them
    // by the tasks.
    click_hex(&mut s, Hex::new(5, 7));
    click_hex(&mut s, city);
    press(&mut s, SpecialKey::Enter);
    assert!(field.iter().all(|id| unit(&s, *id).pos() == city && unit(&s, *id).movement > 0));
    press(&mut s, SpecialKey::Esc);
    let rows = tasks(&mut s);
    let row = rows.iter().position(|l| l.contains("in Redhold:")).expect("the stack's row");
    assert_eq!(rows[row], "2 units in Redhold: moved, with moves left");
    push_widget(&mut s, &row_id(row));
    s.frame(0);
    let selected = s.screen.game.selection().to_vec();
    // A click on the map: asked, and Enter sends them.
    click_hex(&mut s, Hex::new(5, 8));
    let asked = if s.asks_mission() { says(&s, MISSION_ASK_ID) } else { String::new() };
    press(&mut s, SpecialKey::Enter);
    let left: Vec<bool> = garrison.iter().map(|id| unit(&s, *id).pos() != city).collect();
    assert_eq!(
        (selected.as_slice(), left.as_slice()),
        (field.as_slice(), [false, false, false].as_slice()),
        "the row said 2 units; selected {} ({}); the garrison left: {left:?}; the window asked: {asked}",
        selected.len(),
        s.unit_line(),
    );
}

/// A temple that has not blessed a stack stops it for the round: the
/// screen does not offer, nor take, a waypoint after it.
#[test]
fn nothing_is_planned_after_a_temple_that_stops_the_stack() {
    let mut ids = Vec::new();
    let (from, temple, on) = (Hex::new(10, 5), Hex::new(10, 8), Hex::new(10, 11));
    let g = game(Mode::Rounds, Fog::Off, |g| {
        g.map.tile_mut(temple).unwrap().kind = TileType::Temple;
        ids.push(put(g, Side::Red, "soldiers", from));
        ids.push(put(g, Side::Red, "soldiers", from));
    });
    let mut s = session_of(g);
    click_hex(&mut s, from);
    click_hex(&mut s, temple);
    let offered = (s.screen.game.onward_from(), Scene::marked(&s, on), s.unit_line().to_string());
    click_hex(&mut s, on);
    let planned = unit(&s, ids[0]).route.clone();
    let said = s.unit_line().to_string();
    play_round(&mut s);
    let ended = unit(&s, ids[0]).pos();
    assert!(
        ended == on || planned == [temple],
        "offered from {:?} (marked: {}; \"{}\"); planned {planned:?}, \"{said}\"; the round left them on {ended:?}",
        offered.0,
        offered.1,
        offered.2,
    );
}

/// Go to, for a stack kept selected to be sent on from its plan's end,
/// goes by the plan's stops as a click beyond its reach does (or the screen
/// says the plan is dropped).
#[test]
fn go_to_after_a_plan_keeps_the_waypoint_as_a_click_does() {
    let far = Hex::new(7, 25);
    let mut routes = Vec::new();
    let mut lines = Vec::new();
    for go_to in [false, true] {
        let mut ids = Vec::new();
        let g = game(Mode::Rounds, Fog::Off, |g| {
            ids.push(put(g, Side::Red, "soldiers", FROM));
            ids.push(put(g, Side::Red, "soldiers", FROM));
        });
        let mut s = session_of(g);
        click_hex(&mut s, FROM);
        click_hex(&mut s, VIA);
        assert_eq!(s.screen.game.onward_from(), Some(VIA));
        if go_to {
            type_char(&mut s, 'g');
            s.frame(0);
            lines.push(s.unit_line().to_string());
        }
        click_hex(&mut s, far);
        routes.push(unit(&s, ids[0]).route.clone());
    }
    assert_eq!(routes[1], routes[0], "(with G, a click; a click alone). The line while picking: {lines:?}");
}

/// The status line says what a hex costs the selected unit of what it
/// has: sent on from its plan's end, of what the plan leaves it.
#[test]
fn the_status_line_counts_the_plan_so_far() {
    let g = game(Mode::Rounds, Fog::Off, |g| {
        put(g, Side::Red, "soldiers", FROM);
    });
    let mut s = session_of(g);
    click_hex(&mut s, FROM);
    click_hex(&mut s, VIA);
    assert_eq!(s.screen.game.onward_from(), Some(VIA), "three hexes planned: 6 of its 12 points");
    press(&mut s, SpecialKey::Right);
    assert_eq!(s.screen.game.map.cursor, Hex::new(7, 11));
    let status = s.status().to_string();
    assert!(
        status.contains("2 of 6 points") || status.contains("8 of 12 points"),
        "one hex on from the plan's end: \"{status}\""
    );
}

/// Total fog, rounds: a click on the black sends the selected stack to
/// look (the help says so), also while it is selected to be sent on.
#[test]
fn a_click_on_the_black_sends_a_stack_with_a_plan_to_look() {
    let mut ids = Vec::new();
    let g = game(Mode::Rounds, Fog::Total, |g| {
        ids.push(put(g, Side::Red, "soldiers", FROM));
    });
    let mut s = session_of(g);
    click_hex(&mut s, FROM);
    click_hex(&mut s, Hex::new(9, 10));
    assert_eq!(s.screen.game.onward_from(), Some(Hex::new(9, 10)));
    let black = Hex::new(10, 20);
    assert!(!s.sight().knows(black));
    click_hex(&mut s, black);
    let route = unit(&s, ids[0]).route.clone();
    assert!(route.last() == Some(&black), "they go to {route:?}; the screen says: {:?}", s.refusal());
}

/// The line under the stack says the truth after a click (or Enter: the
/// cursor is there) on the hex its plan ends on.
#[test]
fn the_line_after_a_click_on_the_plans_end() {
    let g = game(Mode::Rounds, Fog::Off, |g| {
        put(g, Side::Red, "soldiers", FROM);
        put(g, Side::Red, "soldiers", FROM);
    });
    let mut s = session_of(g);
    click_hex(&mut s, FROM);
    click_hex(&mut s, VIA);
    // (The cursor is on the plan's end: Enter is a click there.)
    press(&mut s, SpecialKey::Enter);
    s.frame(0);
    assert_eq!(s.screen.game.onward_from(), Some(VIA), "nothing changed");
    let line = s.unit_line().to_string();
    assert!(line.contains("on from there") && !line.contains("on their way here"), "\"{line}\"; boxes {:?}", s.stack());
}

/// The tasks say where a stack goes: one with a plan that its player
/// then put aside (D) is on its way, and two units of a hex sent two ways
/// are not both on the way to the first one's hex.
#[test]
fn the_tasks_say_where_a_planned_stack_goes() {
    let mut ids = Vec::new();
    let g = game(Mode::Rounds, Fog::Off, |g| {
        ids.push(put(g, Side::Red, "soldiers", FROM));
        ids.push(put(g, Side::Red, "soldiers", FROM));
    });
    let mut s = session_of(g);
    // Each its own way: the first east, the second north.
    let (east, north) = (Hex::new(10, 16), Hex::new(4, 10));
    s.screen.game.apply(Order::Move { units: vec![ids[0]], to: east }).unwrap();
    s.screen.game.apply(Order::Move { units: vec![ids[1]], to: north }).unwrap();
    let two_ways = tasks(&mut s).into_iter().find(|l| l.contains("at row 10, column 10")).unwrap();
    // Both the same way, selected to go on, and D.
    s.screen.game.apply(Order::Move { units: ids.clone(), to: VIA }).unwrap();
    click_hex(&mut s, FROM);
    type_char(&mut s, 'd');
    assert!(ids.iter().all(|id| unit(&s, *id).route == [VIA] && unit(&s, *id).done));
    let put_aside = tasks(&mut s).into_iter().find(|l| l.contains("at row 10, column 10")).unwrap();
    assert!(
        put_aside.contains("on the way to row 7, column 10") && two_ways.contains("row 4, column 10"),
        "after D: \"{put_aside}\"; two ways: \"{two_ways}\""
    );
}

/// The map rings the way of the units selected, not of one taken out of
/// the selection by its box.
#[test]
fn the_ringed_way_follows_the_boxes() {
    let mut ids = Vec::new();
    let g = game(Mode::Rounds, Fog::Off, |g| {
        ids.push(put(g, Side::Red, "soldiers", FROM));
        ids.push(put(g, Side::Red, "soldiers", FROM));
    });
    let mut s = session_of(g);
    let (east, north) = (Hex::new(10, 16), Hex::new(4, 10));
    s.screen.game.apply(Order::Move { units: vec![ids[0]], to: east }).unwrap();
    s.screen.game.apply(Order::Move { units: vec![ids[1]], to: north }).unwrap();
    click_hex(&mut s, FROM);
    assert_eq!(s.screen.game.selection(), ids, "both, each with its plan");
    let ringed = |s: &Session, hex: Hex| Scene::rings(s).iter().any(|(h, _)| *h == hex);
    assert!(ringed(&s, east) && ringed(&s, north));
    // The second is left out: its way is another stack's now.
    push_widget(&mut s, &session::unit_button_id(1));
    s.frame(0);
    assert_eq!(s.screen.game.selection(), [ids[0]]);
    assert!(
        ringed(&s, east) && !ringed(&s, north) && Scene::outlined(&s).contains(&north),
        "ringed as the selection's: east {}, north {}; outlined as another's: north {}",
        ringed(&s, east),
        ringed(&s, north),
        Scene::outlined(&s).contains(&north)
    );
}
