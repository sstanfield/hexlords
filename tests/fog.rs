//! What a side sees: with fog of war (set with the game), what is near its
//! units and cities, the rest dimmed, with no units and no holders shown;
//! and, fog or not, of the other sides' stacks and cities only what shows
//! from outside (a banner and how many, a city's walls and garrison), never
//! what is in them or what they build.

mod common;

use common::{assets, choose, click, pixel_of};
use hexgame::agents;
use hexgame::game::{CITY_SIGHT, Game, UNIT_SIGHT, UNSEEN};
use hexgame::info;
use hexgame::orders::{Event, Order};
use hexgame::screen::Scene;
use hexgame::session::{ALL_CITIES_ID, MENU_CITIES_ID, Session};
use hexgame::setup::{Controller, Fog, Setup};
use hexgame::state::scenario::{self, Scenario};
use hexgame::state::{Hex, Side, TileType};
use turbo_ferret::Size;

const REDHOLD: Hex = Hex::new(5, 5);
const BLUEMOOR: Hex = Hex::new(5, 25);
const SCOUT: Hex = Hex::new(15, 15);

/// 30 x 30 grass: red's Redhold with soldiers, blue's Bluemoor with two
/// soldiers, far apart, and a red scout in the middle.
fn game(fog: Fog, red: Controller) -> Game {
    let mut rows = vec!["g".repeat(30); 30];
    rows[5] = format!("{}c{}c{}", "g".repeat(5), "g".repeat(19), "g".repeat(4));
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nprice 5\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\nincome 12\nproduce soldiers 1\ngarrison soldiers\n\
                  city 5 25\nname Bluemoor\nside blue\nincome 9\nproduce soldiers 1\ngarrison soldiers 2\n";
    let (mut map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let kind = map.units().kind_named("soldiers").unwrap();
    let scout = map.new_unit_of(Side::Red, kind).unwrap();
    map.place_unit(scout, SCOUT).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup.set_players(&format!("red={}", if red == Controller::Person { "person" } else { "passive" })).unwrap();
    setup.fog = fog;
    game.with_setup(setup)
}

#[test]
fn a_side_sees_near_its_units_and_cities() {
    assert_eq!((UNIT_SIGHT, CITY_SIGHT), (2, 3));
    let foggy = game(Fog::On, Controller::Person);
    let sight = foggy.sight(Side::Red);
    let far =
        |from: Hex, d: u32| (0..30).flat_map(|r| (0..30).map(move |c| Hex::new(r, c))).find(|h| from.distance(*h) == d);
    // Two hexes from a unit, three from a city.
    assert!(sight.sees(SCOUT) && sight.sees(far(SCOUT, 2).unwrap()));
    assert!(!sight.sees(Hex::new(15, 19)));
    assert!(sight.sees(Hex::new(5, 8)) && !sight.sees(Hex::new(5, 9)));
    assert!(!sight.sees(BLUEMOOR));
    // Blue sees its own, and not red's.
    let blue = foggy.sight(Side::Blue);
    assert!(blue.sees(BLUEMOOR) && !blue.sees(REDHOLD) && !blue.sees(SCOUT));
    // No fog: all of it.
    let clear = game(Fog::Off, Controller::Person);
    assert!(clear.sight(Side::Red).sees(BLUEMOOR));
}

fn session(game: Game) -> Session {
    Session::with_game(&assets(), Size::new(1024, 768), 96, game).expect("the session starts")
}

fn look(s: &mut Session, hex: Hex) {
    s.screen.view.center_on(hex);
    let p = pixel_of(s, hex);
    click(s, p);
}

#[test]
fn in_the_fog_the_panels_tell_no_more_than_is_seen() {
    let mut s = session(game(Fog::On, Controller::Person));
    assert_eq!(Scene::viewer(&s), Some(Side::Red));
    assert!(s.sight().sees(SCOUT) && !s.sight().sees(BLUEMOOR));
    // Blue's city, in the fog: where it is, its walls and taxes; not who
    // holds it, who is in it, what it builds.
    look(&mut s, BLUEMOOR);
    assert_eq!(s.hex_title(), "Row 5, column 25: city (Bluemoor), not seen");
    assert_eq!(s.city(), ["Bluemoor", "Not seen", "Walls 0, taxes 9"]);
    assert!(s.stack().is_empty());
    // Red's own: all of it, in the sidebar.
    look(&mut s, REDHOLD);
    assert_eq!(s.city_panel().map(|p| p.city()), s.screen.game.map.city_at(REDHOLD).map(|c| c.id));
    assert!(s.city().is_empty());
    assert_eq!(s.stack().len(), 1);
}

#[test]
fn the_other_sides_stacks_show_how_many_and_no_more() {
    let mut game = game(Fog::Off, Controller::Person);
    // Blue's two soldiers come out to the field, in sight.
    let blue = game.map.units_at(BLUEMOOR).to_vec();
    for id in &blue {
        game.map.move_unit(*id, Hex::new(15, 17)).unwrap();
    }
    let mut s = session(game);
    look(&mut s, Hex::new(15, 17));
    assert_eq!(s.hex_title(), "Row 15, column 17: grass, 2 units (blue)");
    assert!(s.stack().is_empty(), "no slots for blue's units: what they are is not shown");
    // Its city: from outside, the garrison's size, not what it builds.
    look(&mut s, BLUEMOOR);
    assert_eq!(s.city(), ["Bluemoor", "Held by blue", "Walls 0, taxes 9", "No garrison"]);
}

#[test]
fn with_nobody_at_the_screen_all_is_seen() {
    let s = session(game(Fog::On, Controller::Computer("passive".to_string())));
    assert_eq!(s.seer(), Side::Grey);
    assert_eq!(Scene::viewer(&s), None);
    assert!(s.sight().sees(BLUEMOOR));
}

/// A road through water: red's Redhold at its west end, Bluemoor at its
/// east, red's soldiers in Redhold; blue's in the middle of it, out of
/// sight, if `ambush`.
fn road(ambush: bool) -> Game {
    let mut rows = vec!["w".repeat(30); 30];
    rows[5] = format!("{}c{}c{}", "g".repeat(5), "g".repeat(18), "g".repeat(5));
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 20\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\ngarrison soldiers 2\ncity 5 24\nname Bluemoor\nside blue\n";
    let (mut map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    if ambush {
        let kind = map.units().kind_named("soldiers").unwrap();
        let blue = map.new_unit_of(Side::Blue, kind).unwrap();
        map.place_unit(blue, Hex::new(5, 10)).unwrap();
    }
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup.fog = Fog::On;
    game.with_setup(setup)
}

#[test]
fn an_order_into_the_fog_goes_and_meets_what_it_meets() {
    let mut s = session(road(true));
    look(&mut s, REDHOLD);
    let beyond = Hex::new(5, 12);
    assert!(!s.sight().sees(Hex::new(5, 10)));
    // The reach marked goes on past the unseen soldier, as if nobody were
    // there; and so does the order.
    assert!(s.screen.game.can_move_to(beyond) && Scene::marked(&s, beyond));
    look(&mut s, beyond);
    // It met blue on the way: a battle there, unlooked for; nobody got to
    // (5, 12).
    let surprise = s
        .screen
        .game
        .history()
        .iter()
        .any(|r| matches!(r.event, Event::Surprise { side: Side::Red, at } if at == Hex::new(5, 10)));
    let battle =
        s.screen.game.history().iter().any(|r| matches!(r.event, Event::Battle { at, .. } if at == Hex::new(5, 10)));
    assert!(surprise && battle, "{:?}", s.screen.game.history());
    assert!(s.screen.game.map.units_at(beyond).is_empty());
    // The log says so.
    let log = info::log(&s.screen.game, Side::Red, 0);
    let lines: Vec<&String> = log.iter().flat_map(|(_, entries)| entries).flat_map(|e| &e.lines).collect();
    assert!(lines.iter().any(|l| l.contains("ran into others they had not seen")), "{lines:?}");
}

#[test]
fn what_a_unit_saw_on_its_way_stays_seen_till_the_turn_ends() {
    let mut game = road(false);
    let id = game.map.units_at(REDHOLD)[0];
    assert!(!game.sight(Side::Red).sees(Hex::new(5, 12)));
    game.apply(Order::Move { units: vec![id], to: Hex::new(5, 15) }).unwrap();
    // Back home: what it saw on the way out is still seen.
    game.map.unit_mut(id).unwrap().movement = 20;
    game.apply(Order::Move { units: vec![id], to: REDHOLD }).unwrap();
    let sight = game.sight(Side::Red);
    assert!(sight.sees(Hex::new(5, 12)) && sight.sees(Hex::new(5, 17)), "the way out and two hexes beyond");
    // Its turn over: no longer.
    game.apply(Order::EndTurn).unwrap();
    assert!(!game.sight(Side::Red).sees(Hex::new(5, 12)));
}

#[test]
fn a_stack_seen_is_remembered_where_it_was_seen_last() {
    let mut game = road(false);
    let kind = game.map.units().kind_named("soldiers").unwrap();
    let blue = game.map.new_unit_of(Side::Blue, kind).unwrap();
    game.map.place_unit(blue, Hex::new(5, 7)).unwrap();
    // Red sees it (two hexes from its city); something is done, and it is
    // noted.
    game.apply(Order::EndTurn).unwrap();
    assert!(
        game.sight(Side::Red).sees(Hex::new(5, 7)) && game.ghost_at(Side::Red, Hex::new(5, 7)).is_some(),
        "noted as it is"
    );
    // Blue goes off east, into the fog: red remembers it where it went out
    // of sight.
    game.apply(Order::Move { units: vec![blue], to: Hex::new(5, 16) }).unwrap();
    let ghost = *game.ghost_at(Side::Red, Hex::new(5, 9)).expect("where it went into the fog");
    assert_eq!((ghost.side, ghost.count), (Side::Blue, 1));
    assert!(!game.sight(Side::Red).sees(ghost.at));
    game.apply(Order::EndTurn).unwrap();
    // Red goes to look: the hex in sight, empty, it is forgotten.
    let red = game.map.units_at(REDHOLD)[0];
    game.apply(Order::Move { units: vec![red], to: Hex::new(5, 8) }).unwrap();
    assert!(game.ghost_at(Side::Red, Hex::new(5, 9)).is_none());
    // The view of each side has its own memory only.
    let kind = game.map.units().kind_named("soldiers").unwrap();
    let other = game.map.new_unit_of(Side::Blue, kind).unwrap();
    game.map.place_unit(other, Hex::new(5, 9)).unwrap();
    game.apply(Order::EndTurn).unwrap();
    assert!(!game.ghosts(Side::Blue).is_empty() || !game.ghosts(Side::Red).is_empty());
    assert!(game.view_for(Side::Blue).ghosts(Side::Red).is_empty());
}

#[test]
fn what_was_seen_cannot_be_unseen_by_an_undo() {
    // A move that shows nothing new can be taken back.
    let mut game = road(false);
    let id = game.map.units_at(REDHOLD)[0];
    game.apply(Order::Move { units: vec![id], to: Hex::new(5, 8) }).unwrap();
    assert!(game.can_undo());
    game.apply(Order::Undo).unwrap();
    // One that shows another side's unit cannot.
    let kind = game.map.units().kind_named("soldiers").unwrap();
    let blue = game.map.new_unit_of(Side::Blue, kind).unwrap();
    game.map.place_unit(blue, Hex::new(5, 14)).unwrap();
    game.apply(Order::Move { units: vec![id], to: Hex::new(5, 12) }).unwrap();
    assert!(game.sight(Side::Red).sees(Hex::new(5, 14)));
    assert!(!game.can_undo());
}

#[test]
fn a_march_into_the_fog_meets_what_is_there() {
    // Sent far east with a standing order, the soldiers walk into blue's
    // unseen on the road: a battle, not a stop.
    let mut game = road(true);
    let id = game.map.units_at(REDHOLD)[0];
    let events = game.apply(Order::GoTo { units: vec![id], to: Hex::new(5, 20) }).unwrap();
    assert!(events.iter().any(|e| matches!(e, Event::Surprise { .. })), "{events:?}");
    assert!(!events.iter().any(|e| matches!(e, Event::Stalled { .. })));
}

#[test]
fn the_computer_knows_what_a_person_in_its_place_would() {
    let mut game = game(Fog::On, Controller::Person);
    let bluemoor = game.map.city_at(BLUEMOOR).unwrap().id;
    game.apply(hexgame::orders::Order::EndTurn).unwrap();
    assert_eq!(game.player(), Side::Blue);
    game.apply(hexgame::orders::Order::Build { city: bluemoor, what: Some(0) }).unwrap();
    game.apply(hexgame::orders::Order::EndTurn).unwrap();
    // Red's view: its own all there; blue's soldiers in their city out of
    // sight, and so is who holds it; what blue builds, blue's gold: not told.
    let view = game.view_for(Side::Red);
    assert_eq!(view.map.units().of_side(Side::Red).count(), game.map.units().of_side(Side::Red).count());
    assert_eq!(view.map.units().of_side(Side::Blue).count(), 0);
    let city = view.map.city_at(BLUEMOOR).unwrap();
    assert_eq!((city.side, city.building), (Side::Grey, None));
    assert_eq!(view.gold(Side::Blue), 0);
    assert_eq!(view.gold(Side::Red), game.gold(Side::Red));
    assert!(view.history().is_empty());
    // A blue unit in sight is in it.
    let kind = game.map.units().kind_named("soldiers").unwrap();
    let near = game.map.new_unit_of(Side::Blue, kind).unwrap();
    game.map.place_unit(near, Hex::new(15, 16)).unwrap();
    let view = game.view_for(Side::Red);
    assert_eq!(view.map.units().of_side(Side::Blue).map(|u| u.id()).collect::<Vec<_>>(), [near]);
    // Without fog, every unit is in it; what nobody is told is not, fog or no
    // fog (what the others build, their gold).
    let mut clear = self::game(Fog::Off, Controller::Person);
    clear.apply(hexgame::orders::Order::EndTurn).unwrap();
    let bluemoor = clear.map.city_at(BLUEMOOR).unwrap().id;
    clear.apply(hexgame::orders::Order::Build { city: bluemoor, what: Some(0) }).unwrap();
    let view = clear.view_for(Side::Red);
    assert_eq!(view.map.units().iter().count(), clear.map.units().iter().count());
    let city = view.map.city_at(BLUEMOOR).unwrap();
    assert_eq!((city.side, city.building, city.units.len()), (Side::Blue, None, 0));
    assert_eq!(view.gold(Side::Blue), 0);
}

// What is remembered of a city.

#[test]
fn a_city_is_remembered_as_it_was_seen_last() {
    let mut game = game(Fog::On, Controller::Person);
    let scout = game.map.units_at(SCOUT)[0];
    let redhold = game.map.city_at(REDHOLD).unwrap().id;
    let bluemoor = game.map.city_at(BLUEMOOR).unwrap().id;
    // The scout looks at Bluemoor: blue's.
    game.map.move_unit(scout, Hex::new(5, 23)).unwrap();
    game.apply(Order::Build { city: redhold, what: None }).unwrap();
    assert_eq!(game.sight(Side::Red).held(BLUEMOOR), Some((Side::Blue, 1)));
    // It goes; green takes Bluemoor out of red's sight.
    game.map.move_unit(scout, SCOUT).unwrap();
    game.map.city_mut(bluemoor).unwrap().side = Side::Green;
    game.apply(Order::Build { city: redhold, what: None }).unwrap();
    let sight = game.sight(Side::Red);
    let city = game.map.cities().get(bluemoor).unwrap();
    assert!(!sight.sees(BLUEMOOR));
    assert_eq!(sight.holder(city), Side::Blue, "as it was seen");
    let known = info::Known::of(city, Side::Red, &sight);
    assert_eq!(known, info::Known::Fog(Some((Side::Blue, 1))));
    assert_eq!(info::city_lines_for(&game.map, city, 10, known)[1..3], ["Not seen since round 1", "Held by blue then"]);
    assert_eq!(game.view_for(Side::Red).map.city_at(BLUEMOOR).map(|c| c.side), Some(Side::Blue));
    // Red's own city taken out of its sight: it is told who took it.
    for id in game.map.units_at(REDHOLD).to_vec() {
        game.map.kill_unit(id);
    }
    game.map.city_mut(redhold).unwrap().side = Side::Green;
    game.apply(Order::Move { units: vec![scout], to: Hex::new(15, 16) }).unwrap();
    let sight = game.sight(Side::Red);
    assert!(!sight.sees(REDHOLD));
    assert_eq!(sight.holder(game.map.cities().get(redhold).unwrap()), Side::Green);
}

// Total fog of war: land never seen is black.

#[test]
fn land_never_seen_is_not_known_and_once_seen_stays_known() {
    let mut game = game(Fog::Total, Controller::Person);
    let sight = game.sight(Side::Red);
    assert!(sight.knows(REDHOLD) && sight.knows(Hex::new(5, 8)) && sight.knows(SCOUT));
    assert!(!sight.knows(BLUEMOOR) && !sight.knows(Hex::new(15, 18)));
    assert!(sight.frontier(&game.map).contains(&Hex::new(15, 17)));
    // Fog of war of the other kind knows the whole map.
    assert!(game_knows_all(Fog::On));
    // The scout goes east and back: what it saw stays known, dimmed.
    let scout = game.map.units_at(SCOUT)[0];
    game.apply(Order::Move { units: vec![scout], to: Hex::new(15, 17) }).unwrap();
    game.apply(Order::Move { units: vec![scout], to: SCOUT }).unwrap();
    game.apply(Order::EndTurn).unwrap();
    while game.player() != Side::Red {
        game.apply(Order::EndTurn).unwrap();
    }
    let sight = game.sight(Side::Red);
    assert!(sight.knows(Hex::new(15, 19)) && !sight.sees(Hex::new(15, 19)));
    assert!(!sight.knows(Hex::new(15, 20)));
}

fn game_knows_all(fog: Fog) -> bool {
    let game = game(fog, Controller::Person);
    let sight = game.sight(Side::Red);
    sight.knows(BLUEMOOR) && sight.known_count().is_none() && sight.frontier(&game.map).is_empty()
}

#[test]
fn no_order_goes_into_land_never_seen_nor_through_it() {
    let mut game = game(Fog::Total, Controller::Person);
    let scout = game.map.units_at(SCOUT)[0];
    // Three hexes east: in reach over the grass, but never seen.
    let beyond = Hex::new(15, 18);
    assert!(!game.sight(Side::Red).knows(beyond));
    assert_eq!(game.apply(Order::Move { units: vec![scout], to: beyond }), Err(UNSEEN.to_string()));
    assert_eq!(game.apply(Order::Toward { units: vec![scout], to: BLUEMOOR }), Err(UNSEEN.to_string()));
    assert_eq!(game.apply(Order::GoTo { units: vec![scout], to: BLUEMOOR }), Err(UNSEEN.to_string()));
    let redhold = game.map.city_at(REDHOLD).unwrap().id;
    assert_eq!(game.apply(Order::Rally { city: redhold, to: Some(beyond) }), Err(UNSEEN.to_string()));
    // Known land the way to which is not known: no way there.
    let why = game.apply(Order::GoTo { units: vec![scout], to: Hex::new(5, 7) }).unwrap_err();
    assert!(why.contains("cannot get there"), "{why}");
    // Selected, it can go to the edge of what is known, and no farther.
    assert!(game.select_unit(scout));
    assert!(game.can_move_to(Hex::new(15, 17)) && !game.can_move_to(beyond));
}

#[test]
fn a_move_that_showed_land_never_seen_cannot_be_undone() {
    for (fog, undone) in [(Fog::On, true), (Fog::Total, false)] {
        let mut game = game(fog, Controller::Person);
        let scout = game.map.units_at(SCOUT)[0];
        // A step east shows land three hexes east of where it stood.
        game.apply(Order::Move { units: vec![scout], to: Hex::new(15, 16) }).unwrap();
        assert_eq!(game.can_undo(), undone, "{fog:?}");
        // The step back shows nothing new.
        game.apply(Order::Move { units: vec![scout], to: SCOUT }).unwrap();
        assert!(game.can_undo(), "{fog:?}");
    }
}

#[test]
fn the_computer_is_told_nothing_of_land_never_seen() {
    let game = game(Fog::Total, Controller::Person);
    let view = game.view_for(Side::Red);
    let bluemoor = view.map.city_at(BLUEMOOR).unwrap();
    assert_eq!((bluemoor.side, bluemoor.name.as_str(), bluemoor.income, bluemoor.walls), (Side::Grey, "", 0, 0));
    assert_eq!(view.map.tile(BLUEMOOR).map(|t| t.kind), Some(TileType::Grass));
    assert!(!view.sight(Side::Red).knows(BLUEMOOR) && view.sight(Side::Red).knows(REDHOLD));
    let text = agents::describe(&view, Side::Red);
    assert!(text.lines().any(|l| l == "fog total"), "{text}");
    assert!(!text.contains("Bluemoor") && text.contains("name Redhold"), "{text}");
    let bluemoor_row = text.lines().nth(6 + usize::from(BLUEMOOR.row)).unwrap();
    assert_eq!(bluemoor_row.chars().nth(usize::from(BLUEMOOR.col)), Some('?'));
    assert_eq!(bluemoor_row.chars().nth(5), Some('g'));
}

#[test]
fn on_the_screen_land_never_seen_says_nothing() {
    let mut s = session(game(Fog::Total, Controller::Person));
    look(&mut s, BLUEMOOR);
    assert_eq!(s.hex_title(), "Row 5, column 25: never seen");
    assert!(s.city().is_empty(), "no city panel: it is not known to be there");
    choose(&mut s, MENU_CITIES_ID, ALL_CITIES_ID);
    assert_eq!(s.city_list().map(|l| l.rows().len()), Some(1), "Redhold only");
}
