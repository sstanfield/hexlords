//! Looking at what happened again (`replay::Replay` at the screen; the
//! owner's word, 2026-11-22: "a 'replay' feature of the previous turn would
//! help. Maybe a slider on the side bar that would allow the user to 'go
//! back' in time and examine movement, battles, etc. You can not give
//! orders when 'in the past'").

mod common;

use common::{assets, click, pixel_of, press, push_widget, type_char, widget};
use hexlords::battle_window::BATTLE_CLOSE_ID;
use hexlords::game::Game;
use hexlords::options_window::Options;
use hexlords::orders::Order;
use hexlords::screen::Scene;
use hexlords::session::{END_TURN_ID, REPLAY_ID, REPLAY_SLIDER_ID, Session};
use hexlords::setup::{Fog, Mode, Setup};
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{Hex, Side, UnitId};
use turbo_ferret::widgets::Slider;
use turbo_ferret::{Size, SpecialKey};

/// 30 x 30 grass: red's Redhold, blue's Bluemoor; no chance in battle; red
/// a person, blue the computer's (passive). Soldiers (strength 5) and
/// levies (strength 1) go 6 hexes a round.
fn game(mode: Mode, place: impl FnOnce(&mut Game)) -> Game {
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
    (setup.mode, setup.fog, setup.combat.chance) = (mode, Fog::Off, 0);
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

fn pos(s: &Session, id: UnitId) -> Option<Hex> {
    s.screen.game.map.units().get(id).map(|u| u.pos())
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

fn shows_replay(s: &Session) -> bool {
    s.screen.ui.visibility(widget(s, "replay_row")) == turbo_ferret::Visibility::Visible
}

/// Rounds: the round that played out can be gone back into, a beat at a
/// time; the past is looked at, not ordered about; and now is as it was
/// left.
#[test]
fn the_round_that_played_out_can_be_gone_back_into() {
    let (mut a, mut b) = (None, None);
    let mut s = session(game(Mode::Rounds, |g| {
        a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5)));
        b = Some(put(g, Side::Blue, "levy", Hex::new(10, 8)));
    }));
    let (a, b) = (a.unwrap(), b.unwrap());
    let (from, to) = (Hex::new(10, 5), Hex::new(10, 8));
    s.pump();
    assert!(s.replay_at().is_none() && !shows_replay(&s), "nothing has happened yet");
    click_hex(&mut s, from);
    click_hex(&mut s, to);
    s.end_turn();
    // The round plays: the soldiers go, and beat the levy (its window
    // holds the round until it is closed).
    let mut t = frames(&mut s, 0, 60_000, 20, |s| s.battle().is_some());
    push_widget(&mut s, BATTLE_CLOSE_ID);
    t = frames(&mut s, t, t + 60_000, 20, |s| s.screen.game.turn() == 2 && button(s) == "Start turn");
    assert_eq!((pos(&s, a), pos(&s, b)), (Some(to), None));
    // (Their log is open: what happened can be looked at again once their
    // turn has started.)
    assert!(s.replay_at().is_some() && !shows_replay(&s));
    push_widget(&mut s, END_TURN_ID);
    s.pump();
    assert!(button(&s).starts_with("Orders done"), "{}", button(&s));
    // (The minimap marks where the round's battle was.)
    assert_eq!(Scene::battles(&s), [(to, 0.0)]);
    let (at, steps) = s.replay_at().expect("the round is kept");
    assert!(at == steps && steps >= 3 && shows_replay(&s) && !s.in_the_past(), "{at} of {steps}");
    // This round's first order, given before going back: it is the
    // present's, and still there afterwards.
    click_hex(&mut s, to);
    click_hex(&mut s, Hex::new(10, 10));
    assert!(s.screen.game.map.units().get(a).unwrap().planned);

    // Back to the round's start: everyone where they were, the levy alive.
    s.go_back(0);
    assert!(s.in_the_past() && s.replay_at() == Some((0, steps)));
    assert_eq!((pos(&s, a), pos(&s, b)), (Some(from), Some(to)));
    assert_eq!((s.screen.game.turn(), button(&s).as_str()), (1, "Back to now"));
    // ("Marking the battle locations on the minimap is a good idea, make
    // sure replay also does it": in the past, the battles up to then.)
    assert_eq!(Scene::battles(&s), [], "not fought yet");
    s.go_back(steps);
    assert_eq!(Scene::battles(&s), [(to, 0.0)], "fought by the round's end");
    s.go_back(0);
    assert!(s.unit_line().starts_with("The round as it went, step 0 of"), "{}", s.unit_line());
    // A step on: the soldiers have gone a hex or two.
    s.go_back(1);
    let then = pos(&s, a).unwrap();
    assert!(then != from && then != to && s.replay_at() == Some((1, steps)), "{then:?}");
    // The past is looked at: a click shows a stack, and gives no orders.
    click_hex(&mut s, then);
    click_hex(&mut s, Hex::new(12, 6));
    assert_eq!(pos(&s, a), Some(then));
    assert!(!s.screen.game.map.units().get(a).unwrap().route.contains(&Hex::new(12, 6)));
    let refused = s.screen.game.apply(Order::EndTurn);
    assert!(refused.is_err_and(|why| why.contains("orders are given in the game as it is now")));
    // The slider, moved by the person: the screen goes there.
    let slider = widget(&s, REPLAY_SLIDER_ID);
    s.screen.ui.with_widget::<Slider, _>(slider, |w, cx| w.set_value_f(cx, 2.0));
    s.pump();
    assert_eq!(s.replay_at(), Some((2, steps)));
    // All the way right, or the button, or Esc: now, as it was left.
    s.screen.ui.with_widget::<Slider, _>(slider, |w, cx| w.set_value_f(cx, steps as f64));
    s.pump();
    assert!(!s.in_the_past() && s.screen.game.turn() == 2);
    assert_eq!((pos(&s, a), pos(&s, b)), (Some(to), None));
    assert!(s.screen.game.map.units().get(a).unwrap().planned, "the plan made before is still there");
    s.go_back(1);
    push_widget(&mut s, END_TURN_ID);
    assert!(!s.in_the_past() && button(&s).starts_with("Orders done"));
    s.go_back(1);
    press(&mut s, SpecialKey::Esc);
    assert!(!s.in_the_past());

    // Replay: the round again from its start, shown as it was, the battle
    // in its window; then now.
    push_widget(&mut s, REPLAY_ID);
    assert!(s.in_the_past() && s.replay_at() == Some((0, steps)));
    let mut moved = false;
    t = frames(&mut s, t, t + 60_000, 20, |s| {
        moved |= !Scene::moving(s).is_empty();
        s.battle().is_some()
    });
    assert!(moved && s.battle().is_some() && s.in_the_past(), "the stacks go again, and fight again");
    assert!(Scene::battles(&s).iter().any(|(at, _)| *at == to), "and the minimap marks it again");
    push_widget(&mut s, BATTLE_CLOSE_ID);
    frames(&mut s, t, t + 60_000, 20, |s| !s.in_the_past());
    assert!(!s.in_the_past() && s.screen.game.turn() == 2);
    assert_eq!((pos(&s, a), pos(&s, b)), (Some(to), None));
    // W plays it too, and stops it where it is.
    type_char(&mut s, 'w');
    assert!(s.in_the_past() && button(&s) == "Back to now");
    type_char(&mut s, 'w');
    let t2 = frames(&mut s, 100_000, 103_000, 20, |_| false);
    assert_eq!(s.replay_at(), Some((0, steps)), "stopped at once: still at the start");
    assert!(t2 > 0 && s.in_the_past());
    s.back_to_now();
}

/// Turns: what happened from the end of the person's turn to the start of
/// their next (their own marches as it ended, the others' turns), an
/// order at a time.
#[test]
fn the_others_turns_can_be_gone_back_into() {
    let (mut a, mut b) = (None, None);
    let mut s = session(game(Mode::Turns, |g| {
        a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5)));
        b = Some(put(g, Side::Blue, "levy", Hex::new(20, 20)));
    }));
    let (a, b) = (a.unwrap(), b.unwrap());
    let far = Hex::new(10, 20);
    // Red's soldiers are sent far, and take this turn's part of the way at
    // once; the rest at the end of each of red's turns.
    s.screen.game.apply(Order::GoTo { units: vec![a], to: far, via: Vec::new() }).unwrap();
    let first = pos(&s, a).unwrap();
    s.end_turn();
    frames(&mut s, 0, 60_000, 20, |s| s.screen.game.turn() == 2 && button(s).starts_with("End"));
    assert!(s.replay_at().is_none_or(|(_, steps)| steps <= 2), "red's end of turn and blue's: {:?}", s.replay_at());
    // Round 2: red ends its turn again (the soldiers march on as it ends),
    // blue passes, and red's third turn begins.
    s.end_turn();
    frames(&mut s, 60_000, 120_000, 20, |s| s.screen.game.turn() == 3 && button(s).starts_with("End"));
    let second = pos(&s, a).unwrap();
    assert!(second != first && second.distance(far) < first.distance(far));
    let (at, steps) = s.replay_at().expect("kept");
    assert_eq!((at, steps), (2, 2), "red's EndTurn and blue's");
    assert!(shows_replay(&s));
    // Before red's turn ended: the soldiers where they stood.
    s.go_back(0);
    assert_eq!((pos(&s, a), s.screen.game.turn(), s.screen.game.player()), (Some(first), 2, Side::Red));
    assert!(s.unit_line().starts_with("The others' turns as they went, step 0 of 2"), "{}", s.unit_line());
    // After it: they have marched, and it is blue's turn.
    s.go_back(1);
    assert_eq!((pos(&s, a), s.screen.game.player()), (Some(second), Side::Blue));
    assert_eq!(pos(&s, b), Some(Hex::new(20, 20)));
    s.back_to_now();
    assert_eq!((s.screen.game.turn(), s.screen.game.player(), s.in_the_past()), (3, Side::Red, false));
    // Played again: the march is shown, then now.
    push_widget(&mut s, REPLAY_ID);
    let mut moved = false;
    frames(&mut s, 200_000, 260_000, 20, |s| {
        moved |= !Scene::moving(s).is_empty();
        !s.in_the_past()
    });
    assert!(moved && !s.in_the_past());
    assert_eq!(pos(&s, a), Some(second));
}
