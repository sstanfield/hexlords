//! The new game screen, the game it starts, and the window's way from one to
//! the other: the maps, the mode, the order, who plays; hotseat's cover
//! between two people's turns; the computer's turns; the end, and a new game.
//! No window and no GPU: the shell on the real assets.

mod common;

use common::{assets, click, widget};
use hexgame::game::Game;
use hexgame::new_game::{
    BATTLES_STRENGTH_ID, CHANCE_LESS_ID, CHANCE_MORE_ID, MODE_ROUNDS_ID, ORDER_DRAWN_ID, START_ID, map_id, map_name,
    seat_id,
};
use hexgame::screen::Scene;
use hexgame::session::{BEGIN_TURN_ID, COVER_ID, END_TURN_ID, GAME_OVER_ID, NEW_GAME_ID, Session};
use hexgame::setup::{Controller, Setup, TurnOrder};
use hexgame::shell::Shell;
use hexgame::state::combat::{Rules, System};
use hexgame::state::scenario::{self, Scenario};
use hexgame::state::{Hex, Side};
use turbo_ferret::{Point, Size, TbId};

fn shell() -> Shell {
    Shell::new_game(&assets(), Size::new(1024, 768), 96, None).unwrap()
}

fn exists(s: &dyn Scene, name: &str) -> bool {
    s.screen().ui.find(s.screen().ui.root(), TbId::new(name)).is_some()
}

/// Click the middle of a widget of whatever the shell shows.
fn push(shell: &mut Shell, name: &str) {
    let id = shell.screen().ui.find(shell.screen().ui.root(), TbId::new(name)).unwrap_or_else(|| panic!("no {name}"));
    let r = shell.screen().area(id);
    let p = Point::new(r.x + r.w / 2, r.y + r.h / 2);
    let ui = &mut shell.screen_mut().ui;
    ui.pointer_move(p, turbo_ferret::Modifiers::NONE);
    ui.pointer_down(p, 1, turbo_ferret::Modifiers::NONE);
    ui.pointer_up(p, turbo_ferret::Modifiers::NONE);
    shell.pump();
}

#[test]
fn the_screen_offers_the_maps_and_sets_the_game_up() {
    let mut shell = shell();
    let screen = shell.new_game_screen().expect("the new game screen first");
    let names: Vec<String> = screen.maps().iter().map(|m| map_name(m)).collect();
    assert!(names.contains(&"continent".to_string()) && names.contains(&"example".to_string()), "{names:?}");
    // The continent at first: its four sides, the first a person's.
    assert_eq!(screen.chosen().map(map_name).as_deref(), Some("continent"));
    let seats: Vec<(Side, bool)> =
        screen.setup().seats.iter().map(|s| (s.side, s.controller == Controller::Person)).collect();
    assert_eq!(seats, [(Side::Red, true), (Side::Green, false), (Side::Blue, false), (Side::Yellow, false)]);
    // The map shows, to be looked over.
    assert_eq!((screen.screen.game.map.rows(), screen.screen.game.map.cols()), (46, 72));
    // Green to a person, the order drawn; rounds are not to be had yet.
    push(&mut shell, &seat_id(Side::Green, true));
    push(&mut shell, ORDER_DRAWN_ID);
    push(&mut shell, MODE_ROUNDS_ID);
    let screen = shell.new_game_screen().unwrap();
    assert_eq!(screen.setup().controller(Side::Green), Some(&Controller::Person));
    assert_eq!(screen.setup().order, TurnOrder::Drawn);
    assert_eq!(screen.setup().mode, hexgame::setup::Mode::Turns);
    // Another map: the example is terrain alone, with no sides: nothing to
    // play, and the game does not start.
    let example = screen.maps().iter().position(|m| map_name(m) == "example").unwrap();
    push(&mut shell, &map_id(example));
    let screen = shell.new_game_screen().unwrap();
    assert_eq!(screen.chosen().map(map_name).as_deref(), Some("example"));
    assert!(screen.setup().seats.is_empty() && !screen.can_start());
    assert_eq!(screen.setup().order, TurnOrder::Drawn, "what was chosen stays");
    push(&mut shell, START_ID);
    assert!(shell.new_game_screen().is_some(), "still the new game screen");
}

#[test]
fn start_hands_the_window_to_the_game_set_up_as_chosen() {
    let mut shell = shell();
    push(&mut shell, &seat_id(Side::Blue, true));
    push(&mut shell, START_ID);
    let session = shell.session().expect("the game");
    let game = &session.screen.game;
    assert_eq!((game.map.rows(), game.map.cols()), (46, 72));
    assert_eq!(game.controller(Side::Red), Some(&Controller::Person));
    assert_eq!(game.controller(Side::Blue), Some(&Controller::Person));
    assert_eq!(game.controller(Side::Green), Some(&Controller::Computer("simple".to_string())));
    // Red plays first, and the button says so.
    assert_eq!(game.player(), Side::Red);
    assert_eq!(session.screen.ui.text(widget(session, END_TURN_ID)), Some("End red's turn"));
    assert!(exists(session, "main_map") && !exists(session, START_ID));
}

#[test]
fn the_battles_are_set_up_on_the_screen_and_carried_into_the_game() {
    let mut shell = shell();
    assert_eq!(shell.new_game_screen().unwrap().setup().combat, Rules::default());
    assert_eq!(Rules::default(), Rules { chance: 5, system: System::Lines });
    // Strength against strength, and less chance; never below none.
    push(&mut shell, BATTLES_STRENGTH_ID);
    for _ in 0..3 {
        push(&mut shell, CHANCE_LESS_ID);
    }
    assert_eq!(shell.new_game_screen().unwrap().setup().combat, Rules { chance: 2, system: System::Strength });
    for _ in 0..3 {
        push(&mut shell, CHANCE_LESS_ID);
    }
    assert_eq!(shell.new_game_screen().unwrap().setup().combat.chance, 0);
    // The screen says how much, and stops at all chance.
    let says = descendants_of(&shell, "new_game_settings");
    assert!(says.iter().any(|t| t == "Chance 0 of 10"), "{says:?}");
    for _ in 0..12 {
        push(&mut shell, CHANCE_MORE_ID);
    }
    assert_eq!(shell.new_game_screen().unwrap().setup().combat.chance, 10);
    push(&mut shell, START_ID);
    let game = &shell.session().unwrap().screen.game;
    assert_eq!(game.setup().combat, Rules { chance: 10, system: System::Strength });
}

fn descendants_of(shell: &Shell, name: &str) -> Vec<String> {
    let ui = &shell.screen().ui;
    let mut out = Vec::new();
    let mut stack = vec![ui.find(ui.root(), TbId::new(name)).unwrap_or(ui.root())];
    while let Some(id) = stack.pop() {
        if let Some(text) = ui.text(id) {
            out.push(text.to_string());
        }
        stack.extend(ui.children(id).iter().copied());
    }
    out
}

/// A map with red's and blue's cities, a rider in red's, blue's with nobody in
/// it, and nobody's: played as `setup` says.
fn duel(setup: impl FnOnce(&mut Setup)) -> Session {
    let mut rows = vec!["g".repeat(20); 20];
    rows[5] = "gggggcgggcgggggggggg".to_string();
    let terrain = format!("20\n20\n{}\n", rows.join("\n"));
    let units = "unit riders\nname Riders\nimage units/riders.png\nstrength 4\nmoves 12\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\ngarrison riders 2\ncity 5 9\nname Bluemoor\nside blue\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut s = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup(&mut s);
    Session::with_game(&assets(), Size::new(1024, 768), 96, game.with_setup(s)).unwrap()
}

#[test]
fn between_two_peoples_turns_the_map_is_covered() {
    let mut s = duel(|s| s.set_players("blue=person").unwrap());
    // The first person too: the game is handed to them.
    assert!(s.covered());
    let begin = |s: &mut Session| {
        let r = s.screen.area(widget(s, BEGIN_TURN_ID));
        click(s, Point::new(r.x + r.w / 2, r.y + r.h / 2));
    };
    begin(&mut s);
    assert!(!s.covered());
    // The turn cannot end while the map is covered: only red's button ends it.
    s.end_turn();
    assert!(s.covered() && exists(&s, COVER_ID));
    assert_eq!(s.screen.game.player(), Side::Blue);
    // The cover hides the whole window, and a click on the map does nothing.
    let cover = s.screen.area(widget(&s, COVER_ID));
    assert_eq!((cover.x, cover.y, cover.w, cover.h), (0, 0, 1024, 768));
    let cursor = s.screen.game.map.cursor;
    let p = common::pixel_of(&s, Hex::new(10, 10));
    click(&mut s, p);
    assert_eq!(s.screen.game.map.cursor, cursor);
    // Blue says they are there.
    begin(&mut s);
    assert!(!s.covered() && !exists(&s, COVER_ID));
    assert_eq!(s.screen.ui.text(widget(&s, END_TURN_ID)), Some("End blue's turn"));
    // Red's units are not blue's to move.
    let p = common::pixel_of(&s, Hex::new(5, 5));
    click(&mut s, p);
    assert_eq!(s.screen.game.selected(), None);
}

#[test]
fn the_computer_plays_its_turn_between_the_persons() {
    let mut s = duel(|s| s.set_players("blue=simple").unwrap());
    s.end_turn();
    // Blue played (and passed: it has nothing), and it is red's turn again.
    assert_eq!((s.screen.game.player(), s.screen.game.turn()), (Side::Red, 2));
    assert!(!s.covered(), "one person: no cover");
}

#[test]
fn the_end_says_who_won_and_offers_a_new_game() {
    let session = duel(|_| {});
    let mut shell = Shell::playing(&assets(), session);
    // Red's rider takes Bluemoor, blue's last city: red wins.
    let s = shell.session().unwrap();
    let p = common::pixel_of(s, Hex::new(5, 5));
    click(s, p);
    let p = common::pixel_of(s, Hex::new(5, 9));
    click(s, p);
    assert_eq!(s.screen.game.winner(), Some(Side::Red));
    assert!(exists(s, GAME_OVER_ID));
    let says: Vec<String> = descendants(s, GAME_OVER_ID);
    assert!(says.iter().any(|t| t == "red wins, in round 1."), "{says:?}");
    // The turn ends no more.
    s.end_turn();
    assert_eq!(s.screen.game.turn(), 1);
    // A new game: the new game screen again.
    push(&mut shell, NEW_GAME_ID);
    assert!(shell.new_game_screen().is_some());
}

fn descendants(s: &Session, name: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![widget(s, name)];
    while let Some(id) = stack.pop() {
        if let Some(text) = s.screen.ui.text(id) {
            out.push(text.to_string());
        }
        stack.extend(s.screen.ui.children(id).iter().copied());
    }
    out
}
