//! Saved games (`save`): a game the computer's agents played is the same
//! game when it is loaded; the window keeps the game being played in a file
//! as its turns pass and when it closes; the new game screen lists the
//! games in progress, plays them on and deletes them; a game that is over
//! is kept no more. No window and no GPU: the shell on the real assets.

mod common;

use std::path::{Path, PathBuf};

use common::{assets, click, widget};
use hexlords::agents::{self, Agents};
use hexlords::game::Game;
use hexlords::new_game::{MAKE_ID, MAP_FACTS_ID, RANDOM_ID, START_ID, continue_id, delete_id};
use hexlords::orders::{Order, Step};
use hexlords::save;
use hexlords::screen::Scene;
use hexlords::session::{ASK_NEW_GAME_ID, NEW_GAME_ID};
use hexlords::setup::{Fog, Mode, Setup};
use hexlords::shell::Shell;
use hexlords::state::{Hex, MapSource, Side, UnitId};
use turbo_ferret::{Point, Size, TbId};

/// An empty directory for a test's saved games.
fn saves(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("hexlords-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn shell(dir: &Path) -> Shell {
    let mut shell = Shell::new_game(&assets(), Size::new(1024, 768), 96, None).unwrap();
    shell.keep_games_in(dir.to_path_buf());
    shell
}

fn exists(s: &dyn Scene, name: &str) -> bool {
    s.screen().ui.find(s.screen().ui.root(), TbId::new(name)).is_some()
}

fn text_of(s: &dyn Scene, name: &str) -> Option<String> {
    let ui = &s.screen().ui;
    ui.find(ui.root(), TbId::new(name)).and_then(|id| ui.text(id)).map(str::to_string)
}

/// Click the middle of a widget of whatever the shell shows.
fn push(shell: &mut Shell, name: &str) {
    let id = shell.screen().ui.find(shell.screen().ui.root(), TbId::new(name)).unwrap_or_else(|| panic!("no {name}"));
    shell.screen_mut().ui.scroll_into_view_recursive(id);
    shell.screen_mut().settle();
    let r = shell.screen().area(id);
    let p = Point::new(r.x + r.w / 2, r.y + r.h / 2);
    let ui = &mut shell.screen_mut().ui;
    ui.pointer_move(p, turbo_ferret::Modifiers::NONE);
    ui.pointer_down(p, 1, turbo_ferret::Modifiers::NONE);
    ui.pointer_up(p, turbo_ferret::Modifiers::NONE);
    shell.pump();
}

fn places(game: &Game) -> Vec<(UnitId, Hex, u32, u32)> {
    game.map.units().iter().map(|u| (u.id(), u.pos(), u.movement, u.wounds)).collect()
}

/// The continent with the computer on every side, each its own agent, with
/// fog of war: kept, as a window keeps it.
fn computers(mode: Mode) -> Game {
    let (game, _) = Game::load(&assets().join("maps").join("continent")).unwrap();
    let mut setup = Setup::solo(&game.map, None, "steady");
    setup.set_players("red=steady,green=bold,blue=careful,yellow=simple").unwrap();
    (setup.mode, setup.seed, setup.fog) = (mode, 11, Fog::On);
    (setup.combat.wounds, setup.combat.blows_back) = (true, true);
    let mut game = game.with_setup(setup);
    game.record(true);
    game
}

/// Play on until round `rounds` is over (or the game is).
fn play(game: &mut Game, agents: &mut Agents, rounds: u32) {
    while game.turn() <= rounds && game.winner().is_none() {
        if game.playing_out() {
            game.play_out();
        } else {
            agents::play_computers(game, agents);
        }
    }
}

/// A game the agents played for some rounds, saved in the middle of a turn
/// (in rounds: of a round playing out), is the same game when it is loaded:
/// every unit, the whole history, the chance; and played on from there by
/// the same players, both go the same way.
#[test]
fn a_game_the_agents_played_is_loaded_as_it_was() {
    for (mode, rounds) in [(Mode::Turns, 12), (Mode::Rounds, 12)] {
        let mut game = computers(mode);
        let mut agents = Agents::for_game(&game).unwrap();
        play(&mut game, &mut agents, rounds);
        // On into the next: a few orders of the side whose turn it is; in
        // rounds, all the orders and two beats of the round.
        if mode == Mode::Turns {
            let side = game.player();
            for given in 0..3 {
                agents::play_step(&mut game, agents.get_mut(side).unwrap(), given);
            }
        } else {
            agents::play_computers(&mut game, &mut agents);
            assert!(game.playing_out());
            game.play_out();
            game.play_out();
        }
        let steps = game.take_steps();
        assert!(steps.len() > 200, "{mode:?}: {} steps", steps.len());
        assert_eq!(steps.contains(&Step::Beat), mode == Mode::Rounds);
        assert!(game.history().iter().any(|r| matches!(r.event, hexlords::orders::Event::Battle { .. })));
        let text = save::text(&game, &steps, &[], 0).unwrap();
        let loaded = save::load(&text).unwrap_or_else(|why| panic!("{mode:?}: {why}"));
        assert_eq!(loaded.steps.len(), steps.len());
        let mut again = loaded.game;
        assert_eq!(places(&again), places(&game), "{mode:?}");
        assert_eq!(again.history(), game.history(), "{mode:?}");
        assert_eq!(
            (again.turn(), again.player(), again.playing_out()),
            (game.turn(), game.player(), game.playing_out())
        );
        assert_eq!(again.digest(), game.digest(), "{mode:?}");
        for side in [Side::Red, Side::Green, Side::Blue, Side::Yellow] {
            assert_eq!(again.gold(side), game.gold(side), "{mode:?}");
        }
        // On from there, the same players anew in each: the same game.
        let (mut agents, mut agents_again) = (Agents::for_game(&game).unwrap(), Agents::for_game(&again).unwrap());
        again.record(true);
        play(&mut game, &mut agents, rounds + 3);
        play(&mut again, &mut agents_again, rounds + 3);
        assert_eq!((places(&again), again.digest()), (places(&game), game.digest()), "{mode:?}: played on");
    }
}

/// The computer leading a side's marauders lets those that fought go on
/// before the turn's end does: that is no order, and it is kept all the
/// same, so that the game loaded is the game as it was (without it, a stack
/// that fought as the turn ended waited for its player's word in the one
/// and not in the other).
#[test]
fn what_the_marauders_leader_does_is_kept_too() {
    let mut rows = vec!["g".repeat(20); 20];
    rows[5] = "ggcggggggggggggggcgg".to_string();
    rows[16] = "gggggggggggggggggcgg".to_string();
    let source = MapSource {
        name: "marauders".to_string(),
        terrain: format!("20\n20\n{}\n", rows.join("\n")),
        units: Some(
            "unit soldiers\nname Soldiers\nstrength 8\nmoves 12\nenter grass 2, city 1\n\
             unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n"
                .to_string(),
        ),
        cities: Some(
            "city 5 2\nname Redhold\nside red\nproduce soldiers 1\ngarrison soldiers 3\n\
             city 5 17\nname Bluemoor\nside blue\nproduce levy 1\ngarrison levy 2\n\
             city 16 17\nname Farhold\nside blue\nproduce levy 1\ngarrison levy 3\n"
                .to_string(),
        ),
    };
    rows.clear();
    let game = Game::from_source(source, None).unwrap();
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    (setup.combat.chance, setup.seed) = (0, 3);
    let mut game = game.with_setup(setup);
    game.record(true);
    // Red's soldiers maraud; blue sends a levy out each turn, into their way.
    let soldiers: Vec<UnitId> = game.map.units().of_side(Side::Red).map(|u| u.id()).collect();
    game.apply(Order::Maraud { units: soldiers[..2].to_vec(), on: true }).unwrap();
    let mut fought = false;
    for _ in 0..8 {
        if game.winner().is_some() {
            break;
        }
        hexlords::marauder::maraud(&mut game, Side::Red);
        fought = game.map.units().of_side(Side::Red).any(|u| u.fought);
        if game.winner().is_some() {
            break;
        }
        game.apply(Order::EndTurn).unwrap();
        // (Saved here: they fought as the turn ended, and wait.)
        if fought {
            break;
        }
        if game.winner().is_some() {
            break;
        }
        let levy: Vec<UnitId> = game.map.units_at(Hex::new(5, 17)).iter().copied().take(1).collect();
        let _ = game.apply(Order::Toward { units: levy, to: Hex::new(5, 10) });
        game.apply(Order::EndTurn).unwrap();
    }
    assert!(fought && game.winner().is_none(), "the marauders fought, and wait for their player's word");
    assert!(game.map.units().of_side(Side::Red).any(|u| u.fought), "still, after the turn's end");
    let steps = game.take_steps();
    assert!(steps.iter().any(|s| matches!(s, Step::Ease(Side::Red))), "{steps:?}");
    assert!(steps.iter().any(|s| matches!(s, Step::Order(Order::Roam { .. }))));
    let text = save::text(&game, &steps, &[], 0).unwrap();
    let again = save::load(&text).unwrap().game;
    assert_eq!((places(&again), again.digest()), (places(&game), game.digest()));
    let waits = |g: &Game| g.map.units().iter().map(|u| (u.id(), u.fought, u.marauding)).collect::<Vec<_>>();
    assert_eq!(waits(&again), waits(&game));
}

/// The window keeps the game: a file as soon as it starts, written again as
/// the turns pass and when the window closes (with the orders given since);
/// the next time, the new game screen lists it, and Play on is the game as
/// it was left.
#[test]
fn the_game_being_played_is_kept_and_played_on_later() {
    let dir = saves("kept");
    let mut shell = shell(&dir);
    assert!(shell.new_game_screen().unwrap().saved().is_empty());
    assert!(!exists(&shell, &continue_id(0)));
    push(&mut shell, START_ID);
    let file = dir.join("continent-1.save");
    let s = shell.session().expect("the game");
    assert_eq!(s.kept_in(), Some(file.as_path()));
    assert_eq!(save::summary(&std::fs::read_to_string(&file).unwrap()).unwrap().round, 1, "kept at once");
    // Red's first turn: a stack sent off, the turn ended; the computer's
    // turns; red's second turn.
    let capital = s.screen.game.capital(Side::Red).unwrap();
    let stack: Vec<UnitId> = s.screen.game.map.units_at(capital).to_vec();
    let near: Vec<Hex> = s.screen.game.map.neighbours(capital).collect();
    let sent = near.into_iter().any(|to| s.screen.game.apply(Order::Toward { units: stack.clone(), to }).is_ok());
    assert!(sent, "the stack goes a hex");
    s.pump();
    s.end_turn();
    s.wait_for_turn();
    s.start_turn();
    assert_eq!((s.screen.game.turn(), s.screen.game.player()), (2, Side::Red));
    s.pump();
    let kept = save::load_file(&file).unwrap();
    assert_eq!((kept.game.turn(), kept.game.player()), (2, Side::Red), "written as the turns passed");
    // An order in the second turn, and the window is closed.
    let city = s.screen.game.map.city_at(capital).unwrap().id;
    s.screen.game.apply(Order::Build { city, what: Some(1) }).unwrap();
    let left = (s.screen.game.digest(), places(&s.screen.game), s.screen.game.history().to_vec());
    assert!(shell.confirm_quit(), "the window closes");
    drop(shell);

    // The next time: the game is listed, and played on as it was left.
    let mut shell = self::shell(&dir);
    let screen = shell.new_game_screen().unwrap();
    assert_eq!(screen.saved().len(), 1);
    let summary = screen.saved()[0].1.as_ref().unwrap();
    assert_eq!(
        (summary.line().as_str(), summary.players().as_str()),
        ("continent, round 2, turns", "red and 3 computers")
    );
    assert!(exists(&shell, &continue_id(0)) && exists(&shell, &delete_id(0)));
    push(&mut shell, &continue_id(0));
    let s = shell.session().expect("the game, played on");
    assert_eq!(s.kept_in(), Some(file.as_path()), "kept in the same file");
    assert_eq!((s.screen.game.digest(), places(&s.screen.game), s.screen.game.history().to_vec()), left);
    assert_eq!((s.screen.game.turn(), s.screen.game.player()), (2, Side::Red));
    assert_eq!(s.screen.game.map.city_at(capital).unwrap().building, Some(1), "the last order too");
    // It is red's to play: on, a turn; kept as it goes.
    s.end_turn();
    s.wait_for_turn();
    s.start_turn();
    s.pump();
    assert_eq!(s.screen.game.turn(), 3);
    assert_eq!(save::load_file(&file).unwrap().game.digest(), s.screen.game.digest());

    // Left for a new game: the window that asks says it is kept; the new
    // game screen lists it.
    s.ask_new_game();
    let says = common::descendants(s, ASK_NEW_GAME_ID);
    let says: Vec<&str> = says.iter().filter_map(|w| s.screen.ui.text(*w)).collect();
    assert!(says.iter().any(|t| t.contains("it is kept, to be played on")), "{says:?}");
    push(&mut shell, NEW_GAME_ID);
    let screen = shell.new_game_screen().expect("the new game screen");
    assert_eq!(screen.saved().len(), 1);
    assert_eq!(screen.saved()[0].1.as_ref().unwrap().round, 3);
    // Another game of the same map is another file.
    push(&mut shell, START_ID);
    assert_eq!(shell.session().unwrap().kept_in(), Some(dir.join("continent-2.save").as_path()));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Delete asks once: pressed again, the game is gone, and its file.
#[test]
fn a_game_in_progress_is_deleted_from_the_new_game_screen() {
    let dir = saves("deleted");
    let mut shell = shell(&dir);
    push(&mut shell, START_ID);
    shell.session().unwrap().ask_new_game();
    push(&mut shell, NEW_GAME_ID);
    let file = dir.join("continent-1.save");
    assert!(file.is_file() && shell.new_game_screen().unwrap().saved().len() == 1);
    assert_eq!(text_of(&shell, &delete_id(0)).as_deref(), Some("Delete"));
    push(&mut shell, &delete_id(0));
    assert!(file.is_file(), "asked once: still there");
    assert_eq!(text_of(&shell, &delete_id(0)).as_deref(), Some("Delete it for good"));
    push(&mut shell, &delete_id(0));
    assert!(!file.is_file(), "asked again: gone");
    assert!(shell.new_game_screen().unwrap().saved().is_empty());
    assert!(!exists(&shell, &continue_id(0)) && !exists(&shell, &delete_id(0)));
    // A file that is no saved game is listed with why, and can only be
    // deleted; one that cannot be played on says why when asked.
    std::fs::write(dir.join("broken.save"), "nothing").unwrap();
    push(&mut shell, START_ID);
    let s = shell.session().unwrap();
    s.end_turn();
    s.ask_new_game();
    push(&mut shell, NEW_GAME_ID);
    let screen = shell.new_game_screen().unwrap();
    assert_eq!(screen.saved().len(), 2);
    assert!(screen.saved()[1].1.is_err(), "the broken one last");
    let play = shell.screen().ui.find(shell.screen().ui.root(), TbId::new(&continue_id(1))).unwrap();
    assert!(shell.screen().ui.node(play).is_some_and(|n| n.is_disabled()));
    // (The rules changed under a saved game: its first order is refused.)
    let text = std::fs::read_to_string(&file).unwrap();
    let steps = text.lines().position(|l| l.starts_with("steps ")).unwrap();
    let mut lines: Vec<&str> = text.lines().collect();
    lines[steps + 1] = "move red.0.0 0 0";
    std::fs::write(&file, lines.join("\n")).unwrap();
    push(&mut shell, &continue_id(0));
    let screen = shell.new_game_screen().expect("still the new game screen");
    let why = screen.resume_problem().expect("it says why");
    assert!(why.contains("saved under other rules, and cannot be played on"), "{why}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// 20 x 20 grass: red's city with two riders, blue's with nobody in it.
fn duel(mode: Mode) -> Game {
    let mut rows = vec!["g".repeat(20); 20];
    rows[5] = "gggggcgggcgggggggggg".to_string();
    let source = MapSource {
        name: "duel".to_string(),
        terrain: format!("20\n20\n{}\n", rows.join("\n")),
        units: Some(
            "unit riders\nname Riders\nimage units/lightcavalry.png\nstrength 4\nmoves 12\nenter grass 2, city 1\n"
                .to_string(),
        ),
        cities: Some(
            "city 5 5\nname Redhold\nside red\ngarrison riders 2\ncity 5 9\nname Bluemoor\nside blue\n".to_string(),
        ),
    };
    let game = Game::from_source(source, None).unwrap();
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup.mode = mode;
    game.with_setup(setup)
}

/// A game that is over is not kept: its file is taken away.
#[test]
fn a_game_that_is_over_is_kept_no_more() {
    let dir = saves("over");
    let mut s = common::with_game(&assets(), Size::new(1024, 768), 96, duel(Mode::Turns)).unwrap();
    let file = save::fresh(&dir, "duel");
    s.keep_in(file.clone());
    assert!(file.is_file());
    // Red's riders take Bluemoor, blue's last city: red wins.
    let p = common::pixel_of(&s, Hex::new(5, 5));
    click(&mut s, p);
    let p = common::pixel_of(&s, Hex::new(5, 9));
    click(&mut s, p);
    assert_eq!(s.screen.game.winner(), Some(Side::Red));
    s.pump();
    assert!(!file.is_file(), "over: not kept");
    assert_eq!(s.kept_in(), None);
    assert!(Scene::confirm_quit(&mut s) && !file.is_file());
    // (A map made by hand has no texts to keep: nothing is written.)
    let mut s = common::session();
    s.keep_in(file.clone());
    assert!(!file.is_file() && s.kept_in().is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Rounds: the window closed while the round plays out; played on, the
/// round goes on from where it was, and ends as it would have.
#[test]
fn a_round_playing_out_is_played_on_from_where_it_was() {
    let dir = saves("round");
    let mut s = common::with_game(&assets(), Size::new(1024, 768), 96, duel(Mode::Rounds)).unwrap();
    let file = save::fresh(&dir, "duel");
    s.keep_in(file.clone());
    let riders: Vec<UnitId> = s.screen.game.map.units_at(Hex::new(5, 5)).to_vec();
    s.screen.game.apply(Order::Move { units: riders.clone(), to: Hex::new(9, 5) }).unwrap();
    s.pump();
    s.end_turn();
    // The round begins to play out; a beat of it, and the window closes.
    let mut now = 1000;
    while !s.screen.game.playing_out() {
        now += 10;
        s.frame(now);
        assert!(now < 20_000, "the round begins to play out");
    }
    s.screen.game.play_out();
    assert_ne!(s.screen.game.map.units().get(riders[0]).unwrap().pos(), Hex::new(5, 5), "they have set off");
    assert!(s.screen.game.playing_out(), "in the middle of the round");
    assert!(Scene::confirm_quit(&mut s));
    let halfway = places(&s.screen.game);
    // The same game, played to the round's end.
    let mut whole = s.screen.game.clone();
    while whole.playing_out() {
        whole.play_out();
    }
    drop(s);
    let loaded = save::load_file(&file).unwrap();
    assert!(loaded.game.playing_out());
    assert_eq!(places(&loaded.game), halfway);
    let mut s = common::with_game(&assets(), Size::new(1024, 768), 96, loaded.game).unwrap();
    s.restore(loaded.steps, loaded.seen);
    s.keep_in(file.clone());
    while s.screen.game.playing_out() {
        now += 50;
        s.frame(now);
        s.pump();
        assert!(now < 60_000, "the round plays on to its end");
    }
    assert_eq!((places(&s.screen.game), s.screen.game.digest()), (places(&whole), whole.digest()));
    assert_eq!(s.screen.game.turn(), 2);
    // And kept on: what the file holds now is the game as it is.
    s.frame(now + 50);
    s.pump();
    assert_eq!(save::load_file(&file).unwrap().game.digest(), s.screen.game.digest());
    let _ = std::fs::remove_dir_all(&dir);
}

/// The screen showing how it was (the replay) changes nothing of what is
/// kept: the game as it is.
#[test]
fn the_game_as_it_is_is_kept_while_the_screen_shows_how_it_was() {
    let dir = saves("past");
    let mut shell = shell(&dir);
    push(&mut shell, START_ID);
    let s = shell.session().unwrap();
    s.end_turn();
    s.wait_for_turn();
    s.start_turn();
    s.pump();
    let now = s.screen.game.digest();
    s.go_back(0);
    assert!(s.screen.game.frozen(), "the screen shows how it was");
    assert_ne!(s.screen.game.digest(), now);
    assert!(shell.confirm_quit());
    let file = dir.join("continent-1.save");
    assert_eq!(save::load_file(&file).unwrap().game.digest(), now);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The new game screen says how big the map shown is and how many cities
/// it has: a map of the list, and a random map once it is made.
#[test]
fn the_screen_says_how_big_the_map_is_and_how_many_cities_it_has() {
    let mut shell = Shell::new_game(&assets(), Size::new(1024, 768), 96, None).unwrap();
    assert_eq!(text_of(&shell, MAP_FACTS_ID).as_deref(), Some("72 wide, 46 high: 9 cities, 4 of them a side's"));
    push(&mut shell, RANDOM_ID);
    assert_eq!(text_of(&shell, MAP_FACTS_ID), None, "not made yet: nothing to say");
    shell.new_game_screen().unwrap().random_config_mut().unwrap().seed = 5;
    push(&mut shell, MAKE_ID);
    let screen = shell.new_game_screen().unwrap();
    let map = &screen.screen.game.map;
    let (rows, cols, cities) = (map.rows(), map.cols(), map.cities().iter().count());
    assert!(cities >= 8, "{cities}");
    let says = text_of(&shell, MAP_FACTS_ID).expect("made: it says");
    assert_eq!(says, format!("{cols} wide, {rows} high: {cities} cities, 4 of them a side's"));
    // A random map's game is kept and played on like any other.
    let dir = saves("random");
    shell.keep_games_in(dir.clone());
    push(&mut shell, START_ID);
    let s = shell.session().unwrap();
    s.end_turn();
    s.wait_for_turn();
    s.start_turn();
    s.pump();
    let left = (s.screen.game.digest(), places(&s.screen.game));
    let setup = s.screen.game.setup().clone();
    assert_eq!(setup.people(), 1);
    assert!(shell.confirm_quit());
    let loaded = save::load_file(&dir.join("random-1.save")).unwrap();
    assert_eq!((loaded.game.digest(), places(&loaded.game)), left);
    assert_eq!((loaded.game.map.rows(), loaded.game.map.cols()), (rows, cols));
    assert_eq!(loaded.game.setup(), &setup);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Nowhere to keep games (a browser; a run with a script): nothing is
/// listed, nothing written, and leaving a game says it will be lost.
#[test]
fn with_nowhere_to_keep_them_no_game_is_kept() {
    let mut shell = Shell::new_game(&assets(), Size::new(1024, 768), 96, None).unwrap();
    assert!(!exists(&shell, &continue_id(0)));
    push(&mut shell, START_ID);
    let s = shell.session().unwrap();
    assert_eq!(s.kept_in(), None);
    s.end_turn();
    s.ask_new_game();
    let says = common::descendants(s, ASK_NEW_GAME_ID);
    let says: Vec<&str> = says.iter().filter_map(|w| s.screen.ui.text(*w)).collect();
    assert!(says.iter().any(|t| t.contains("it is not saved: it will be lost")), "{says:?}");
    assert!(shell.confirm_quit());
    let _ = widget;
}
