//! The game's history and the log made of it: what is kept (battles, cities
//! taken, sides out, not the moves), what an undo takes out of it, what it
//! says to each side; and on the screen, the Log window and the news at the
//! start of a person's turn.

mod common;

use common::{assets, choose, click, pixel_of, press, type_char, widget};
use hexgame::game::Game;
use hexgame::info::{log, record_lines};
use hexgame::orders::{Event, Order, Record};
use hexgame::session::{BEGIN_TURN_ID, END_TURN_ID, LOG_ID, MENU_VIEW_ID, Session};
use hexgame::setup::Setup;
use hexgame::state::scenario::{self, Scenario};
use hexgame::state::{Hex, Side, UnitId};
use turbo_ferret::{Point, Size, SpecialKey};

const REDHOLD: Hex = Hex::new(5, 5);
const SOUTHWICK: Hex = Hex::new(15, 5);
const BLUEMOOR: Hex = Hex::new(5, 9);

/// Red's Redhold with two levies and Southwick with nobody; blue's Bluemoor
/// with four soldiers. No chance in battles; red a person, blue passive,
/// unless `setup` says otherwise.
fn game(setup: impl FnOnce(&mut Setup)) -> Game {
    let mut rows = vec!["g".repeat(20); 20];
    rows[5] = "gggggcgggcgggggggggg".to_string();
    rows[15] = "gggggcgggggggggggggg".to_string();
    let terrain = format!("20\n20\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\ngarrison levy 2\n\
                  city 15 5\nname Southwick\nside red\n\
                  city 5 9\nname Bluemoor\nside blue\ngarrison soldiers 4\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut s = Setup::solo(&game.map, Some(Side::Red), "passive");
    s.combat.chance = 0;
    setup(&mut s);
    game.with_setup(s)
}

fn at(game: &Game, hex: Hex) -> Vec<UnitId> {
    game.map.units_at(hex).to_vec()
}

#[test]
fn the_history_keeps_battles_and_cities_and_not_moves() {
    let mut game = game(|_| {});
    let levies = at(&game, REDHOLD);
    game.apply(Order::Move { units: vec![levies[0]], to: Hex::new(5, 6) }).unwrap();
    assert!(game.history().is_empty(), "a move is not kept");
    // A levy against four soldiers: it dies.
    game.apply(Order::Move { units: vec![levies[1]], to: BLUEMOOR }).unwrap();
    let [Record { round: 1, turn: Side::Red, event: Event::Battle { won: false, dead, .. } }] = game.history() else {
        panic!("{:?}", game.history())
    };
    let fallen = hexgame::orders::Fallen { side: Side::Red, name: "Levy".to_string(), person: false };
    assert_eq!(dead, &[fallen], "who died, by name");
    // Blue's turn: it takes empty Southwick, and the record says it was
    // blue's turn in round 1.
    game.end_turn();
    let soldiers = at(&game, BLUEMOOR);
    game.map.move_unit(soldiers[0], Hex::new(13, 5)).unwrap();
    game.apply(Order::Move { units: vec![soldiers[0]], to: SOUTHWICK }).unwrap();
    let last = game.history().last().unwrap();
    assert_eq!((last.round, last.turn), (1, Side::Blue));
    assert!(matches!(last.event, Event::Taken { from: Side::Red, by: Side::Blue, .. }));
    // Undone, it was never there.
    let before = game.history().len();
    game.apply(Order::Undo).unwrap();
    assert_eq!(game.history().len(), before - 1);
}

#[test]
fn each_side_reads_what_is_about_it() {
    let mut game = game(|_| {});
    let levy = at(&game, REDHOLD)[1];
    game.apply(Order::Move { units: vec![levy], to: BLUEMOOR }).unwrap();
    let record = &game.history()[0];
    assert_eq!(
        record_lines(&game.map, record, Side::Red),
        ["You attacked blue at Bluemoor, 1 against 4, and lost.", "You lost 1 Levy; they lost nothing."]
    );
    assert_eq!(
        record_lines(&game.map, record, Side::Blue),
        ["Red attacked you at Bluemoor, 1 against 4: you held.", "You lost nothing; they lost 1 Levy."]
    );
    assert!(record_lines(&game.map, record, Side::Green).is_empty(), "not green's battle");
    // The log: a round at a time, the latest first.
    game.end_turn();
    game.end_turn();
    let last = at(&game, REDHOLD)[0];
    game.map.move_unit(last, Hex::new(5, 7)).unwrap();
    game.apply(Order::Move { units: vec![last], to: BLUEMOOR }).unwrap();
    let rounds: Vec<u32> = log(&game, Side::Red, 0).iter().map(|(round, _)| *round).collect();
    assert_eq!(rounds, [2, 1]);
    assert!(log(&game, Side::Green, 0).is_empty());
}

// The screen.

fn session(setup: impl FnOnce(&mut Setup)) -> Session {
    common::with_game(&assets(), Size::new(1024, 768), 96, game(setup)).unwrap()
}

fn push(s: &mut Session, name: &str) {
    let r = s.screen.area(widget(s, name));
    click(s, Point::new(r.x + r.w / 2, r.y + r.h / 2));
}

#[test]
fn the_log_has_the_battles_you_fought_on_both_sides() {
    let mut s = session(|s| s.set_players("blue=simple").unwrap());
    // Nothing yet.
    choose(&mut s, MENU_VIEW_ID, LOG_ID);
    assert_eq!(s.log_lines(), ["Nothing yet."]);
    // Red sends a levy against Bluemoor, with the log open (over the map: by
    // the keys, then): it says so at once, and the report opens over it.
    let levies = at(&s.screen.game, REDHOLD);
    s.screen.game.select_unit(levies[1]);
    s.screen.game.set_cursor(BLUEMOOR);
    press(&mut s, SpecialKey::Enter);
    assert_eq!(
        s.log_lines(),
        ["Round 1", "You attacked blue at Bluemoor, 1 against 4, and lost.", "    You lost 1 Levy; they lost nothing."]
    );
    // Esc: the battle's window first, then the log.
    press(&mut s, SpecialKey::Esc);
    assert!(s.battle().is_none() && !s.log_lines().is_empty());
    press(&mut s, SpecialKey::Esc);
    assert!(s.log_lines().is_empty());
    // End turn: the button waits for blue (the log stays shut).
    let button = |s: &Session| s.screen.ui.text(widget(s, END_TURN_ID)).unwrap_or("").to_string();
    push(&mut s, END_TURN_ID);
    assert!(s.log_lines().is_empty());
    assert_eq!(button(&s), "Waiting for blue");
    // A frame: blue plays. It attacks Redhold's last levy and takes the
    // city: red's turn starts with its log open, and it has that, new
    // (red's own attack, before it ended its turn, is not).
    s.frame(0);
    let new = s.log_new_lines();
    assert!(
        new.iter().any(|l| l.starts_with("Blue attacked you at Redhold") && l.ends_with("you were beaten.")),
        "{new:?}"
    );
    assert!(new.contains(&"Blue took Redhold from you.".to_string()), "{new:?}");
    assert!(!new.iter().any(|l| l.starts_with("You attacked")), "{new:?}");
    // Red's turn: the button starts it, and the log closes.
    assert_eq!(s.log_status_line(), "Your turn: press Start turn when you are ready.");
    assert_eq!(button(&s), "Start turn");
    push(&mut s, END_TURN_ID);
    assert!(s.log_lines().is_empty());
    assert_eq!(button(&s), "End turn (1)", "what red has left waits");
    // L opens it again: all of round 1, in order.
    type_char(&mut s, 'l');
    let lines = s.log_lines();
    assert_eq!(lines[0], "Round 1", "{lines:?}");
    assert_eq!(lines[1], "You attacked blue at Bluemoor, 1 against 4, and lost.", "in the order they happened");
    assert!(lines[3].starts_with("Blue attacked you at Redhold"), "{lines:?}");
    assert_eq!(lines.last().map(String::as_str), Some("Blue took Redhold from you."));
    // L again closes it.
    type_char(&mut s, 'l');
    assert!(s.log_lines().is_empty());
}

#[test]
fn the_next_person_does_not_see_the_last_ones_log() {
    let mut s = session(|s| s.set_players("blue=person").unwrap());
    push(&mut s, BEGIN_TURN_ID);
    let p = pixel_of(&s, REDHOLD);
    click(&mut s, p);
    let p = pixel_of(&s, BLUEMOOR);
    click(&mut s, p);
    press(&mut s, SpecialKey::Esc);
    choose(&mut s, MENU_VIEW_ID, LOG_ID);
    assert!(s.log_lines()[1].starts_with("You attacked blue"));
    // The cover closes it; blue opens its own, which tells it from blue's side.
    s.end_turn();
    assert!(s.covered() && s.log_lines().is_empty());
    push(&mut s, BEGIN_TURN_ID);
    press(&mut s, SpecialKey::Esc);
    choose(&mut s, MENU_VIEW_ID, LOG_ID);
    let lines = s.log_lines();
    assert!(lines[1].starts_with("Red attacked you at Bluemoor") && lines[1].ends_with("you held."), "{lines:?}");
}

#[test]
fn when_the_last_person_is_out_the_computer_plays_on_a_turn_a_frame() {
    // Red's one city and its levy; blue's four soldiers next door; green far
    // off, still in the game.
    let mut rows = vec!["g".repeat(20); 20];
    rows[5] = "gggggcgggcgggggggggg".to_string();
    rows[18] = "ggggggggggggggggggcg".to_string();
    let terrain = format!("20\n20\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\ngarrison levy\n\
                  city 5 9\nname Bluemoor\nside blue\ngarrison soldiers 4\n\
                  city 18 18\nname Greenford\nside green\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup.combat.chance = 0;
    setup.set_players("blue=simple").unwrap();
    let mut s = common::with_game(&assets(), Size::new(1024, 768), 96, game.with_setup(setup)).unwrap();
    let end_turn = |s: &Session| s.screen.ui.state(widget(s, hexgame::session::END_TURN_ID));
    assert!(!end_turn(&s).contains(turbo_ferret::WidgetState::DISABLED));
    // Green's turn, then blue's: blue takes Redhold, red is out, and its log
    // says so; the computer does not play the rest of the game at once.
    s.end_turn();
    s.wait_for_turn();
    assert!(s.screen.game.out().contains(&Side::Red));
    assert_eq!(s.screen.game.winner(), None);
    assert!(s.log_new_lines().contains(&"You are out of the game.".to_string()), "{:?}", s.log_new_lines());
    // The turn is no longer the screen's to end; the frames play one each,
    // a little apart, and the log stays red's.
    assert!(end_turn(&s).contains(turbo_ferret::WidgetState::DISABLED));
    let turn = |s: &Session| (s.screen.game.player(), s.screen.game.turn());
    let now = turn(&s);
    s.end_turn();
    assert_eq!(turn(&s), now);
    s.frame(16);
    let next = turn(&s);
    assert_ne!(next, now, "a frame plays a turn");
    s.frame(32);
    assert_eq!(turn(&s), next, "but not the next one so soon");
    s.frame(16 + hexgame::session::COMPUTER_TURN_MS);
    assert_ne!(turn(&s), next);
    assert!(s.log_lines().iter().any(|l| l == "You are out of the game."), "{:?}", s.log_lines());
}
