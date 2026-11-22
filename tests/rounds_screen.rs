//! Rounds at the screen (`docs/gameplay.md`, "Rounds"; the owner's word,
//! 2026-11-22): a click plans, the map shows the person's plans, "Orders
//! done" passes to the next side, and once every side has done the round
//! plays out before the person's eyes, a tick at a time, every stack of a
//! tick at the same moment; in hotseat each person plans behind the cover,
//! and is shown what happened.

mod common;

use common::{assets, click, pixel_of, push_widget, widget};
use hexlords::battle_window::BATTLE_CLOSE_ID;
use hexlords::game::Game;
use hexlords::options_window::Options;
use hexlords::screen::Scene;
use hexlords::session::{BEGIN_TURN_ID, END_TURN_ID, Session, UNDO_ID};
use hexlords::setup::{Fog, Mode, Setup};
use hexlords::showing::STEP_MS;
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{Hex, Side, UnitId};
use turbo_ferret::Size;

/// 30 x 30 grass: red's Redhold, blue's Bluemoor; in rounds, no chance in
/// battle; red a person, blue `blue` ("person": hotseat). Soldiers
/// (strength 5) and levies (strength 1) go 6 hexes a round.
fn game(blue: &str, place: impl FnOnce(&mut Game)) -> Game {
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
    setup.set_players(&format!("blue={blue}")).unwrap();
    (setup.mode, setup.fog, setup.combat.chance) = (Mode::Rounds, Fog::Off, 0);
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

fn pos(s: &Session, id: UnitId) -> Hex {
    s.screen.game.map.units().get(id).unwrap().pos()
}

fn button(s: &Session) -> String {
    s.screen.ui.text(widget(s, END_TURN_ID)).unwrap_or("").to_string()
}

/// Frames go by, `step` ms apart from `from`, until `done` or `until`.
fn frames(s: &mut Session, from: u64, until: u64, step: u64, mut done: impl FnMut(&Session) -> bool) -> u64 {
    let mut t = from;
    while t < until && !done(s) {
        t += step;
        s.frame(t);
    }
    t
}

#[test]
fn a_click_plans_and_the_map_shows_the_plan() {
    let mut a = None;
    let mut s = session(game("passive", |g| a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5)))));
    let a = a.unwrap();
    assert_eq!(button(&s), "Orders done (2)", "a stack waits for its orders, and Redhold builds nothing");
    assert!(Scene::outlined(&s).is_empty());
    let to = Hex::new(10, 9);
    click_hex(&mut s, Hex::new(10, 5));
    click_hex(&mut s, to);
    // It has not moved: it has a plan, and the map outlines its way.
    assert_eq!(pos(&s, a), Hex::new(10, 5));
    assert!(s.screen.game.map.units().get(a).unwrap().planned);
    let way = Scene::outlined(&s).to_vec();
    assert_eq!((way.len(), way.last()), (4, Some(&to)), "{way:?}");
    assert!(s.unit_line().starts_with("Planned: they go there as the round plays out"), "{}", s.unit_line());
    assert_eq!(button(&s), "Orders done (1)", "the stack has its orders");
    assert!(s.screen.game.selection().is_empty() && !s.asks_mission());
    // Selected again, it says what it plans; another click is another
    // plan, unasked.
    click_hex(&mut s, Hex::new(10, 5));
    assert_eq!(s.screen.game.selection(), [a]);
    assert!(s.unit_line().starts_with("Planned: to row 10, column 9"), "{}", s.unit_line());
    click_hex(&mut s, Hex::new(12, 5));
    assert!(!s.asks_mission(), "a plan is no mission to ask about");
    assert_eq!(Scene::outlined(&s).last(), Some(&Hex::new(12, 5)));
    // Undo takes the last plan back: the one before it stands.
    push_widget(&mut s, UNDO_ID);
    assert_eq!(s.screen.game.map.units().get(a).unwrap().route, [to]);
}

#[test]
fn the_round_plays_out_before_the_persons_eyes_and_their_next_orders_begin() {
    let (mut a, mut b) = (None, None);
    let mut s = session(game("passive", |g| {
        a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5)));
        b = Some(put(g, Side::Red, "soldiers", Hex::new(20, 5)));
    }));
    let (a, b) = (a.unwrap(), b.unwrap());
    click_hex(&mut s, Hex::new(10, 5));
    click_hex(&mut s, Hex::new(10, 9));
    click_hex(&mut s, Hex::new(20, 5));
    click_hex(&mut s, Hex::new(20, 8));
    // Where the person looks when their orders are done.
    s.screen.view.center_on(Hex::new(10, 7));
    let looked = s.screen.view.middle();
    s.end_turn();
    assert_eq!(s.screen.game.player(), Side::Blue, "blue gives its orders (the frames play them)");
    // A frame: blue has done. The next: the round plays out: both stacks
    // go at the same moment, a hex a tick.
    s.frame(990);
    assert!(
        s.screen.game.playing_out() && s.screen.game.map.units().get(a).is_some_and(|u| u.pos() == Hex::new(10, 5))
    );
    s.frame(1000);
    assert!(s.screen.game.playing_out());
    assert_eq!(button(&s), "The round plays out");
    let going: Vec<Vec<UnitId>> = Scene::moving(&s).into_iter().map(|m| m.units).collect();
    assert!(going.contains(&vec![a]) && going.contains(&vec![b]), "{going:?}");
    assert_eq!(pos(&s, a).distance(Hex::new(10, 5)), 1, "the game is a tick on");
    // A click gives no order meanwhile.
    click_hex(&mut s, Hex::new(10, 5));
    assert!(s.screen.game.selection().is_empty());
    // A tick is shown before the next is played.
    s.frame(1000 + STEP_MS / 2);
    assert_eq!(pos(&s, a).distance(Hex::new(10, 5)), 1);
    s.frame(1000 + STEP_MS + 1);
    assert_eq!(pos(&s, a).distance(Hex::new(10, 5)), 2, "the next tick");
    // To the end: round 2, the person's orders, the map where they left it.
    let t = frames(&mut s, 1000 + STEP_MS, 20_000, 40, |s| !s.screen.game.playing_out() && s.screen.game.turn() == 2);
    frames(&mut s, t, t + 1000, 40, |_| false);
    assert_eq!((s.screen.game.turn(), s.screen.game.player()), (2, Side::Red));
    assert_eq!((pos(&s, a), pos(&s, b)), (Hex::new(10, 9), Hex::new(20, 8)));
    assert!(Scene::moving(&s).is_empty());
    assert_eq!(button(&s), "Orders done (3)");
    assert_eq!(s.screen.view.middle(), looked);
    assert!(Scene::outlined(&s).is_empty(), "the plans ended with the round");
    // And they can give orders again.
    click_hex(&mut s, Hex::new(10, 9));
    assert_eq!(s.screen.game.selection(), [a]);
}

#[test]
fn the_persons_battles_are_played_out_in_their_window_as_the_round_goes() {
    let mut a = None;
    let mut s = session(game("passive", |g| {
        a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5)));
        put(g, Side::Blue, "levy", Hex::new(10, 7));
    }));
    let a = a.unwrap();
    click_hex(&mut s, Hex::new(10, 5));
    click_hex(&mut s, Hex::new(10, 7));
    assert_eq!(pos(&s, a), Hex::new(10, 5), "a plan: no battle yet");
    assert!(s.battle().is_none());
    s.end_turn();
    let t = frames(&mut s, 1000, 20_000, 40, |s| s.battle().is_some());
    assert!(s.battle().is_some(), "the battle, as the round comes to it");
    assert!(s.screen.game.map.units_at(Hex::new(10, 7)).contains(&a), "won, and in");
    // The round waits for the battle to be watched.
    assert_eq!(s.screen.game.turn(), 1);
    push_widget(&mut s, BATTLE_CLOSE_ID);
    frames(&mut s, t, t + 20_000, 40, |s| s.screen.game.turn() == 2 && s.screen.game.player() == Side::Red);
    assert_eq!((s.screen.game.turn(), s.screen.game.player()), (2, Side::Red));
}

#[test]
fn in_hotseat_each_plans_behind_the_cover_and_is_shown_the_round() {
    let (mut a, mut b) = (None, None);
    let mut s = session(game("person", |g| {
        a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5)));
        b = Some(put(g, Side::Blue, "soldiers", Hex::new(20, 24)));
    }));
    let (a, b) = (a.unwrap(), b.unwrap());
    // Red plans, and has done: the cover, for blue.
    push_widget(&mut s, BEGIN_TURN_ID);
    click_hex(&mut s, Hex::new(10, 5));
    click_hex(&mut s, Hex::new(10, 9));
    s.end_turn();
    assert!(s.covered() && s.screen.game.player() == Side::Blue);
    assert_eq!(pos(&s, a), Hex::new(10, 5), "nothing has moved");
    push_widget(&mut s, BEGIN_TURN_ID);
    // Blue sees none of red's plans.
    assert!(Scene::outlined(&s).is_empty());
    click_hex(&mut s, Hex::new(20, 24));
    click_hex(&mut s, Hex::new(20, 20));
    assert_eq!(Scene::outlined(&s).last(), Some(&Hex::new(20, 20)));
    // Blue has done: the round plays out before blue's eyes; then the
    // cover, for red's next orders.
    s.end_turn();
    assert!(!s.covered());
    let t = frames(&mut s, 1000, 20_000, 40, |s| s.covered());
    assert!(s.covered(), "red's turn, covered");
    assert_eq!((s.screen.game.turn(), s.screen.game.player()), (2, Side::Red));
    assert_eq!((pos(&s, a), pos(&s, b)), (Hex::new(10, 9), Hex::new(20, 20)));
    // Red begins: what happened is shown to red, who did not watch.
    push_widget(&mut s, BEGIN_TURN_ID);
    s.frame(t + 100);
    let going: Vec<Vec<UnitId>> = Scene::moving(&s).into_iter().map(|m| m.units).collect();
    assert!(going.contains(&vec![a]), "red's own stack, going again: {going:?}");
    frames(&mut s, t + 100, t + 5000, 40, |_| false);
    assert!(Scene::moving(&s).is_empty());
    // Once: blue, who watched it, is not shown it again; nor red twice.
    s.end_turn();
    push_widget(&mut s, BEGIN_TURN_ID);
    s.frame(t + 6000);
    assert!(Scene::moving(&s).is_empty(), "blue watched the round play out");
}

#[test]
fn a_round_in_one_step_for_scripts() {
    let mut a = None;
    let mut s = session(game("passive", |g| a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5)))));
    let a = a.unwrap();
    click_hex(&mut s, Hex::new(10, 5));
    click_hex(&mut s, Hex::new(10, 9));
    assert!(Scene::next_turn(&mut s));
    assert_eq!((s.screen.game.turn(), s.screen.game.player()), (2, Side::Red));
    assert!(!s.screen.game.playing_out());
    assert_eq!(pos(&s, a), Hex::new(10, 9));
}
