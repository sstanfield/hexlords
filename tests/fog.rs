//! What a side sees: with fog of war (set with the game), what is near its
//! units and cities, the rest dimmed, with no units and no holders shown;
//! and, fog or not, of the other sides' stacks and cities only what shows
//! from outside (a banner and how many, a city's walls and garrison), never
//! what is in them or what they build.

mod common;

use common::{assets, click, pixel_of};
use hexgame::game::{CITY_SIGHT, Game, UNIT_SIGHT};
use hexgame::screen::Scene;
use hexgame::session::Session;
use hexgame::setup::{Controller, Setup};
use hexgame::state::scenario::{self, Scenario};
use hexgame::state::{Hex, Side};
use turbo_ferret::Size;

const REDHOLD: Hex = Hex::new(5, 5);
const BLUEMOOR: Hex = Hex::new(5, 25);
const SCOUT: Hex = Hex::new(15, 15);

/// 30 x 30 grass: red's Redhold with soldiers, blue's Bluemoor with two
/// soldiers, far apart, and a red scout in the middle.
fn game(fog: bool, red: Controller) -> Game {
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
    let foggy = game(true, Controller::Person);
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
    let clear = game(false, Controller::Person);
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
    let mut s = session(game(true, Controller::Person));
    assert_eq!(Scene::viewer(&s), Some(Side::Red));
    assert!(s.sees(SCOUT) && !s.sees(BLUEMOOR));
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
    let mut game = game(false, Controller::Person);
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
    let s = session(game(true, Controller::Computer("passive".to_string())));
    assert_eq!(s.seer(), Side::Grey);
    assert_eq!(Scene::viewer(&s), None);
    assert!(s.sees(BLUEMOOR));
}

#[test]
fn in_the_fog_the_reach_marked_is_as_far_as_is_known() {
    // A road through water: red's soldiers at one end, a blue soldier on it
    // out of sight.
    let mut rows = vec!["w".repeat(30); 30];
    rows[5] = format!("{}c{}", "g".repeat(5), "g".repeat(24));
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 20\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\ngarrison soldiers\n";
    let (mut map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let kind = map.units().kind_named("soldiers").unwrap();
    let blue = map.new_unit_of(Side::Blue, kind).unwrap();
    map.place_unit(blue, Hex::new(5, 10)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup.fog = true;
    let mut s = session(game.with_setup(setup));
    look(&mut s, REDHOLD);
    let beyond = Hex::new(5, 12);
    assert!(!s.sees(Hex::new(5, 10)));
    // The map marks the way on past the unseen soldier, as if nobody were
    // there; the rules know better.
    assert!(!s.screen.game.can_move_to(beyond));
    assert!(Scene::marked(&s, beyond), "the reach does not stop at what is not seen");
    // Going there finds out: something is in the way, and nothing moves.
    look(&mut s, beyond);
    assert_eq!(s.unit_line(), "Something in the fog is in the way.");
    assert_eq!(s.screen.game.map.units_at(REDHOLD).len(), 1);
}

#[test]
fn the_computer_knows_what_a_person_in_its_place_would() {
    let mut game = game(true, Controller::Person);
    let bluemoor = game.map.city_at(BLUEMOOR).unwrap().id;
    game.apply(hexgame::orders::Order::EndTurn).unwrap();
    assert_eq!(game.player(), Side::Blue);
    game.apply(hexgame::orders::Order::Build { city: bluemoor, what: Some(0) }).unwrap();
    game.apply(hexgame::orders::Order::EndTurn).unwrap();
    // Red's view: its own all there; blue's soldiers in their city out of
    // sight, and so is who holds it; what blue builds, blue's gold: not told.
    let view = game.view_for(Side::Red).expect("fog of war");
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
    let view = game.view_for(Side::Red).unwrap();
    assert_eq!(view.map.units().of_side(Side::Blue).map(|u| u.id()).collect::<Vec<_>>(), [near]);
    // Without fog, no view: the whole game is seen.
    assert!(self::game(false, Controller::Person).view_for(Side::Red).is_none());
}
