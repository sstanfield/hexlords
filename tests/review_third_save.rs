//! What the review of 2026-11-23 found in saved games and the replay: a
//! refused move changes nothing (it looked along its way: a refused click
//! scouted, and the game saved after it was another game); a game played
//! on in the computer's turn, or while a round plays out, is shown to the
//! person, not to the computer's side or to nobody; the replay selects
//! nothing after an undo done again; a map with no sides leaves no file
//! that cannot be read; an order carried out reads back (a hex off the
//! map is refused).

mod common;

use hexlords::game::Game;
use hexlords::orders::{Order, RallyPoint};
use hexlords::save;
use hexlords::screen::Scene;
use hexlords::session::{BEGIN_TURN_ID, MISSION_GO_ID, Session};
use hexlords::setup::{Fog, Mode, Setup};
use hexlords::state::{Hex, MapSource, Sends, Side, UnitId};
use turbo_ferret::{Size, SpecialKey};

const UNITS: &str = "unit soldiers\nname Soldiers\nstrength 5\nmoves 12\nenter grass 2, city 1\n\
                     unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n";

fn source(name: &str, rows: Vec<String>, cities: &str) -> MapSource {
    MapSource {
        name: name.to_string(),
        terrain: format!("{}\n{}\n{}\n", rows.len(), rows[0].len(), rows.join("\n")),
        units: Some(UNITS.to_string()),
        cities: Some(cities.to_string()),
    }
}

fn units_at(game: &Game, hex: Hex) -> Vec<UnitId> {
    game.map.units_at(hex).to_vec()
}

/// 20 rows x 30 columns of grass. Red: Redhold (5,2), full (8 soldiers), and
/// Redford (5,8) with one. Blue far away. Both the computer's (passive: the
/// test gives the orders), kept (`record`).
fn full_city(fog: Fog) -> Game {
    let mut rows = vec!["g".repeat(30); 20];
    rows[5] = format!("ggcgggggc{}cg", "g".repeat(19));
    let cities = "city 5 2\nname Redhold\nside red\ngarrison soldiers 8\n\
                  city 5 8\nname Redford\nside red\ngarrison soldiers 1\n\
                  city 5 28\nname Bluemoor\nside blue\ngarrison levy 2\n";
    let game = Game::from_source(source("full", rows, cities), None).unwrap();
    let mut setup = Setup::solo(&game.map, None, "passive");
    setup.set_players("red=passive,blue=passive").unwrap();
    (setup.mode, setup.seed, setup.fog) = (Mode::Turns, 5, fog);
    let mut game = game.with_setup(setup);
    game.record(true);
    game
}

/// The continent, set up as said.
fn continent(mode: Mode, fog: Fog, players: &str) -> Game {
    let (game, _) = Game::load(&common::assets().join("maps").join("continent")).unwrap();
    let mut setup = Setup::solo(&game.map, None, "passive");
    setup.set_players(players).unwrap();
    (setup.mode, setup.seed, setup.fog) = (mode, 7, fog);
    game.with_setup(setup)
}

fn session(game: Game) -> Session {
    common::with_game(&common::assets(), Size::new(1024, 768), 96, game).unwrap()
}

fn click_hex(s: &mut Session, hex: Hex) {
    s.screen.view.center_on(hex);
    let p = common::pixel_of(s, hex);
    common::click(s, p);
}

fn saves(test: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("hexlords-hunt-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

// ---- 1: an order that is refused changes the game ----------------------

/// A move the game refuses ("there is no room there") leaves behind what
/// its units would have seen on the way.
#[test]
fn f1a_a_refused_move_changes_nothing() {
    let mut game = full_city(Fog::Total);
    let a = units_at(&game, Hex::new(5, 8));
    let before = game.sight(Side::Red);
    let refused = game.apply(Order::Move { units: a.clone(), to: Hex::new(5, 2) });
    assert_eq!(refused, Err("there is no room there".to_string()));
    assert!(game.take_steps().is_empty(), "refused: no step is kept");
    let after = game.sight(Side::Red);
    let hexes = (0..20).flat_map(|r| (0..30).map(move |c| Hex::new(r, c)));
    let new: Vec<Hex> = hexes.filter(|h| after.knows(*h) && !before.knows(*h)).collect();
    assert!(new.is_empty(), "a refused order showed red land it had never seen: {new:?}");
}

/// The same by clicks at the screen, with total fog: a stack sent onto a
/// full stack of its side's (refused), then on to land only the refused
/// move showed. The window closes; the game is played on.
#[test]
fn f1b_a_click_onto_a_full_stack_and_the_game_can_be_played_on() {
    let dir = saves("click");
    let game = full_city(Fog::Total);
    let mut setup = game.setup().clone();
    setup.set_players("red=person,blue=passive").unwrap();
    let mut s = session(game.with_setup(setup));
    let file = save::fresh(&dir, "full");
    s.keep_in(file.clone());
    let a = units_at(&s.screen.game, Hex::new(5, 8))[0];
    click_hex(&mut s, Hex::new(5, 8));
    click_hex(&mut s, Hex::new(5, 2));
    if s.asks_mission() {
        common::push_widget(&mut s, MISSION_GO_ID);
    }
    assert!(s.refusal().is_some_and(|lines| lines.iter().any(|l| l.contains("no room"))), "{:?}", s.refusal());
    common::press(&mut s, SpecialKey::Esc);
    assert_eq!(s.screen.game.selection(), [a], "still selected");
    // The land the refused move would have shown is as dark as it was: a
    // click there is refused; one on land red knows moves the unit.
    click_hex(&mut s, Hex::new(2, 4));
    assert!(s.refusal().is_some_and(|lines| lines.iter().any(|l| l.contains("seen that land"))), "{:?}", s.refusal());
    common::press(&mut s, SpecialKey::Esc);
    click_hex(&mut s, Hex::new(5, 9));
    assert_eq!(s.screen.game.map.units().get(a).unwrap().pos(), Hex::new(5, 9), "{:?}", s.refusal());
    assert!(Scene::confirm_quit(&mut s));
    let loaded = save::load_file(&file).map(|l| l.game.digest());
    let _ = std::fs::remove_dir_all(&dir);
    let loaded = loaded.map_err(|why| why.split(".save: ").last().unwrap_or_default().to_string());
    assert_eq!(loaded, Ok(s.screen.game.digest()));
}

/// With fog (not total) the refused move shows red a blue stack: the game
/// remembers it (a ghost), the loaded game does not, and the check number
/// does not tell.
#[test]
fn f1c_a_loaded_game_remembers_what_the_game_remembered() {
    let mut rows = vec!["g".repeat(30); 20];
    rows[2] = format!("gggggc{}", "g".repeat(24));
    rows[5] = format!("ggcgggggc{}cg", "g".repeat(19));
    let cities = "city 5 2\nname Redhold\nside red\ngarrison soldiers 8\n\
                  city 5 8\nname Redford\nside red\ngarrison soldiers 1\n\
                  city 2 5\nname Bluewatch\nside blue\ngarrison levy 2\n\
                  city 5 28\nname Bluemoor\nside blue\ngarrison levy 2\n";
    let game = Game::from_source(source("ghost", rows, cities), None).unwrap();
    let mut setup = Setup::solo(&game.map, None, "passive");
    setup.set_players("red=passive,blue=passive").unwrap();
    (setup.seed, setup.fog) = (5, Fog::On);
    let mut game = game.with_setup(setup);
    game.record(true);
    let a = units_at(&game, Hex::new(5, 8));
    assert!(game.ghost_at(Side::Red, Hex::new(2, 5)).is_none(), "red has not seen who is in Bluewatch");
    let refused = game.apply(Order::Move { units: a.clone(), to: Hex::new(5, 2) });
    assert_eq!(refused, Err("there is no room there".to_string()));
    game.apply(Order::Move { units: a, to: Hex::new(5, 9) }).unwrap();
    let steps = game.take_steps();
    let text = save::text(&game, &steps, &[], 0).unwrap();
    let loaded = save::load(&text).expect("the check number passes").game;
    assert_eq!(loaded.digest(), game.digest());
    assert_eq!(loaded.ghosts(Side::Red), game.ghosts(Side::Red), "what red remembers of the others' stacks");
}

/// The replay does the kept steps again too: after the refused move, the
/// move it made possible does not happen in the past.
#[test]
fn f1d_the_replay_goes_as_the_game_went() {
    let mut game = full_city(Fog::Total);
    game.watch(true);
    let mut replay = hexlords::replay::Replay::begin(&game);
    let a = units_at(&game, Hex::new(5, 8));
    assert!(game.apply(Order::Move { units: a.clone(), to: Hex::new(5, 2) }).is_err());
    // (What the refused move would have shown is not known: no going there.)
    assert!(game.apply(Order::Move { units: a.clone(), to: Hex::new(2, 4) }).is_err());
    game.apply(Order::Move { units: a.clone(), to: Hex::new(5, 9) }).unwrap();
    game.take_steps().into_iter().for_each(|step| replay.push(step));
    assert_eq!(replay.len(), 1);
    let then = replay.state(1);
    assert_eq!(then.map.units().get(a[0]).map(|u| u.pos()), game.map.units().get(a[0]).map(|u| u.pos()));
}

// ---- 2: played on, the screen is not the person's ----------------------

/// One person (red) against the computer, with fog. The file is written
/// as red's turn ends; the program stops there and the game is played on:
/// while the computer plays, the screen is still red's.
#[test]
fn f2a_a_game_played_on_in_the_computers_turn_is_shown_to_the_person() {
    let dir = saves("seer");
    let players = "red=person,green=passive,blue=passive,yellow=passive";
    let mut s = session(continent(Mode::Turns, Fog::On, players));
    let file = save::fresh(&dir, "continent");
    s.keep_in(file.clone());
    s.end_turn();
    s.pump();
    assert_eq!(s.screen.game.player(), Side::Green);
    assert_eq!(s.seer(), Side::Red, "the game goes on: the screen is red's while green plays");
    let green = s.screen.game.capital(Side::Green).unwrap();
    assert!(!s.sight().sees(green), "red does not see green's capital");
    drop(s);
    // Played on from the file, as the shell does (`Shell::switch`).
    let loaded = save::load_file(&file).unwrap();
    assert_eq!((loaded.game.turn(), loaded.game.player()), (1, Side::Green), "kept as the turn passed");
    let mut s = session(loaded.game);
    s.restore(loaded.steps, loaded.seen);
    s.keep_in(file.clone());
    s.pump();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(s.seer(), Side::Red, "played on: whose the screen is");
    assert!(!s.sight().sees(green), "played on: red does not see green's capital");
}

/// The same in rounds: the window closed while the round plays out.
#[test]
fn f2b_a_round_played_on_is_shown_to_the_person() {
    let dir = saves("round");
    let players = "red=person,green=passive,blue=passive,yellow=passive";
    let mut s = session(continent(Mode::Rounds, Fog::On, players));
    let file = save::fresh(&dir, "continent");
    s.keep_in(file.clone());
    let capital = s.screen.game.capital(Side::Red).unwrap();
    let stack = units_at(&s.screen.game, capital);
    let near: Vec<Hex> = s.screen.game.map.neighbours(capital).collect();
    assert!(near.into_iter().any(|to| s.screen.game.apply(Order::Toward { units: stack.clone(), to }).is_ok()));
    s.pump();
    s.end_turn();
    let mut now = 1000;
    while !s.screen.game.playing_out() {
        now += 10;
        s.frame(now);
        assert!(now < 20_000, "the round begins to play out");
    }
    let green = s.screen.game.capital(Side::Green).unwrap();
    assert_eq!(s.seer(), Side::Red);
    assert!(!s.sight().sees(green), "red does not see green's capital");
    assert!(Scene::confirm_quit(&mut s));
    drop(s);
    let loaded = save::load_file(&file).unwrap();
    assert!(loaded.game.playing_out());
    let mut s = session(loaded.game);
    s.restore(loaded.steps, loaded.seen);
    s.keep_in(file.clone());
    s.pump();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(s.seer(), Side::Red, "played on: whose the screen is (grey: nobody's, all of the map shows)");
    assert!(!s.sight().sees(green), "played on: red does not see green's capital");
}

/// (No saved game: a new one where the computer's turn comes first.)
#[test]
fn f2c_a_new_game_the_computer_begins_is_shown_to_the_person() {
    let players = "red=passive,green=person,blue=passive,yellow=passive";
    let mut s = session(continent(Mode::Turns, Fog::On, players));
    s.pump();
    assert_eq!(s.screen.game.player(), Side::Red);
    assert_eq!(s.seer(), Side::Green, "the one person in the game");
}

// ---- 3: the replay shows a stack the person cannot see -----------------

/// Hotseat with fog: blue moves a stack red cannot see and takes the move
/// back. Red plays the replay: nothing of blue's stack shows.
#[test]
fn f3_the_replay_shows_nothing_of_a_move_taken_back() {
    let mut rows = vec!["g".repeat(30); 30];
    rows[5] = format!("{}c{}", "g".repeat(5), "g".repeat(24));
    rows[24] = format!("{}c{}", "g".repeat(24), "g".repeat(5));
    let cities = "city 5 5\nname Redhold\nside red\ngarrison soldiers 1\n\
                  city 24 24\nname Bluemoor\nside blue\ngarrison levy 1\n";
    let game = Game::from_source(source("hotseat", rows, cities), None).unwrap();
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup.set_players("red=person,blue=person").unwrap();
    setup.fog = Fog::On;
    let options = hexlords::options_window::Options { show_moves: true, ..hexlords::options_window::Options::manual() };
    let game = game.with_setup(setup);
    let mut s = Session::with_options(&common::assets(), Size::new(1024, 768), 96, game, options).unwrap();
    let levy = units_at(&s.screen.game, Hex::new(24, 24))[0];
    // Red's turn: nothing; blue's: the levy out of Bluemoor, and back.
    common::push_widget(&mut s, BEGIN_TURN_ID);
    s.end_turn();
    common::push_widget(&mut s, BEGIN_TURN_ID);
    assert_eq!(s.screen.game.player(), Side::Blue);
    click_hex(&mut s, Hex::new(24, 24));
    click_hex(&mut s, Hex::new(24, 21));
    assert_eq!(s.screen.game.map.units().get(levy).unwrap().pos(), Hex::new(24, 21));
    s.undo();
    assert_eq!(s.screen.game.map.units().get(levy).unwrap().pos(), Hex::new(24, 24));
    s.pump();
    s.end_turn();
    common::push_widget(&mut s, BEGIN_TURN_ID);
    s.start_turn();
    s.pump();
    assert_eq!((s.screen.game.player(), s.screen.game.turn()), (Side::Red, 2));
    assert!(!s.sight().sees(Hex::new(24, 24)), "red does not see Bluemoor");
    // Red plays what happened again (the Replay button, W).
    let steps = s.replay_at().expect("something to look at again").1;
    s.play_replay();
    assert!(s.in_the_past());
    let mut shown = Vec::new();
    let mut t = 0;
    while s.in_the_past() && t < 120_000 {
        t += 20;
        s.frame(t);
        if !s.in_the_past() {
            break;
        }
        let at = s.replay_at().unwrap().0;
        let front = Scene::front_unit(&s);
        let rings = Scene::rings(&s);
        let hexes = (0..30u16).flat_map(|r| (0..30u16).map(move |c| Hex::new(r, c)));
        let marked = hexes.filter(|h| Scene::marked(&s, *h)).count();
        if front.is_some() || !rings.is_empty() || marked > 0 {
            shown.push(format!("step {at} of {steps}: front {front:?}, rings {rings:?}, {marked} hexes marked"));
        }
    }
    shown.dedup();
    assert!(shown.is_empty(), "the replay shows red: {shown:#?}");
}

// ---- 4: a map with no sides leaves a file that cannot be read ----------

/// `cargo run -- --map assets/maps/example.map`: a map with no sides. What
/// is kept of it can be listed (or nothing is kept).
#[test]
fn f4_a_map_with_no_sides_leaves_no_broken_file() {
    let dir = saves("nosides");
    let (game, _) = Game::load(&common::assets().join("maps").join("example.map")).unwrap();
    // (As main.rs sets it up.)
    let setup = Setup::solo(&game.map, None, "steady");
    let mut shell = hexlords::shell::Shell::playing(&common::assets(), session(game.with_setup(setup)));
    shell.keep_games_in(dir.clone());
    assert!(shell.confirm_quit());
    let found = save::list(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    for (path, summary) in &found {
        assert!(summary.is_ok(), "{}: {summary:?}", path.file_name().unwrap().to_string_lossy());
    }
}

// ---- 5: an order carried out that a saved game cannot hold -------------

/// An order the game carries out is one a saved game can hold: a hex off
/// the map is refused by the reader, so the game must refuse it too.
#[test]
fn f5_an_order_carried_out_can_be_read_back() {
    let mut carried = Vec::new();
    for mode in [Mode::Turns, Mode::Rounds] {
        for fog in [Fog::Off, Fog::On, Fog::Total] {
            for off in [Hex::new(500, 500), Hex::new(5, 31), Hex::new(21, 3)] {
                let game = full_city(fog);
                let mut setup = game.setup().clone();
                setup.mode = mode;
                let mut game = game.with_setup(setup);
                game.record(true);
                let a = units_at(&game, Hex::new(5, 8));
                let city = game.map.city_at(Hex::new(5, 8)).unwrap().id;
                let orders = [
                    Order::Move { units: a.clone(), to: off },
                    Order::Toward { units: a.clone(), to: off },
                    Order::GoTo { units: a.clone(), to: off, via: vec![] },
                    Order::GoTo { units: a.clone(), to: Hex::new(6, 9), via: vec![off] },
                    Order::Patrol { units: a.clone(), waypoints: vec![off, Hex::new(6, 9)], attack: None },
                    Order::Roam { units: a.clone(), to: off },
                    Order::Rally { city, to: Sends::Route(vec![off]) },
                    Order::RallyPoint { n: 1, point: Some(RallyPoint { at: off, defend: false }) },
                ];
                for order in orders {
                    if game.apply(order.clone()).is_ok() {
                        let steps = game.take_steps();
                        let text = save::text(&game, &steps, &[], 0).unwrap();
                        if let Err(why) = save::load(&text) {
                            carried.push(format!("{mode:?} {fog:?} {order:?}: {why}"));
                        }
                    }
                }
            }
        }
    }
    assert!(carried.is_empty(), "{carried:#?}");
}
