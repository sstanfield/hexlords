//! What the bug hunt found (2026-11-21: the game played for many turns by
//! fuzzers and computer players), each held to what the docs say: the
//! patrol's window and other picks, the disband window and its units,
//! orders by key starting the turn, the patrol's guard, the line under the
//! stack in hotseat, a city's buttons in another side's turn, rally points
//! on water or in another's city; a round of the computer's turns, an
//! agent's view of the others' defenders, the fog in round 1.

mod common;

use common::{assets, click, pixel_of, press, push_widget, type_char};
use hexgame::game::Game;
use hexgame::options_window::Options;
use hexgame::session::{Pick, Session};
use hexgame::setup::Setup;
use hexgame::state::scenario::{self, Scenario};
use hexgame::state::{Hex, Side, UnitId};
use turbo_ferret::{Size, SpecialKey};

const REDHOLD: Hex = Hex::new(5, 5);
const EAST: Hex = Hex::new(5, 25);
const SOUTH: Hex = Hex::new(10, 6);
const LEVY: Hex = Hex::new(3, 8);

/// turn_flow.rs's game: red's Redhold, soldiers on EAST and SOUTH; blue's
/// levy near Redhold, and Bluemoor in the far corner.
fn game() -> Game {
    let mut rows = vec!["g".repeat(30); 30];
    rows[5] = format!("{}c{}", "g".repeat(5), "g".repeat(24));
    rows[8] = format!("{}t{}", "g".repeat(3), "g".repeat(26));
    rows[28] = format!("{}c{}", "g".repeat(28), "g");
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nprice 5\nupkeep 4\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n\
                 unit giant\nname Giant\nstrength 9\nmoves 12\nenter grass 2, city 1\n";
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

fn session(game: Game, options: Options) -> Session {
    Session::with_options(&assets(), Size::new(1024, 768), 96, game, options).expect("the session starts")
}

fn click_hex(s: &mut Session, hex: Hex) {
    s.screen.view.center_on(hex);
    let p = pixel_of(s, hex);
    click(s, p);
}

fn at(s: &Session, hex: Hex) -> Vec<UnitId> {
    s.screen.game.map.units_at(hex).to_vec()
}

/// While the patrol's window asks about one stack, the person selects
/// another and starts a Go to for it: the window goes, and Enter is the Go
/// to's (it gave the first its patrol, and left Go to picking for nobody).
#[test]
fn a_go_to_picked_while_the_patrol_window_asked_is_not_left_for_nobody() {
    let mut s = session(game(), Options::manual());
    let south = at(&s, SOUTH);
    click_hex(&mut s, SOUTH);
    type_char(&mut s, 'p');
    click_hex(&mut s, Hex::new(13, 6));
    assert!(s.patrol_window().is_some(), "the patrol's window asks");
    // Another stack (Next stack): the window goes; Go to for that one.
    type_char(&mut s, 'n');
    assert!(s.patrol_window().is_none());
    let other = s.screen.game.selection().to_vec();
    type_char(&mut s, 'g');
    assert_eq!(s.picking(), Some(Pick::GoTo));
    s.screen.game.set_cursor(Hex::new(5, 28));
    press(&mut s, SpecialKey::Enter);
    let g = &s.screen.game;
    assert_eq!(s.picking(), None);
    assert_eq!(g.map.units().get(other[0]).unwrap().goto(), Some(Hex::new(5, 28)), "the Go to was given");
    assert!(g.map.units().get(south[0]).unwrap().patrol.is_none(), "and no patrol");
}

/// The disband window asks about the selected stack; the stack then dies
/// in a battle of its own (an attack): the window goes with it.
#[test]
fn the_disband_window_goes_with_its_units() {
    let mut g = game();
    // A giant of blue's beside the south stack; no chance in the battle:
    // the stronger wins.
    let giant = g.map.units().kind_named("giant").unwrap();
    let id = g.map.new_unit_of(Side::Blue, giant).unwrap();
    g.map.place_unit(id, Hex::new(11, 6)).unwrap();
    let mut setup = g.setup().clone();
    setup.combat.chance = 0;
    let g = g.with_setup(setup);
    let mut s = session(g, Options::manual());
    let south = at(&s, SOUTH);
    click_hex(&mut s, SOUTH);
    type_char(&mut s, 'x');
    assert!(s.disband().is_some(), "the disband window asks");
    // They attack the giant, and die.
    click_hex(&mut s, Hex::new(11, 6));
    assert!(south.iter().all(|id| s.screen.game.map.units().get(*id).is_none()), "they fell");
    assert!(s.disband().is_none(), "the window went with them");
}

/// The person's turn starts with their log open (a march stopped short):
/// "Start turn closes it (so does any order)": an order by a key too (D).
#[test]
fn an_order_by_key_starts_the_turn() {
    let mut g = game();
    let kind = g.map.units().kind_named("soldiers").unwrap();
    let scout = g.map.new_unit_of(Side::Red, kind).unwrap();
    g.map.place_unit(scout, Hex::new(4, 8)).unwrap();
    let mut s = session(g, Options::default());
    // A scout marches on blue's levy beside it: it stops short at the end
    // of the turn; red's next turn starts with its log open.
    s.screen.game.apply(hexgame::orders::Order::GoTo { units: vec![scout], to: LEVY, via: vec![] }).unwrap();
    s.end_turn();
    s.wait_for_turn();
    s.frame(10_000);
    assert!(!s.log_lines().is_empty(), "the log is open");
    let label = |s: &Session| {
        s.screen.ui.text(common::widget(s, hexgame::session::END_TURN_ID)).unwrap_or_default().to_string()
    };
    assert_eq!(label(&s), "Start turn");
    // Next stack (only a look), then Done: an order, which starts the turn.
    type_char(&mut s, 'n');
    assert!(!s.log_lines().is_empty(), "a look is no order");
    type_char(&mut s, 'd');
    s.frame(10_100);
    assert!(s.log_lines().is_empty(), "the log closed");
    assert_ne!(label(&s), "Start turn");
}

/// A stack on patrol is selected; a click on another of the player's
/// stacks in its reach asks whether to join it or take it (it said "click
/// there again to send it there", and a second click only selected it).
#[test]
fn a_click_on_another_stack_with_a_patrol_selected_asks_join_or_take() {
    let mut g = game();
    let kind = g.map.units().kind_named("soldiers").unwrap();
    let other = g.map.new_unit_of(Side::Red, kind).unwrap();
    g.map.place_unit(other, Hex::new(12, 6)).unwrap();
    let mut s = session(g, Options::manual());
    let south = at(&s, SOUTH);
    s.screen
        .game
        .apply(hexgame::orders::Order::Patrol {
            units: south.clone(),
            waypoints: vec![SOUTH, Hex::new(10, 2)],
            attack: None,
        })
        .unwrap();
    s.screen.game.end_turn();
    s.screen.game.end_turn();
    s.frame(1);
    let pos = s.screen.game.map.units().get(south[0]).unwrap().pos();
    click_hex(&mut s, pos);
    let target = Hex::new(12, 6);
    // The other stack is in reach: join it, or take it? Take it.
    click_hex(&mut s, target);
    assert!(s.asks_mission(), "it asks");
    push_widget(&mut s, hexgame::session::MISSION_TAKE_ID);
    assert_eq!(s.screen.game.selection(), [other], "the other stack is selected");
    assert!(s.screen.game.map.units().get(south[0]).unwrap().patrol.is_some(), "the patrol goes on");
}

/// Hotseat: red's last line under the stack is gone when blue begins.
#[test]
fn in_hotseat_the_line_under_the_stack_does_not_carry_over() {
    let g = game();
    let mut setup = g.setup().clone();
    setup.set_players("red=person,blue=person").unwrap();
    let g = g.with_setup(setup);
    let mut s = session(g, Options::manual());
    if s.covered() {
        push_widget(&mut s, hexgame::session::BEGIN_TURN_ID);
    }
    click_hex(&mut s, SOUTH);
    // Red is told something: a click on the selected patrol-less stack's far
    // hex it cannot reach is a Go to; one onto Bluemoor's hex as a patrol
    // waypoint is refused with a line. Use the plainest: a Go to refused.
    type_char(&mut s, 'p');
    click_hex(&mut s, Hex::new(28, 28));
    press(&mut s, SpecialKey::Esc);
    // A battle told in a line: red attacks blue's levy from Redhold.
    let mut o = s.options();
    o.quick_battles = true;
    s.set_options(o);
    click_hex(&mut s, REDHOLD);
    click_hex(&mut s, LEVY);
    s.end_turn();
    s.wait_for_turn();
    push_widget(&mut s, hexgame::session::BEGIN_TURN_ID);
    s.frame(5000);
    assert!(!s.unit_line().starts_with("You attacked"), "red's battle line is shown to blue");
}

/// While the patrol's window asks, N goes on to another stack: the window
/// goes (it stayed, and its stack could be moved under it), and Enter is
/// no patrol.
#[test]
fn the_patrol_window_goes_when_the_person_goes_on_to_another_stack() {
    let mut s = session(game(), Options::manual());
    let south = at(&s, SOUTH);
    click_hex(&mut s, SOUTH);
    type_char(&mut s, 'p');
    click_hex(&mut s, Hex::new(13, 6));
    assert!(s.patrol_window().is_some());
    assert!(s.screen.game.selection().is_empty(), "while it asks, nothing is selected");
    type_char(&mut s, 'n');
    assert!(s.patrol_window().is_none(), "the window went");
    s.screen.game.set_cursor(Hex::new(10, 9));
    press(&mut s, SpecialKey::Enter);
    let u = s.screen.game.map.units().get(south[0]).unwrap();
    assert!(u.patrol.is_none(), "no patrol was given");
}

/// The city window's Send to... during the computer's turn does nothing (it
/// started a pick, refused at the next click).
#[test]
fn send_to_does_nothing_in_the_computers_turn() {
    let mut s = session(game(), Options::manual());
    click_hex(&mut s, REDHOLD);
    type_char(&mut s, 'o');
    s.end_turn();
    assert_ne!(s.screen.game.player(), Side::Red);
    push_widget(&mut s, hexgame::city_windows::CITY_RALLY_ID);
    assert_eq!(s.picking(), None, "no pick in blue's turn");
    click_hex(&mut s, Hex::new(9, 9));
}

/// "Rally points ... placed on land it has seen": not on open water, nor in
/// another side's city.
#[test]
fn a_rally_point_goes_on_land_and_in_no_other_sides_city() {
    let mut rows = vec!["g".repeat(30); 30];
    rows[5] = format!("{}c{}c{}", "g".repeat(5), "g".repeat(19), "g".repeat(4));
    rows[10] = format!("{}w{}", "g".repeat(10), "g".repeat(19));
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\nproduce soldiers 1\ngarrison soldiers\ncity 5 25\nname Bluemoor\nside blue\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    game = game.with_setup(setup);
    use hexgame::orders::{Order, RallyPoint};
    let water = game.apply(Order::RallyPoint { n: 0, point: Some(RallyPoint { at: Hex::new(10, 10), defend: false }) });
    let city = game.apply(Order::RallyPoint { n: 1, point: Some(RallyPoint { at: Hex::new(5, 25), defend: false }) });
    assert_eq!(water, Err("a rally point goes on land".to_string()));
    assert_eq!(city, Err("a rally point goes in no city but its side's".to_string()));
}

/// `agents::play_computers` plays a round: a turn for each side still in
/// (it counted the sides out too, so `--rounds` ran past its rounds).
#[test]
fn a_round_of_the_computers_turns_is_one_for_each_side_still_in() {
    let mut rows = vec!["g".repeat(20); 20];
    rows[5] = "gggggcgcgggggggggggg".to_string();
    rows[15] = "gggggggggggggggcgggg".to_string();
    let terrain = format!("20\n20\n{}\n", rows.join("\n"));
    let units = "unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\ngarrison levy\ncity 5 7\nname Bluemoor\nside blue\n\
                  city 15 15\nname Greenford\nside green\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup.set_players("red=passive,blue=passive,green=passive").unwrap();
    let mut game = game.with_setup(setup);
    assert_eq!(game.order(), [Side::Red, Side::Green, Side::Blue]);
    // Red takes Bluemoor, empty: blue is out. Green's turn.
    let levy = game.map.units_at(REDHOLD).to_vec();
    game.apply(hexgame::orders::Order::Move { units: levy, to: Hex::new(5, 7) }).unwrap();
    assert!(game.out().contains(&Side::Blue));
    game.apply(hexgame::orders::Order::EndTurn).unwrap();
    assert_eq!((game.player(), game.turn()), (Side::Green, 1));
    // A round: green's turn and red's, and green's of round 2 is next.
    let mut agents = hexgame::agents::Agents::for_game(&game).unwrap();
    hexgame::agents::play_computers(&mut game, &mut agents);
    assert_eq!((game.player(), game.turn()), (Side::Green, 2));
}

/// An agent's view says nothing of which of the others' units defend (a
/// standing order: it is the others' own).
#[test]
fn an_agents_view_does_not_say_which_of_the_others_units_defend() {
    let mut game = game();
    game.apply(hexgame::orders::Order::EndTurn).unwrap();
    let levy = game.map.units_at(LEVY).to_vec();
    game.apply(hexgame::orders::Order::Defend { units: levy.clone(), on: true }).unwrap();
    assert!(game.map.units().get(levy[0]).unwrap().defending);
    let red = game.view_for(Side::Red);
    assert!(!red.map.units().get(levy[0]).unwrap().defending, "red is not told");
    let blue = game.view_for(Side::Blue);
    assert!(blue.map.units().get(levy[0]).unwrap().defending, "blue knows its own");
}

/// With fog of war, what a side's units saw on their way is seen no longer
/// once its turn is over: the screen works it out again then (in round 1
/// nothing else changed the map, and it showed the old sight).
#[test]
fn the_screen_forgets_what_was_seen_on_the_way_when_the_turn_ends() {
    let game = game();
    let mut setup = game.setup().clone();
    setup.fog = hexgame::setup::Fog::On;
    let mut s = session(game.with_setup(setup), Options::manual());
    click_hex(&mut s, SOUTH);
    click_hex(&mut s, Hex::new(16, 6));
    assert!(s.sight().sees(SOUTH), "seen on the way, this turn");
    s.end_turn();
    s.pump();
    assert!(!s.sight().sees(SOUTH), "no longer");
}
