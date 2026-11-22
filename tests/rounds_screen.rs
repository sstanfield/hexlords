//! Rounds at the screen (`docs/gameplay.md`, "Rounds"; the owner's word,
//! 2026-11-22): a click plans, the map shows the person's plans, "Orders
//! done" passes to the next side, and once every side has done the round
//! plays out before the person's eyes, a beat at a time, the stacks of a
//! beat at the same time (a battle holds it until its window is closed);
//! in hotseat each person plans behind the cover, and is shown what
//! happened.

mod common;

use common::{assets, click, pixel_of, press, push_widget, type_char, widget};
use hexlords::battle_window::BATTLE_CLOSE_ID;
use hexlords::game::Game;
use hexlords::options_window::Options;
use hexlords::render::{BATTLE_BORDER, OWN_WAY_BORDER, SELECTED_BORDER};
use hexlords::screen::Scene;
use hexlords::session::{BEGIN_TURN_ID, END_TURN_ID, MISSION_GO_ID, Session, UNDO_ID};
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
    assert_eq!(button(&s), "Playing the round");
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
fn a_click_on_land_never_seen_sends_the_stack_to_look() {
    let g = game("passive", |g| {
        put(g, Side::Red, "soldiers", Hex::new(10, 5));
    });
    let mut setup = g.setup().clone();
    setup.fog = Fog::Total;
    let g = g.with_setup(setup);
    let a = g.map.units().of_side(Side::Red).next().map(|u| u.id()).unwrap();
    let mut s = session(g);
    let far = Hex::new(10, 20);
    assert!(!s.sight().knows(far));
    click_hex(&mut s, Hex::new(10, 5));
    click_hex(&mut s, far);
    assert!(s.refusal().is_none(), "not refused: it goes to look");
    let unit = s.screen.game.map.units().get(a).unwrap();
    assert!(unit.planned && unit.route == [far]);
    assert!(s.unit_line().starts_with("Planned: they set off as the round plays out"), "{}", s.unit_line());
    assert_eq!(Scene::outlined(&s).last(), Some(&far), "where it is sent, in the dark");
    assert!(Scene::next_turn(&mut s));
    assert!(pos(&s, a).distance(Hex::new(10, 5)) >= 5, "its whole round, into what it came to see");
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

/// The owner's word, 2026-11-22: "If you want to consolidate stacks then
/// you would order one stack to another then combine them and order them
/// as a new stack. That would cause the stack being joined to wait for the
/// other units before moving or attacking."
#[test]
fn a_stack_sent_onto_another_is_offered_with_it_and_they_are_sent_on_as_one() {
    let (mut a, mut b) = (None, None);
    let (here, on) = (Hex::new(10, 8), Hex::new(10, 10));
    let mut s = session(game("passive", |g| {
        a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5)));
        b = Some(put(g, Side::Red, "levy", here));
    }));
    let (a, b) = (a.unwrap(), b.unwrap());
    click_hex(&mut s, Hex::new(10, 5));
    // (The line under the map: a hex with their own side's stack on it is
    // somewhere to go, not an attack with its odds.)
    s.screen.game.set_cursor(here);
    let line = hexlords::info::cursor_line_for(&s.screen.game, s.sight());
    assert!(line.ends_with("1 unit  |  6 of 12 points to move here"), "{line}");
    click_hex(&mut s, here);
    assert!(s.asks_mission(), "join it, or take that stack?");
    push_widget(&mut s, MISSION_GO_ID);
    // The soldiers plan to join the levy; the levy's hex is selected, the
    // soldiers with it, marked as on their way.
    assert_eq!(s.screen.game.map.units().get(a).unwrap().route, [here]);
    assert_eq!((s.shown(), pos(&s, a)), (here, Hex::new(10, 5)));
    assert_eq!(s.screen.game.selection(), [b, a]);
    assert_eq!(s.stack(), ["1 \u{b7} 12/12", "+ 5 \u{b7} 12"]);
    assert!(s.unit_line().starts_with("Planned: they join this stack"), "{}", s.unit_line());
    // Where they can go together is from the levy's hex, with what the
    // way there leaves the soldiers: three hexes more, not four (the levy
    // alone could).
    let game = &s.screen.game;
    assert!(game.can_move_to(Hex::new(10, 11)) && !game.can_move_to(Hex::new(10, 12)));
    // A box leaves one out, or takes it along.
    s.screen.game.toggle_unit(a);
    assert_eq!(s.screen.game.selection(), [b]);
    assert!(s.screen.game.can_move_to(Hex::new(10, 12)));
    s.screen.game.toggle_unit(a);
    assert_eq!(s.screen.game.selection(), [b, a]);
    // (A click on their hex leaves them selected, and says what they are.)
    click_hex(&mut s, here);
    assert_eq!(s.screen.game.selection(), [b, a]);
    assert!(s.unit_line().starts_with("2 units selected, 1 of them (+) on their way here"), "{}", s.unit_line());
    // Sent on: one order for both. The levy goes from where it stands, the
    // soldiers by way of it; the map outlines all of it.
    click_hex(&mut s, on);
    assert!(!s.asks_mission(), "{}", s.unit_line());
    let unit = |s: &Session, id: UnitId| s.screen.game.map.units().get(id).unwrap().clone();
    assert_eq!((unit(&s, b).route, unit(&s, a).route), (vec![on], vec![here, on]));
    assert_eq!(unit(&s, a).party, unit(&s, b).party);
    let outlined = Scene::outlined(&s).to_vec();
    assert!(outlined.contains(&here) && outlined.contains(&on) && outlined.len() >= 5, "{outlined:?}");
    // The round: the levy waits for the soldiers, and they get there
    // together.
    s.end_turn();
    let mut together = false;
    frames(&mut s, 0, 60_000, 20, |s| {
        together |= pos(s, a) == here && pos(s, b) == here;
        s.screen.game.turn() == 2
    });
    assert!(together, "the levy was still there when the soldiers came");
    assert_eq!((pos(&s, a), pos(&s, b)), (on, on));
}

/// The owner's word, 2026-11-22: "when combat happens the advancement
/// should stop, resolve the combat, and let the user dismiss the box (if
/// using it) before moving on. The user can miss details otherwise."
#[test]
fn a_battle_holds_the_round_until_its_window_is_closed() {
    let (mut a, mut c) = (None, None);
    let mut s = session(game("passive", |g| {
        a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5)));
        c = Some(put(g, Side::Red, "soldiers", Hex::new(20, 5)));
        put(g, Side::Blue, "levy", Hex::new(10, 7));
    }));
    let (a, c) = (a.unwrap(), c.unwrap());
    // One stack attacks two hexes off (a third of the round); the other
    // has six hexes to go, the whole round.
    let far = Hex::new(20, 11);
    click_hex(&mut s, Hex::new(10, 5));
    click_hex(&mut s, Hex::new(10, 7));
    click_hex(&mut s, Hex::new(20, 5));
    click_hex(&mut s, far);
    s.end_turn();
    let t = frames(&mut s, 0, 60_000, 20, |s| s.battle().is_some());
    assert!(s.battle().is_some(), "the battle's window opened");
    assert_eq!(pos(&s, a), Hex::new(10, 7), "the battle has been fought");
    // Where it is fought shows: the hex is ringed, it flashes while the
    // battle is played out, and the window is clear of it.
    let field = Hex::new(10, 7);
    assert!(Scene::rings(&s).contains(&(field, BATTLE_BORDER)), "{:?}", Scene::rings(&s));
    let window = s.screen.ui.rect(s.battle().unwrap().window());
    let p = pixel_of(&s, field);
    let inside = p.x >= window.x && p.x < window.x + window.w && p.y >= window.y && p.y < window.y + window.h;
    assert!(s.screen.view.shows(field) && !inside, "{p:?} in {window:?}");
    let mut flashed = false;
    let t = frames(&mut s, t, t + 2_000, 20, |s| {
        flashed |= Scene::flash(s).is_some_and(|(at, color)| at == field && color[3] > 0.2);
        flashed
    });
    assert!(flashed, "the hex flashes");
    // The round waits: the other stack is where the battle found it, for
    // as long as the window is open, played out or not.
    let waiting = pos(&s, c);
    assert!(s.screen.game.playing_out() && waiting != far && waiting != Hex::new(20, 5), "{waiting:?}");
    let t = frames(&mut s, t, t + 30_000, 50, |_| false);
    assert!(s.battle().is_some_and(|b| b.done()), "played out, and still open");
    assert!(s.screen.game.playing_out() && pos(&s, c) == waiting, "nothing went on behind it");
    assert!(Scene::rings(&s).contains(&(field, BATTLE_BORDER)) && Scene::flash(&s).is_none(), "ringed, at rest");
    // Closed, the round goes on to its end.
    push_widget(&mut s, BATTLE_CLOSE_ID);
    assert!(!Scene::rings(&s).iter().any(|(_, color)| *color == BATTLE_BORDER));
    frames(&mut s, t, t + 60_000, 20, |s| s.screen.game.turn() == 2);
    assert_eq!((s.screen.game.turn(), pos(&s, c)), (2, far));
}

/// With battles told in a line (Options) no window opens, but the map
/// still shows where: "If the combat window is off still move to the
/// location and maybe flash up the ring." Then the round goes on by itself.
#[test]
fn a_battle_told_in_a_line_still_shows_where_it_was() {
    let mut a = None;
    let game = game("passive", |g| {
        a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5)));
        put(g, Side::Blue, "levy", Hex::new(10, 7));
    });
    let options = Options { show_moves: true, quick_battles: true, ..Options::manual() };
    let mut s = Session::with_options(&assets(), Size::new(1024, 768), 96, game, options).unwrap();
    let field = Hex::new(10, 7);
    click_hex(&mut s, Hex::new(10, 5));
    click_hex(&mut s, field);
    // The person looks elsewhere when their orders are done.
    s.screen.view.center_on(Hex::new(25, 25));
    assert!(!s.screen.view.shows(field));
    s.end_turn();
    let marked = |s: &Session| Scene::rings(s).contains(&(field, BATTLE_BORDER));
    let t = frames(&mut s, 0, 60_000, 20, marked);
    assert!(marked(&s) && s.battle().is_none(), "ringed, with no window");
    assert!(s.screen.view.shows(field), "the map has gone there");
    assert!(s.unit_line().starts_with("You attacked"), "{}", s.unit_line());
    assert!(s.screen.game.playing_out(), "the round waits a moment");
    // A moment later the ring is gone and the round has gone on, unasked.
    frames(&mut s, t, t + 60_000, 20, |s| s.screen.game.turn() == 2);
    assert_eq!((s.screen.game.turn(), pos(&s, a.unwrap())), (2, field));
    assert!(!marked(&s));
}

/// The owner's word, 2026-11-22: "When a stack is selected display its
/// order outlines in a different color than other orders. Add an
/// option/hotkey to alternate between displaying all orders (default) and
/// only the order for the current stack ... The stack that is currently
/// selected can be harder to find on a busy map ... outline the selected
/// stack/hex (could be a city) in green".
#[test]
fn the_selected_stack_and_its_orders_stand_out_and_v_shows_them_alone() {
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
    let way_of = |s: &Session, id: UnitId| s.screen.game.map.units().get(id).unwrap().way.clone();
    let (way_a, way_b) = (way_of(&s, a), way_of(&s, b));
    assert_eq!((way_a.len(), way_b.len()), (4, 3));
    // Nothing selected: every order outlined alike, nothing ringed.
    assert!(s.screen.game.selection().is_empty() && Scene::rings(&s).is_empty());
    let outlined = Scene::outlined(&s).to_vec();
    assert!(way_a.iter().chain(&way_b).all(|hex| outlined.contains(hex)) && outlined.len() == 7);
    // The first stack selected: its hex ringed in green, its way in a
    // color of its own; the other's order is outlined as before.
    click_hex(&mut s, Hex::new(10, 5));
    assert_eq!(s.screen.game.selection(), [a]);
    let rings = Scene::rings(&s);
    assert!(rings.contains(&(Hex::new(10, 5), SELECTED_BORDER)), "{rings:?}");
    assert!(way_a.iter().all(|hex| rings.contains(&(*hex, OWN_WAY_BORDER))) && rings.len() == 5, "{rings:?}");
    assert_eq!(Scene::outlined(&s), way_b);
    // V: the selected stack's only; again: everyone's.
    type_char(&mut s, 'v');
    assert!(s.options().own_orders_only && Scene::outlined(&s).is_empty());
    assert_eq!(Scene::rings(&s).len(), 5, "its own, still");
    assert!(s.unit_line().starts_with("The map outlines the selected stack's orders only"), "{}", s.unit_line());
    // (With nothing selected then, none.)
    press(&mut s, turbo_ferret::SpecialKey::Esc);
    assert!(s.screen.game.selection().is_empty());
    assert!(Scene::outlined(&s).is_empty() && Scene::rings(&s).is_empty());
    type_char(&mut s, 'V');
    assert!(!s.options().own_orders_only);
    assert_eq!(Scene::outlined(&s).len(), 7);
}
