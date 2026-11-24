//! The review of rounds at the screen (2026-11-22): the reviewer's probes,
//! kept as tests. Those marked as findings failed before their fix (the
//! round shown again in hotseat going on under the next person's cover,
//! not cut short by a plan, showing stacks the person never saw, starting
//! behind the log; labels that did not fit; a plan with no room accepted;
//! temples and later stops in rounds); the others are coverage.

mod common;

use common::{assets, click, pixel_of, press, push_widget, type_char, widget};
use hexlords::battle_window::BATTLE_CLOSE_ID;
use hexlords::game::Game;
use hexlords::options_window::Options;
use hexlords::orders::Order;
use hexlords::screen::Scene;
use hexlords::session::{BEGIN_TURN_ID, END_TURN_ID, Session, UNDO_ID};
use hexlords::setup::{Fog, Mode, Setup, TurnOrder};
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{Hex, Side, UnitId};
use turbo_ferret::Size;

/// As `tests/rounds_screen.rs`: 30 x 30 grass, Redhold (5, 5), Bluemoor
/// (24, 24), rounds, no chance in battle; red a person, blue `blue`.
fn game(blue: &str, place: impl FnOnce(&mut Game)) -> Game {
    game_with(blue, Mode::Rounds, place)
}

fn game_with(blue: &str, mode: Mode, place: impl FnOnce(&mut Game)) -> Game {
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
    session_with(game, Options { show_moves: true, ..Options::manual() })
}

fn session_with(game: Game, options: Options) -> Session {
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

fn frames(s: &mut Session, from: u64, until: u64, step: u64, mut done: impl FnMut(&Session) -> bool) -> u64 {
    let mut t = from;
    while t < until && !done(s) {
        t += step;
        s.frame(t);
    }
    t
}

/// A person at the screen who does what the button says, round after round:
/// Begin under the cover, Start turn between turns, Orders done on their
/// turn. Frames 50 ms apart. The round reached after `ms` of that; panics if
/// a round takes more than two minutes of frames.
fn play_along(s: &mut Session, rounds: u32) -> u32 {
    let mut t = 0;
    let mut last = (s.screen.game.turn(), 0u64);
    while s.screen.game.turn() <= rounds && s.screen.game.winner().is_none() {
        t += 50;
        s.frame(t);
        // (A battle shown holds the round until its window is closed.)
        if s.battle().is_some_and(|b| b.done()) {
            push_widget(s, BATTLE_CLOSE_ID);
        }
        if s.covered() {
            push_widget(s, BEGIN_TURN_ID);
        } else {
            let label = button(s);
            if label == "Start turn" || label.starts_with("Orders done") || label.ends_with("orders done") {
                push_widget(s, END_TURN_ID);
            }
        }
        if s.screen.game.turn() != last.0 {
            last = (s.screen.game.turn(), t);
        }
        assert!(
            t - last.1 < 120_000,
            "stuck in round {}: button \"{}\", playing out {}, covered {}, battle {}, log \"{}\"",
            s.screen.game.turn(),
            button(s),
            s.screen.game.playing_out(),
            s.covered(),
            s.battle().is_some(),
            s.log_status_line(),
        );
    }
    s.screen.game.turn()
}

/// The continent in rounds: `people` at the screen, the others `simple`; the
/// people's stacks all maraud (the computer leads them: battles of theirs).
fn continent(people: &str, fog: Fog, order: TurnOrder, seed: u64) -> Game {
    let (game, _) = Game::load(&assets().join("maps/continent")).unwrap();
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "simple");
    setup.set_players(people).unwrap();
    (setup.mode, setup.fog, setup.order, setup.seed) = (Mode::Rounds, fog, order, seed);
    let mut game = game.with_setup(setup);
    game.watch(true);
    game
}

fn maraud_all(s: &mut Session) {
    let side = s.screen.game.player();
    for hex in s.screen.game.stacks_of(side) {
        let map = &s.screen.game.map;
        let units: Vec<UnitId> = map
            .units_at(hex)
            .iter()
            .copied()
            .filter(|id| map.units().get(*id).is_some_and(|u| u.side() == side))
            .collect();
        let _ = s.screen.game.apply(Order::Maraud { units, on: true });
    }
}

// ---------------------------------------------------------------- soak

#[test]
fn soak_solo_rounds_go_on_with_every_option() {
    for (i, options) in [
        Options { show_moves: true, ..Options::manual() },
        Options::manual(),
        Options { show_moves: true, quick_battles: true, ..Options::manual() },
        Options::default(),
        Options { next_after_move: true, always_bless: true, fast_battles: true, ..Options::default() },
    ]
    .into_iter()
    .enumerate()
    {
        for (fog, order) in [(Fog::Off, TurnOrder::Fixed), (Fog::On, TurnOrder::Drawn), (Fog::Total, TurnOrder::Drawn)]
        {
            let mut s = session_with(continent("red=person", fog, order, 7 + i as u64), options);
            // (End turn asks, by default: the probe presses on.)
            let mut o = s.options();
            o.ask_end_turn = false;
            s.set_options(o);
            maraud_all(&mut s);
            let reached = play_along(&mut s, 12);
            let battles = s
                .screen
                .game
                .history()
                .iter()
                .filter(|r| matches!(r.event, hexlords::orders::Event::Battle { .. }))
                .count();
            eprintln!(
                "solo {i} {fog:?} {order:?}: round {reached}, {battles} battles, out {:?}, winner {:?}",
                s.screen.game.out(),
                s.screen.game.winner()
            );
            assert!(reached > 12 || s.screen.game.winner().is_some(), "{i} {fog:?} {order:?}: round {reached}");
        }
    }
}

#[test]
fn soak_hotseat_rounds_go_on() {
    for (people, order) in [
        ("red=person,blue=person", TurnOrder::Fixed),
        ("red=person,blue=person", TurnOrder::Drawn),
        ("red=person,green=person,blue=person,yellow=person", TurnOrder::Drawn),
        ("green=person,yellow=person,red=simple", TurnOrder::Drawn),
    ] {
        let mut s =
            session_with(continent(people, Fog::On, order, 3), Options { show_moves: true, ..Options::manual() });
        let reached = play_along(&mut s, 10);
        let battles = s
            .screen
            .game
            .history()
            .iter()
            .filter(|r| matches!(r.event, hexlords::orders::Event::Battle { .. }))
            .count();
        eprintln!("hotseat {people} {order:?}: round {reached}, {battles} battles, out {:?}", s.screen.game.out());
        assert!(reached > 10 || s.screen.game.winner().is_some(), "{people} {order:?}: round {reached}");
    }
}

/// Sound (it passes): two people whose stacks fight (they maraud), each
/// pressing Begin, Start turn and "Orders done" as soon as they can: the
/// game goes on round after round. (This game does not happen to leave a
/// battle of the round shown again for the next person's cover: the probe
/// `the_round_shown_again_does_not_go_on_under_the_next_persons_cover` does.)
#[test]
fn soak_hotseat_rounds_with_battles_go_on() {
    let mut s = session_with(
        continent("red=person,green=person", Fog::On, TurnOrder::Fixed, 3),
        Options { show_moves: true, ..Options::manual() },
    );
    let mut t = 0;
    let mut last = (s.screen.game.turn(), 0u64);
    let mut marauding: Vec<Side> = Vec::new();
    let mut over_cover = 0;
    while s.screen.game.turn() <= 12 && s.screen.game.winner().is_none() {
        t += 50;
        s.frame(t);
        if s.covered() && s.battle().is_some() {
            over_cover += 1;
        }
        if s.battle().is_some_and(|b| b.done()) {
            push_widget(&mut s, BATTLE_CLOSE_ID);
        }
        if s.covered() {
            push_widget(&mut s, BEGIN_TURN_ID);
        } else {
            let label = button(&s);
            let side = s.screen.game.player();
            if label.starts_with("Orders done") && !marauding.contains(&side) {
                marauding.push(side);
                maraud_all(&mut s);
            }
            if label == "Start turn" || label.starts_with("Orders done") || label.ends_with("orders done") {
                push_widget(&mut s, END_TURN_ID);
            }
        }
        if s.screen.game.turn() != last.0 {
            last = (s.screen.game.turn(), t);
        }
        assert!(
            t - last.1 < 120_000,
            "stuck in round {}: covered {}, a battle's window open {}, button \"{}\"",
            s.screen.game.turn(),
            s.covered(),
            s.battle().is_some(),
            button(&s),
        );
    }
    let battles =
        s.screen.game.history().iter().filter(|r| matches!(r.event, hexlords::orders::Event::Battle { .. })).count();
    eprintln!("round {}, {battles} battles, out {:?}", s.screen.game.turn(), s.screen.game.out());
    assert_eq!(over_cover, 0, "frames with a battle's window open over a cover");
}

// ---------------------------------------------------------------- input while it plays out

#[test]
fn nothing_gives_an_order_while_the_round_plays_out() {
    let mut a = None;
    let mut s = session(game("passive", |g| {
        a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5)));
        put(g, Side::Red, "soldiers", Hex::new(20, 5));
    }));
    let a = a.unwrap();
    click_hex(&mut s, Hex::new(10, 5));
    click_hex(&mut s, Hex::new(10, 9));
    s.end_turn();
    s.frame(990);
    s.frame(1000);
    assert!(s.screen.game.playing_out());
    let before = format!("{:?}", s.screen.game.map.units().get(a));
    let revision = s.screen.game.map.revision();
    for key in ['n', 'N', 'd', 'f', 'g', 'p', 'r', 'u', 'b', 'x', 'c', 't', 'o', 'h', 'm', 'a', 'l'] {
        type_char(&mut s, key);
        assert!(s.refusal().is_none(), "{key}: {:?}", s.refusal());
        assert!(s.screen.game.selection().is_empty(), "{key} selected");
        assert!(s.picking().is_none(), "{key} picks");
    }
    push_widget(&mut s, UNDO_ID);
    push_widget(&mut s, END_TURN_ID);
    click_hex(&mut s, Hex::new(20, 5));
    click_hex(&mut s, Hex::new(5, 5));
    assert!(s.refusal().is_none() && s.screen.game.selection().is_empty());
    assert!(s.screen.game.playing_out());
    assert_eq!(format!("{:?}", s.screen.game.map.units().get(a)), before);
    assert_eq!(s.screen.game.map.revision(), revision, "nothing changed the map");
    // And the round still gets to its end.
    let t = frames(&mut s, 1000, 60_000, 40, |s| s.screen.game.turn() == 2 && button(s).starts_with("Orders done"));
    assert!(t < 60_000, "{} {}", s.screen.game.turn(), button(&s));
}

/// The city's window open as the round plays out: its buttons give no order
/// and say nothing was refused.
#[test]
fn a_city_window_gives_no_order_while_the_round_plays_out() {
    let mut s = session(game("passive", |g| {
        put(g, Side::Red, "soldiers", Hex::new(10, 5));
    }));
    click_hex(&mut s, Hex::new(10, 5));
    click_hex(&mut s, Hex::new(10, 9));
    // (The stack stays selected, to be sent on: let go of it.)
    press(&mut s, turbo_ferret::SpecialKey::Esc);
    click_hex(&mut s, Hex::new(5, 5));
    type_char(&mut s, 'o');
    assert!(s.city_window().is_some());
    s.end_turn();
    s.frame(990);
    s.frame(1000);
    assert!(s.screen.game.playing_out());
    let ids: Vec<_> = common::descendants(&s, hexlords::city_windows::CITY_WINDOW_ID);
    let revision = s.screen.game.map.revision();
    for id in ids {
        if s.screen
            .ui
            .find(s.screen.ui.root(), turbo_ferret::TbId::new(hexlords::city_windows::CITY_WINDOW_ID))
            .is_none()
        {
            break;
        }
        if s.screen.ui.node(id).is_none() {
            continue;
        }
        let r = s.screen.area(id);
        if r.w <= 0 || r.h <= 0 {
            continue;
        }
        click(&mut s, turbo_ferret::Point::new(r.x + r.w / 2, r.y + r.h / 2));
        assert!(s.refusal().is_none(), "{:?}", s.refusal());
    }
    assert_eq!(s.screen.game.map.revision(), revision);
}

// ---------------------------------------------------------------- hotseat: the round shown again

/// Red and blue at the screen; red attacks two of blue's levies with two
/// stacks; blue watches the round; red is shown it as round 2 begins.
fn hotseat_with_two_battles(fog: Fog) -> (Session, u64, [UnitId; 2]) {
    let (mut a, mut b) = (None, None);
    let mut g = game("person", |g| {
        a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5)));
        b = Some(put(g, Side::Red, "soldiers", Hex::new(20, 5)));
        put(g, Side::Blue, "levy", Hex::new(10, 8));
        put(g, Side::Blue, "levy", Hex::new(20, 8));
    });
    if fog != Fog::Off {
        let mut setup = g.setup().clone();
        setup.fog = fog;
        g = g.with_setup(setup);
    }
    let mut s = session(g);
    push_widget(&mut s, BEGIN_TURN_ID);
    click_hex(&mut s, Hex::new(10, 5));
    click_hex(&mut s, Hex::new(10, 8));
    click_hex(&mut s, Hex::new(20, 5));
    click_hex(&mut s, Hex::new(20, 8));
    s.end_turn();
    push_widget(&mut s, BEGIN_TURN_ID);
    s.end_turn();
    // Blue watches: the battles' windows are closed as they come.
    let mut t = 1000;
    while t < 120_000 && !s.covered() {
        t += 40;
        s.frame(t);
        if s.battle().is_some_and(|b| b.done()) {
            push_widget(&mut s, hexlords::battle_window::BATTLE_CLOSE_ID);
        }
    }
    assert!(s.covered() && s.screen.game.turn() == 2 && s.screen.game.player() == Side::Red, "round 2, red's");
    (s, t, [a.unwrap(), b.unwrap()])
}

/// Red begins round 2 (the log of the battles opens: Start turn closes it).
fn red_begins(s: &mut Session, t: u64) {
    push_widget(s, BEGIN_TURN_ID);
    assert!(!s.covered());
    s.frame(t + 40);
    if button(s) == "Start turn" {
        push_widget(s, END_TURN_ID);
    }
    assert!(button(s).starts_with("Orders done"), "{}", button(s));
}

/// FINDING: red is shown the last round as their orders begin; if red says
/// "Orders done" before it has all been shown, what is left of it goes on
/// for the next person: a battle of red's opens its window over blue's
/// cover.
#[test]
fn the_round_shown_again_does_not_go_on_under_the_next_persons_cover() {
    let (mut s, t, _) = hotseat_with_two_battles(Fog::Off);
    red_begins(&mut s, t);
    // The first of red's two battles opens its window; red lets it play
    // for two seconds, has nothing to say this round, and says Orders done.
    let t = frames(&mut s, t + 40, t + 10_000, 40, |s| s.battle().is_some());
    assert!(s.battle().is_some(), "the first battle of the round shown again");
    let t = frames(&mut s, t, t + 2000, 40, |_| false);
    push_widget(&mut s, END_TURN_ID);
    assert!(s.covered() && s.screen.game.player() == Side::Blue, "blue's cover: {}", button(&s));
    assert!(s.battle().is_none(), "the cover closed red's windows");
    let mut opened = false;
    let mut u = t + 40;
    while u < t + 20_000 && !opened {
        u += 40;
        s.frame(u);
        opened |= s.battle().is_some();
    }
    assert!(s.covered());
    // What blue can do about it: its Close button does nothing under the
    // cover; Begin, if the window is not over it.
    let mut closes = false;
    if opened {
        push_widget(&mut s, hexlords::battle_window::BATTLE_CLOSE_ID);
        closes = s.battle().is_none();
    }
    let over_begin = opened
        && s.battle().is_some_and(|b| {
            let (w, begin) = (s.screen.area(b.window()), s.screen.area(widget(&s, BEGIN_TURN_ID)));
            w.contains(turbo_ferret::Point::new(begin.x + begin.w / 2, begin.y + begin.h / 2))
        });
    assert!(
        !opened,
        "a battle of the round shown to red opened its window over blue's cover (its Close closes it: {closes}; it \
         is over the Begin button: {over_begin})"
    );
}

/// FINDING: the same, and blue begins while it is still going: blue is
/// shown red's replay (red's stacks going again), and the map shows blue
/// less than blue sees.
#[test]
fn the_round_shown_again_is_not_shown_to_the_next_person() {
    let (mut s, t, [a, b]) = hotseat_with_two_battles(Fog::On);
    red_begins(&mut s, t);
    push_widget(&mut s, END_TURN_ID);
    assert!(s.covered() && s.screen.game.player() == Side::Blue);
    push_widget(&mut s, BEGIN_TURN_ID);
    s.frame(t + 80);
    assert!(!s.covered());
    let going: Vec<UnitId> = Scene::moving(&s).into_iter().flat_map(|m| m.units).collect();
    let blue_sees = s.screen.game.sight(Side::Blue);
    let shown = Scene::sight(&s);
    let hidden: Vec<Hex> = (0..30u16)
        .flat_map(|r| (0..30u16).map(move |c| Hex::new(r, c)))
        .filter(|h| blue_sees.sees(*h) && !shown.sees(*h))
        .collect();
    assert!(
        !going.contains(&a) && !going.contains(&b) && hidden.is_empty(),
        "red's stacks shown going to blue: {going:?}; hexes blue sees that the map does not show: {}",
        hidden.len()
    );
}

/// FINDING: an order of the person's does not cut the round shown again
/// short (a plan is no event): the stack they plan for is still drawn where
/// it was a round ago, and its reach is not marked.
#[test]
fn an_order_cuts_the_round_shown_again_short() {
    let (mut s, t, [a, _]) = hotseat_with_two_battles(Fog::Off);
    red_begins(&mut s, t);
    let at = pos(&s, a);
    assert_eq!(at, Hex::new(10, 8), "a won, and is in");
    // Red takes the stack and plans.
    click_hex(&mut s, at);
    assert_eq!(s.screen.game.selection(), [a]);
    let to = Hex::new(10, 10);
    let marked = Scene::marked(&s, to);
    click_hex(&mut s, to);
    assert!(s.screen.game.map.units().get(a).unwrap().planned, "the plan is made");
    s.frame(t + 80);
    let going: Vec<UnitId> = Scene::moving(&s).into_iter().flat_map(|m| m.units).collect();
    assert!(
        marked && going.is_empty(),
        "the selected stack's reach is marked: {marked}; still shown going after the person's order: {going:?}"
    );
}

// ---------------------------------------------------------------- the labels

/// How wide the End turn button and Undo are, and how wide they would like
/// to be, with `label` on the first.
fn turn_row_widths(s: &mut Session, label: &str) -> [(i32, i32); 2] {
    let (end, undo) = (widget(s, END_TURN_ID), widget(s, UNDO_ID));
    s.screen.ui.set_text(end, label);
    s.screen.settle();
    [end, undo].map(|w| (s.screen.ui.rect(w).w, s.screen.ui.preferred_size_free(w).pref.w))
}

/// The review's finding, fixed: the labels rounds put on the End turn
/// button did not fit it ("The round plays...", "Yellow's orders done":
/// cut, and Undo beside it squeezed to "Un..."). They are shorter now.
#[test]
fn the_rounds_labels_fit_the_end_turn_button() {
    let mut cut = Vec::new();
    for (w, h, dpi) in [(1024, 768, 96), (2048, 1536, 192)] {
        let g = game("passive", |g| {
            put(g, Side::Red, "soldiers", Hex::new(10, 5));
        });
        let options = Options { show_moves: true, ..Options::manual() };
        let mut s = Session::with_options(&assets(), Size::new(w, h), dpi, g, options).unwrap();
        // The turns' labels, to compare: they fit (the longest, "Waiting
        // for yellow", but for a pixel of Undo).
        for label in ["End turn (12)", "End yellow's turn", "Waiting for yellow", "Start turn", "Game over"] {
            let [(has, wants), (undo_has, undo_wants)] = turn_row_widths(&mut s, label);
            eprintln!("{w}x{h} turns \"{label}\": {has} px of {wants}, Undo {undo_has} px of {undo_wants}");
            assert!(has + 2 >= wants && undo_has + 2 >= undo_wants, "{w}x{h} \"{label}\"");
        }
        for label in ["Orders done (12)", "Orders done", "Playing the round", "Waiting for yellow"] {
            let [(has, wants), (undo_has, undo_wants)] = turn_row_widths(&mut s, label);
            eprintln!("{w}x{h} rounds \"{label}\": {has} px of {wants}, Undo {undo_has} px of {undo_wants}");
            if has + 2 < wants || undo_has + 2 < undo_wants {
                cut.push(format!("{w}x{h} \"{label}\": {has} px of {wants}, Undo {undo_has} px of {undo_wants}"));
            }
        }
    }
    assert!(cut.is_empty(), "{cut:#?}");
}

// ---------------------------------------------------------------- plans and their outlines

/// Undo takes a plan back, and its outline with it.
#[test]
fn undo_takes_the_outline_back() {
    let mut s = session(game("passive", |g| {
        put(g, Side::Red, "soldiers", Hex::new(10, 5));
    }));
    click_hex(&mut s, Hex::new(10, 5));
    click_hex(&mut s, Hex::new(10, 9));
    // (The stack stays selected: its way is ringed, the selected stack's.)
    assert_eq!(Scene::rings(&s).len(), 5, "{:?}", Scene::rings(&s));
    push_widget(&mut s, UNDO_ID);
    assert!(Scene::outlined(&s).is_empty(), "{:?}", Scene::outlined(&s));
    assert_eq!(Scene::rings(&s).len(), 1, "{:?}", Scene::rings(&s));
    assert_eq!(button(&s), "Orders done (2)");
    // Selected again, as after an undo in turns.
    assert_eq!(s.screen.game.selection().len(), 1);
}

/// A stack sent far by way of a stop: selected, the map outlines its stops
/// (in turns); in rounds too?
#[test]
fn a_route_with_stops_is_outlined_in_rounds_as_in_turns() {
    let mut seen = Vec::new();
    for mode in [Mode::Turns, Mode::Rounds] {
        let mut a = None;
        let mut s = session(game_with("passive", mode, |g| a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5)))));
        let a = a.unwrap();
        let (stop, end) = (Hex::new(10, 20), Hex::new(25, 20));
        s.screen.game.apply(Order::GoTo { units: vec![a], to: end, via: vec![stop] }).unwrap();
        let at = pos(&s, a);
        click_hex(&mut s, at);
        assert_eq!(s.screen.game.selection(), [a], "{mode:?}");
        // (In rounds the selected stack's own orders are ringed in a
        // color of their own, the others' outlined.)
        let mut outlined = Scene::outlined(&s).to_vec();
        outlined.extend(Scene::rings(&s).into_iter().map(|(hex, _)| hex));
        seen.push((mode, outlined.contains(&stop), outlined.contains(&end)));
    }
    assert_eq!(seen, [(Mode::Turns, true, true), (Mode::Rounds, true, true)]);
}

// ---------------------------------------------------------------- a temple

/// The same game with a temple at (10, 8).
fn game_with_temple(mode: Mode, place: impl FnOnce(&mut Game)) -> Game {
    let mut rows = vec!["g".repeat(30); 30];
    rows[5] = format!("{}c{}", "g".repeat(5), "g".repeat(24));
    rows[24] = format!("{}c{}", "g".repeat(24), "g".repeat(5));
    rows[10] = format!("{}t{}", "g".repeat(8), "g".repeat(21));
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 5\nmoves 12\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\ncity 24 24\nname Bluemoor\nside blue\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
    place(&mut game);
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    (setup.mode, setup.fog, setup.combat.chance) = (mode, Fog::Off, 0);
    game.with_setup(setup)
}

/// FINDING: a stack moved onto a temple is offered its blessing (or seeks
/// it unasked: Options) in turns; in rounds a stack that a plan brought to
/// a temple is neither asked nor blessed, whatever the option says.
#[test]
fn a_stack_that_comes_to_a_temple_is_offered_its_blessing_in_rounds_as_in_turns() {
    let mut seen = Vec::new();
    for (mode, always) in [(Mode::Turns, false), (Mode::Turns, true), (Mode::Rounds, false), (Mode::Rounds, true)] {
        let mut a = None;
        let g = game_with_temple(mode, |g| a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5))));
        let a = a.unwrap();
        let temple = Hex::new(10, 8);
        let mut s = session_with(g, Options { show_moves: true, always_bless: always, ..Options::manual() });
        click_hex(&mut s, Hex::new(10, 5));
        click_hex(&mut s, temple);
        if mode == Mode::Rounds {
            s.end_turn();
            let t =
                frames(&mut s, 1000, 60_000, 40, |s| s.screen.game.turn() == 2 && button(s).starts_with("Orders done"));
            frames(&mut s, t, t + 2000, 40, |_| false);
        }
        assert_eq!(pos(&s, a), temple, "{mode:?}");
        let asked = s.temple().is_some();
        let blessed = s.screen.game.map.units().get(a).unwrap().blessed_at(temple);
        seen.push((mode, always, asked || blessed));
    }
    let all = [
        (Mode::Turns, false, true),
        (Mode::Turns, true, true),
        (Mode::Rounds, false, true),
        (Mode::Rounds, true, true),
    ];
    assert_eq!(seen, all, "(mode, the option, asked or blessed)");
}

// ---------------------------------------------------------------- the view

/// The person second in the round's order (the computer first): the map is
/// back where they were looking once the round has played out.
#[test]
fn the_map_comes_back_for_a_person_who_is_not_first() {
    let mut g = game("passive", |g| {
        put(g, Side::Red, "soldiers", Hex::new(20, 20));
    });
    let mut setup = g.setup().clone();
    setup.seats.reverse();
    g = g.with_setup(setup);
    let mut s = session(g);
    let t = frames(&mut s, 0, 5000, 40, |s| button(s).starts_with("Orders done"));
    assert_eq!(s.screen.game.order(), [Side::Blue, Side::Red]);
    click_hex(&mut s, Hex::new(20, 20));
    click_hex(&mut s, Hex::new(20, 25));
    s.screen.view.center_on(Hex::new(6, 6));
    let looked = s.screen.view.middle();
    s.end_turn();
    let mut moved = false;
    let mut u = t;
    while u < t + 60_000 && !(s.screen.game.turn() == 2 && button(&s).starts_with("Orders done")) {
        u += 40;
        s.frame(u);
        moved |= s.screen.view.middle() != looked;
    }
    assert!(moved, "the map went to the stack going");
    assert_eq!(s.screen.game.turn(), 2);
    assert_eq!(s.screen.view.middle(), looked);
}

// ---------------------------------------------------------------- scripts

/// `turn`, a round in one step: with the computer first in the round's
/// order, and in hotseat (the next person's orders, then the round).
#[test]
fn a_round_in_one_step_whoever_is_first_and_in_hotseat() {
    let mut a = None;
    let mut g = game("passive", |g| a = Some(put(g, Side::Red, "soldiers", Hex::new(20, 20))));
    let mut setup = g.setup().clone();
    setup.seats.reverse();
    g = g.with_setup(setup);
    let a = a.unwrap();
    let mut s = session(g);
    frames(&mut s, 0, 5000, 40, |s| button(s).starts_with("Orders done"));
    click_hex(&mut s, Hex::new(20, 20));
    click_hex(&mut s, Hex::new(20, 25));
    assert!(Scene::next_turn(&mut s));
    assert_eq!((s.screen.game.turn(), s.screen.game.player()), (2, Side::Red));
    assert!(!s.screen.game.playing_out() && button(&s).starts_with("Orders done"), "{}", button(&s));
    assert_eq!(pos(&s, a), Hex::new(20, 25));
    hexlords::script::run(&mut s, "turn*3,wait:500").unwrap();
    assert_eq!((s.screen.game.turn(), s.screen.game.player()), (5, Side::Red));
    // Hotseat: red's orders, then blue's and the round.
    let mut s = session(game("person", |g| {
        put(g, Side::Red, "soldiers", Hex::new(10, 5));
    }));
    hexlords::script::run(&mut s, "press:begin_turn,click:10:5,click:10:8,turn").unwrap();
    assert_eq!((s.screen.game.turn(), s.screen.game.player(), s.covered()), (1, Side::Blue, false));
    hexlords::script::run(&mut s, "turn").unwrap();
    assert_eq!((s.screen.game.turn(), s.screen.game.player(), s.covered()), (2, Side::Red, false));
    assert!(button(&s).starts_with("Orders done"), "{}", button(&s));
    // (Two turns a round, in the round's draw from round 2.)
    hexlords::script::run(&mut s, "turn*4,wait:300").unwrap();
    let game = &s.screen.game;
    assert_eq!((game.turn(), game.player()), (4, game.order()[0]));
}

/// FINDING: the round is shown again from the moment the cover goes, while
/// the person's log is open over the map and their turn has not started
/// (Start turn): its moves go by behind the log.
#[test]
fn the_round_is_shown_again_once_the_log_is_closed() {
    let (mut s, t, [a, _]) = hotseat_with_two_battles(Fog::Off);
    push_widget(&mut s, BEGIN_TURN_ID);
    assert_eq!(button(&s), "Start turn", "red's log is open: the battles are news");
    let log = s.screen.area(widget(&s, hexlords::log_window::LOG_WINDOW_ID));
    let mut behind = Vec::new();
    for i in 1..=10 {
        s.frame(t + i * 20);
        for m in Scene::moving(&s) {
            if m.units.contains(&a) {
                s.screen.view.center_on(Hex::new(10, 7));
                let p = pixel_of(&s, m.hex);
                behind.push((m.hex, log.contains(p)));
            }
        }
    }
    assert_eq!(button(&s), "Start turn");
    assert!(behind.is_empty(), "red's stack shown going before Start turn (hex, under the log's window): {behind:?}");
}

// ---------------------------------------------------------------- fog: the round shown again

/// FINDING: with fog of war, the round shown again to a person shows them
/// another side's stack going where they never saw it: every tick is
/// judged by what they see once the round is over (the land their stack
/// came to see at the fifth tick shows who left it at the first).
#[test]
fn the_round_shown_again_shows_only_what_the_person_saw() {
    let (mut a, mut b) = (None, None);
    let mut g = game("person", |g| {
        a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5)));
        b = Some(put(g, Side::Blue, "soldiers", Hex::new(10, 12)));
    });
    let mut setup = g.setup().clone();
    setup.fog = Fog::On;
    g = g.with_setup(setup);
    let (a, b) = (a.unwrap(), b.unwrap());
    let mut s = session(g);
    // Red goes five hexes east; blue, seven hexes east of red, six hexes
    // east: red is never within two hexes of blue's stack.
    push_widget(&mut s, BEGIN_TURN_ID);
    assert!(!s.screen.game.sight(Side::Red).sees(Hex::new(10, 12)));
    click_hex(&mut s, Hex::new(10, 5));
    click_hex(&mut s, Hex::new(10, 10));
    s.end_turn();
    push_widget(&mut s, BEGIN_TURN_ID);
    click_hex(&mut s, Hex::new(10, 12));
    click_hex(&mut s, Hex::new(10, 18));
    s.end_turn();
    // As it plays out, tick by tick: red's stack never sees blue's.
    let mut t = 1000;
    let mut near = u32::MAX;
    while t < 60_000 && !s.covered() {
        t += 40;
        s.frame(t);
        near = near.min(pos(&s, a).distance(pos(&s, b)));
    }
    assert!(s.covered() && near > 2, "red was never nearer blue's stack than {near} hexes");
    assert_eq!((pos(&s, a), pos(&s, b)), (Hex::new(10, 10), Hex::new(10, 18)));
    // Red is shown the round.
    push_widget(&mut s, BEGIN_TURN_ID);
    let mut shown: Vec<Hex> = Vec::new();
    for i in 1..100 {
        s.frame(t + i * 20);
        let sight = Scene::sight(&s);
        for m in Scene::moving(&s) {
            if m.units.contains(&b) && sight.sees(m.hex) && !shown.contains(&m.hex) {
                shown.push(m.hex);
            }
        }
    }
    assert!(shown.is_empty(), "red is shown blue's stack, which it never saw, at {shown:?}");
}

// ---------------------------------------------------------------- a full stack

/// FINDING (the rules' and the screen's): onto a full stack of the
/// person's own, a move is refused and a window says why (turns); in rounds
/// the same click is "Planned: they go there", and then they never do, and
/// nothing says so.
#[test]
fn a_plan_onto_a_full_stack_is_refused_as_a_move_is() {
    let mut seen = Vec::new();
    for mode in [Mode::Turns, Mode::Rounds] {
        let mut a = None;
        let full = Hex::new(10, 6);
        let mut s = session(game_with("passive", mode, |g| {
            a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5)));
            for _ in 0..8 {
                put(g, Side::Red, "soldiers", full);
            }
        }));
        let a = a.unwrap();
        // The stack beside the full one, sent onto it (the window asks:
        // join it).
        click_hex(&mut s, Hex::new(10, 5));
        assert_eq!(s.screen.game.selection(), [a]);
        click_hex(&mut s, full);
        if s.asks_mission() {
            push_widget(&mut s, hexlords::session::MISSION_GO_ID);
        }
        let refused = s.refusal().is_some();
        let line = s.unit_line().to_string();
        let mut told = String::new();
        if mode == Mode::Rounds {
            s.end_turn();
            let t = frames(&mut s, 1000, 60_000, 40, |s| s.screen.game.turn() == 2 && !s.screen.game.playing_out());
            frames(&mut s, t, t + 2000, 40, |_| false);
            told = format!("{:?} {:?}", s.log_lines(), s.unit_line());
        }
        eprintln!("{mode:?}: refused {refused}, line {line:?}, at {:?}, after: {told}", pos(&s, a));
        seen.push((mode, refused, pos(&s, a)));
    }
    assert_eq!(seen, [(Mode::Turns, true, Hex::new(10, 5)), (Mode::Rounds, true, Hex::new(10, 5))]);
}

// ---------------------------------------------------------------- the game ends in a round

/// The person takes the last city of the other side as the round plays
/// out: the game is over, and the screen says so.
#[test]
fn a_game_won_as_the_round_plays_out_is_over_at_the_screen() {
    let mut s = session(game("passive", |g| {
        put(g, Side::Red, "soldiers", Hex::new(24, 21));
    }));
    click_hex(&mut s, Hex::new(24, 21));
    click_hex(&mut s, Hex::new(24, 24));
    s.end_turn();
    let t = frames(&mut s, 1000, 60_000, 40, |s| s.screen.game.winner().is_some());
    frames(&mut s, t, t + 5000, 40, |_| false);
    assert_eq!(s.screen.game.winner(), Some(Side::Red));
    assert!(!s.screen.game.playing_out());
    assert_eq!(button(&s), "Game over");
    assert!(s.screen.ui.find(s.screen.ui.root(), turbo_ferret::TbId::new(hexlords::session::GAME_OVER_ID)).is_some());
    assert!(Scene::moving(&s).is_empty());
}

/// The computer takes the person's last city as the round plays out.
#[test]
fn a_game_lost_as_the_round_plays_out_is_over_at_the_screen() {
    let mut g = game("simple", |g| {
        for _ in 0..4 {
            put(g, Side::Blue, "soldiers", Hex::new(5, 8));
        }
    });
    g.watch(true);
    let mut s = session(g);
    let mut t = 0;
    while t < 120_000 && s.screen.game.winner().is_none() {
        t += 40;
        s.frame(t);
        if button(&s) == "Start turn" || button(&s).contains("rders done") {
            push_widget(&mut s, END_TURN_ID);
        }
    }
    frames(&mut s, t, t + 8000, 40, |_| false);
    assert_eq!(s.screen.game.winner(), Some(Side::Blue), "round {}", s.screen.game.turn());
    assert!(!s.screen.game.playing_out());
    assert_eq!(button(&s), "Game over");
    assert!(s.screen.ui.find(s.screen.ui.root(), turbo_ferret::TbId::new(hexlords::session::GAME_OVER_ID)).is_some());
}
