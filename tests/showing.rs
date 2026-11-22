//! Moves shown as they happen (the owner's word, 2026-11-21): "when units
//! move the player should see them moving and visibility opening up ...
//! This seeing movement should also apply to enemies, any units that you
//! can see ... the map should center on the action and show it ... Any
//! [battle] your side is in should open the combat dialog". An option, on
//! at first; the game's state changes at once, and only the screen shows
//! it going.

mod common;

use common::{assets, click, pixel_of, push_widget};
use hexlords::battle_window::BATTLE_CLOSE_ID;
use hexlords::game::Game;
use hexlords::options_window::Options;
use hexlords::orders::{Event, Order};
use hexlords::screen::Scene;
use hexlords::session::{BEGIN_TURN_ID, END_TURN_ID, GAME_OVER_ID, Session};
use hexlords::setup::{Fog, Setup};
use hexlords::showing::{STEP_MS, WAY_MS};
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{Hex, Side, UnitId};
use turbo_ferret::Size;

const REDHOLD: Hex = Hex::new(5, 5);

/// 30 x 30 grass: red's Redhold (a levy in it) and Redfar, blue's
/// Bluemoor; soldiers go 6 hexes a turn.
fn game(fog: Fog, blue: &str, place: impl FnOnce(&mut Game)) -> Game {
    let mut rows = vec!["g".repeat(30); 30];
    rows[5] = format!("{}c{}c{}", "g".repeat(5), "g".repeat(14), "g".repeat(9));
    rows[25] = format!("{}c{}", "g".repeat(25), "g".repeat(4));
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\ngarrison levy\n\
                  city 25 25\nname Redfar\nside red\n\
                  city 5 20\nname Bluemoor\nside blue\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
    place(&mut game);
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup.set_players(&format!("blue={blue}")).unwrap();
    (setup.fog, setup.combat.chance) = (fog, 0);
    game.with_setup(setup)
}

fn put(game: &mut Game, side: Side, kind: &str, hex: Hex) -> UnitId {
    let kind = game.map.units().kind_named(kind).unwrap();
    let id = game.map.new_unit_of(side, kind).unwrap();
    game.map.place_unit(id, hex).unwrap();
    id
}

fn session(game: Game) -> Session {
    let options = Options { show_moves: true, ..Options::manual() };
    Session::with_options(&assets(), Size::new(1024, 768), 96, game, options).expect("the session starts")
}

fn click_hex(s: &mut Session, hex: Hex) {
    s.screen.view.center_on(hex);
    let p = pixel_of(s, hex);
    click(s, p);
}

/// The units shown going, and where (the hex they are nearest).
fn going(s: &Session) -> Vec<(Vec<UnitId>, Hex)> {
    Scene::moving(s).into_iter().map(|m| (m.units, m.hex)).collect()
}

#[test]
fn a_stack_moved_is_shown_going_hex_by_hex_while_the_game_has_it_there() {
    let mut a = None;
    let mut s = session(game(Fog::Off, "passive", |g| a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5)))));
    let a = a.unwrap();
    click_hex(&mut s, Hex::new(10, 5));
    let to = Hex::new(10, 8);
    click_hex(&mut s, to);
    assert_eq!(s.screen.game.map.units().get(a).unwrap().pos(), to, "the game has it there at once");
    s.frame(1000);
    assert_eq!(going(&s), [(vec![a], Hex::new(10, 5))], "it sets off");
    s.frame(1000 + STEP_MS + STEP_MS / 2 + 1);
    let shown = going(&s);
    assert!(shown.len() == 1 && shown[0].1 != Hex::new(10, 5) && shown[0].1 != to, "on its way: {shown:?}");
    s.frame(1000 + WAY_MS + 1);
    assert!(going(&s).is_empty(), "there");
}

#[test]
fn the_fog_opens_as_the_stack_goes() {
    let mut s = session(game(Fog::On, "passive", |g| {
        put(g, Side::Red, "soldiers", Hex::new(10, 5));
    }));
    let beyond = Hex::new(10, 13);
    click_hex(&mut s, Hex::new(10, 5));
    click_hex(&mut s, Hex::new(10, 11));
    assert!(s.sight().sees(beyond), "the side sees it now");
    s.frame(1000);
    assert!(!Scene::sight(&s).sees(beyond), "but it is not shown yet");
    s.frame(1000 + WAY_MS + 1);
    assert!(Scene::sight(&s).sees(beyond), "once the stack is there");
}

#[test]
fn the_computer_plays_an_order_at_a_time_its_stacks_shown_going() {
    let mut s = session(game(Fog::Off, "simple", |g| {
        put(g, Side::Blue, "soldiers", Hex::new(12, 20));
        put(g, Side::Blue, "soldiers", Hex::new(12, 20));
    }));
    s.end_turn();
    let mut seen_going = false;
    let mut t = 0;
    while s.screen.game.player() == Side::Blue && t < 120_000 {
        t += 50;
        s.frame(t);
        let blue = going(&s).iter().any(|(units, _)| units.iter().all(|u| u.side == Side::Blue));
        seen_going |= blue;
    }
    assert!(seen_going, "blue's stack was shown going in blue's turn");
    assert_eq!(s.screen.game.player(), Side::Red, "and red's turn came");
    assert!(t > STEP_MS, "not at once");
}

#[test]
fn a_battle_the_computer_fights_against_the_person_is_played_out_before_it_goes_on() {
    let mut s = session(game(Fog::Off, "simple", |g| {
        for _ in 0..3 {
            put(g, Side::Blue, "soldiers", Hex::new(5, 9));
        }
    }));
    s.end_turn();
    let mut t = 0;
    while s.battle().is_none() && t < 60_000 {
        t += 50;
        s.frame(t);
    }
    let battle = s.battle().expect("the battle's window opened");
    assert!(!battle.done(), "it is played out");
    assert_eq!(s.screen.game.player(), Side::Blue, "in blue's turn, which waits for it");
    let fought = s.screen.game.history().iter().any(|r| matches!(r.event, Event::Battle { at: REDHOLD, .. }));
    assert!(fought);
    while s.battle().is_some_and(|b| !b.done()) && t < 120_000 {
        t += 50;
        s.frame(t);
    }
    assert_eq!(s.screen.game.player(), Side::Blue, "a moment after it, still");
    for _ in 0..200 {
        t += 50;
        s.frame(t);
    }
    assert_eq!(s.screen.game.player(), Side::Red, "then on");
}

/// Without a window nothing is kept for showing: the headless games and
/// the agents pay nothing for it.
#[test]
fn a_game_keeps_what_happens_only_when_watched() {
    let mut a = None;
    let mut game = game(Fog::Off, "passive", |g| a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5))));
    let a = a.unwrap();
    game.apply(Order::Move { units: vec![a], to: Hex::new(10, 6) }).unwrap();
    assert!(game.take_happened().is_empty());
    game.watch(true);
    game.apply(Order::Move { units: vec![a], to: Hex::new(10, 7) }).unwrap();
    let happened = game.take_happened();
    let way = vec![Hex::new(10, 6), Hex::new(10, 7)];
    assert!(matches!(&happened[..], [(None, Event::Moved { way: w, .. })] if *w == way), "{happened:?}");
    assert!(game.view_for(Side::Red).take_happened().is_empty(), "nor does an agent's view");
}

/// 30 x 30 grass: red's Redhold and Greenwood's and Bluemoor's levies;
/// `players` says who plays each side.
fn three_sides(players: &str, place: impl FnOnce(&mut Game)) -> Game {
    let mut rows = vec!["g".repeat(30); 30];
    rows[5] = format!("{}c{}c{}", "g".repeat(5), "g".repeat(14), "g".repeat(9));
    rows[25] = format!("{}c{}", "g".repeat(25), "g".repeat(4));
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\ngarrison levy\n\
                  city 25 25\nname Greenwood\nside green\ngarrison levy\n\
                  city 5 20\nname Bluemoor\nside blue\ngarrison levy\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
    place(&mut game);
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup.set_players(players).unwrap();
    setup.combat.chance = 0;
    game.with_setup(setup)
}

/// The review (2026-11-21): in hotseat, the next person's turn began, with
/// no cover over it, while the computer's last moves were still being
/// shown: the map showed it as that side knows it, and the person at the
/// screen could end its turn. The screen stays the last person's until the
/// turn is handed over.
#[test]
fn a_turn_is_handed_over_once_the_one_before_has_been_shown() {
    let mut s = session(three_sides("red=person,green=passive,blue=person", |g| {
        let id = put(g, Side::Green, "soldiers", Hex::new(15, 15));
        g.map.unit_mut(id).unwrap().marauding = true;
    }));
    // (In hotseat the first person is handed the game too.)
    push_widget(&mut s, BEGIN_TURN_ID);
    assert!(!s.covered());
    s.end_turn();
    let (mut t, mut held) = (0, false);
    while !s.covered() && t < 20_000 {
        t += 16;
        s.frame(t);
        if s.screen.game.player() == Side::Blue && !s.covered() {
            held = true;
            assert_eq!(s.seer(), Side::Red, "still the last person's screen");
            push_widget(&mut s, END_TURN_ID);
            push_widget(&mut s, END_TURN_ID);
            assert_eq!(s.screen.game.player(), Side::Blue, "blue's turn is not the screen's to end yet");
        }
    }
    assert!(held, "green's marauders were shown going in blue's turn");
    assert!(s.covered() && s.screen.game.player() == Side::Blue, "then blue's turn, covered");
}

/// The review (2026-11-21): played an order at a time, the computer's last
/// order could be taken back by the person's Undo.
#[test]
fn undo_takes_back_nothing_in_the_computers_turn() {
    let mut blue = Vec::new();
    let mut s = session(three_sides("red=person,green=passive,blue=simple", |g| {
        blue.push(put(g, Side::Blue, "soldiers", Hex::new(12, 20)));
        blue.push(put(g, Side::Blue, "soldiers", Hex::new(12, 20)));
    }));
    s.end_turn();
    let (mut t, mut tried) = (0, false);
    while s.screen.game.player() != Side::Red && t < 60_000 {
        t += 16;
        s.frame(t);
        let game = &s.screen.game;
        if game.player() == Side::Blue && game.can_undo() {
            let at = |s: &Session| -> Vec<Hex> {
                blue.iter().filter_map(|id| s.screen.game.map.units().get(*id)).map(|u| u.pos()).collect()
            };
            let before = at(&s);
            s.undo();
            assert_eq!(at(&s), before, "blue's move stands");
            tried = true;
        }
    }
    assert!(tried, "blue moved");
}

/// The review (2026-11-21): the battle that put the person out (and ended
/// the game) was never played out: they were out, so nobody's battles were.
#[test]
fn the_battle_that_puts_the_person_out_is_played_out() {
    let mut rows = vec!["g".repeat(30); 30];
    rows[5] = format!("{}c{}c{}", "g".repeat(5), "g".repeat(14), "g".repeat(9));
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n";
    let cities =
        "city 5 5\nname Redhold\nside red\ngarrison levy\ncity 5 20\nname Bluemoor\nside blue\ngarrison levy\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
    for _ in 0..3 {
        put(&mut game, Side::Blue, "soldiers", Hex::new(5, 8));
    }
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup.set_players("blue=simple").unwrap();
    (setup.fog, setup.combat.chance) = (Fog::On, 0);
    let mut s = session(game.with_setup(setup));
    s.end_turn();
    let (mut t, mut seen) = (0, false);
    let over = |s: &Session| s.screen.ui.find(s.screen.ui.root(), turbo_ferret::TbId::new(GAME_OVER_ID)).is_some();
    while t < 60_000 && !over(&s) {
        t += 16;
        s.frame(t);
        seen |= s.battle().is_some();
    }
    assert_eq!(s.screen.game.winner(), Some(Side::Blue));
    assert!(seen, "the battle was played out before the game's end was said");
}

/// The review (2026-11-21): with moves not shown, the battles against the
/// person still to be played out were dropped by the person's first order.
#[test]
fn an_order_of_the_persons_leaves_the_battles_still_to_be_played_out() {
    // Red's Redhold and Redfar each with a levy, blue's soldiers beside
    // each; Redrest, out of reach, and a red unit far off.
    let mut rows = vec!["g".repeat(30); 30];
    rows[5] = format!("{}c{}c{}", "g".repeat(5), "g".repeat(14), "g".repeat(9));
    rows[15] = format!("{}c{}", "g".repeat(2), "g".repeat(27));
    rows[25] = format!("{}c{}", "g".repeat(25), "g".repeat(4));
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\ngarrison levy\ncity 25 25\nname Redfar\nside red\ngarrison levy\n\
                  city 15 2\nname Redrest\nside red\ncity 5 20\nname Bluemoor\nside blue\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
    for _ in 0..3 {
        put(&mut game, Side::Blue, "soldiers", Hex::new(5, 8));
        put(&mut game, Side::Blue, "soldiers", Hex::new(25, 22));
    }
    let red = put(&mut game, Side::Red, "soldiers", Hex::new(0, 29));
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup.set_players("blue=simple").unwrap();
    setup.combat.chance = 0;
    let game = game.with_setup(setup);
    let options = Options { show_moves: false, ..Options::manual() };
    let mut s = Session::with_options(&assets(), Size::new(1024, 768), 96, game, options).unwrap();
    let before = s.screen.game.history().len();
    s.end_turn();
    let mut t = 0;
    while s.battle().is_none() && t < 20_000 {
        t += 16;
        s.frame(t);
    }
    let history = s.screen.game.history();
    let battles = (before..history.len()).filter(|i| matches!(history[*i].event, Event::Battle { .. })).count();
    assert!(battles >= 2, "two battles against red ({battles})");
    push_widget(&mut s, BATTLE_CLOSE_ID);
    s.start_turn();
    s.screen.game.apply(Order::Move { units: vec![red], to: Hex::new(1, 29) }).unwrap();
    let mut again = false;
    for _ in 0..200 {
        t += 16;
        s.frame(t);
        again |= s.battle().is_some();
    }
    assert!(again, "the second battle is played out");
}
