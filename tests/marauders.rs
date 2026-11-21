//! Marauders (the owner, 2026-11-21: "the stack will come under AI control
//! ... and it will roam the map looking for enemy units to kill and cities
//! to conquer on its own"): the computer leads a stack its player handed it,
//! with ordinary orders; after a battle it waits for its player's word;
//! another order ends it; the text protocol; whoever runs the side leads
//! them, at the end of its turn (an agent's, after its orders, and the
//! screen as a person ends theirs, hotseat too); U and the Units menu.

mod common;

use common::{assets, choose, click, pixel_of, push_widget, type_char};
use hexgame::agents::{self, Passive, parse_order};
use hexgame::game::{Game, StackState, Task};
use hexgame::headless::order_line;
use hexgame::marauder::maraud;
use hexgame::orders::{Event, Order};
use hexgame::session::{BEGIN_TURN_ID, MARAUD_ID, MENU_UNITS_ID, Session};
use hexgame::setup::{Fog, Setup};
use hexgame::state::scenario::{self, Scenario};
use hexgame::state::{Hex, Side, UnitId};
use turbo_ferret::Size;

/// Nobody's Greyford, south of the middle, with nobody in it.
const GREYFORD: Hex = Hex::new(20, 15);
/// Where red's marauders start.
const CAMP: Hex = Hex::new(12, 12);

/// 30 x 30 grass: red's Redhold, blue's Bluemoor far to the east, nobody's
/// Greyford in the south. Soldiers (3 strong) go 6 hexes a turn, riders 10;
/// a levy is 1 strong, knights 9. No chance in battle: the stronger wins.
fn game() -> Game {
    let mut rows = vec!["g".repeat(30); 30];
    rows[5] = format!("{}c{}c{}", "g".repeat(5), "g".repeat(19), "g".repeat(4));
    rows[20] = format!("{}c{}", "g".repeat(15), "g".repeat(14));
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nprice 5\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n\
                 unit knights\nname Knights\nstrength 9\nmoves 12\nenter grass 2, city 1\n\
                 unit riders\nname Riders\nstrength 3\nmoves 20\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\ncity 5 25\nname Bluemoor\nside blue\n\
                  city 20 15\nname Greyford\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup.combat.chance = 0;
    game.with_setup(setup)
}

fn put(game: &mut Game, side: Side, kind: &str, hex: Hex, n: usize) -> Vec<UnitId> {
    let kind = game.map.units().kind_named(kind).unwrap();
    (0..n)
        .map(|_| {
            let id = game.map.new_unit_of(side, kind).unwrap();
            game.map.place_unit(id, hex).unwrap();
            id
        })
        .collect()
}

fn pos(game: &Game, id: UnitId) -> Hex {
    game.map.units().get(id).unwrap().pos()
}

fn unit(game: &Game, id: UnitId) -> &hexgame::state::Unit {
    game.map.units().get(id).unwrap()
}

/// Red's turn ends, blue's (it does nothing) too: red's next begins.
fn round(game: &mut Game) -> Vec<Event> {
    let mut events = game.apply(Order::EndTurn).unwrap();
    events.extend(game.apply(Order::EndTurn).unwrap());
    assert_eq!(game.player(), Side::Red);
    events
}

fn stack_state(game: &Game, at: Hex) -> StackState {
    let task = game.tasks(Side::Red).into_iter().find(|t| matches!(t, Task::Stack { at: a, .. } if *a == at));
    match task {
        Some(Task::Stack { state, .. }) => state,
        other => panic!("no stack on {at}: {other:?}"),
    }
}

/// Red soldiers on the camp, marauding.
fn marauders(game: &mut Game, n: usize) -> Vec<UnitId> {
    let red = put(game, Side::Red, "soldiers", CAMP, n);
    game.apply(Order::Maraud { units: red.clone(), on: true }).unwrap();
    red
}

#[test]
fn marauding_is_a_flag_another_order_ends() {
    let mut game = game();
    let red = put(&mut game, Side::Red, "soldiers", CAMP, 2);
    // The computer's move is for marauders only.
    let roam = Order::Roam { units: red.clone(), to: Hex::new(12, 14) };
    assert_eq!(game.apply(roam.clone()).unwrap_err(), "they are not marauding");
    game.apply(Order::Maraud { units: red.clone(), on: true }).unwrap();
    assert!(red.iter().all(|id| unit(&game, *id).marauding));
    assert_eq!(stack_state(&game, CAMP), StackState::Marauding);
    assert!(game.waiting_stacks(Side::Red).is_empty(), "nobody need see to it");
    assert_eq!(hexgame::info::duty_words(unit(&game, red[0])).unwrap(), "marauding: the computer leads it");
    // Its move keeps it marauding, and is not the player's to take back.
    game.apply(roam).unwrap();
    assert!(red.iter().all(|id| unit(&game, *id).marauding && pos(&game, *id) == Hex::new(12, 14)));
    assert!(!game.can_undo());
    // The player's move ends it.
    game.apply(Order::Move { units: red.clone(), to: Hex::new(12, 15) }).unwrap();
    assert!(red.iter().all(|id| !unit(&game, *id).marauding));
    // Undo gives it back.
    game.apply(Order::Undo).unwrap();
    assert!(red.iter().all(|id| unit(&game, *id).marauding));
    // Called back.
    game.apply(Order::Maraud { units: red.clone(), on: false }).unwrap();
    assert!(red.iter().all(|id| !unit(&game, *id).marauding));
}

#[test]
fn a_marauder_strikes_a_stack_it_can_beat_and_then_waits_for_its_players_word() {
    let mut game = game();
    let red = marauders(&mut game, 2);
    let levy = put(&mut game, Side::Blue, "levy", Hex::new(12, 16), 1);
    let report = maraud(&mut game, Side::Red);
    assert!(report.events.iter().any(|e| matches!(e, Event::Battle { won: true, .. })), "{:?}", report.events);
    assert!(game.map.units().get(levy[0]).is_none());
    let at = pos(&game, red[0]);
    assert_eq!(at, Hex::new(12, 16));
    assert!(red.iter().all(|id| unit(&game, *id).fought && unit(&game, *id).marauding));
    assert_eq!(stack_state(&game, at), StackState::Fought);
    assert_eq!(game.waiting_stacks(Side::Red), [at]);
    // The computer leaves it be until its player has had a word with it.
    put(&mut game, Side::Blue, "levy", Hex::new(12, 18), 1);
    let again = maraud(&mut game, Side::Red);
    assert!(again.events.is_empty());
    // Done: it is the computer's again (the next time it leads it).
    game.apply(Order::Done { units: red.clone() }).unwrap();
    assert_eq!(stack_state(&game, at), StackState::Marauding);
    round(&mut game);
    let report = maraud(&mut game, Side::Red);
    assert!(report.events.iter().any(|e| matches!(e, Event::Battle { .. })));
}

#[test]
fn a_marauding_stack_goes_as_one_as_far_as_the_slowest_gets() {
    let mut game = game();
    let mut red = put(&mut game, Side::Red, "soldiers", CAMP, 2);
    red.extend(put(&mut game, Side::Red, "riders", CAMP, 1));
    game.apply(Order::Maraud { units: red.clone(), on: true }).unwrap();
    // Greyford is more than a soldier's turn away (and a levy near it, out
    // of the soldiers' reach, would tempt the riders alone).
    put(&mut game, Side::Blue, "levy", Hex::new(19, 17), 1);
    let report = maraud(&mut game, Side::Red);
    assert!(!report.events.is_empty());
    let at = pos(&game, red[0]);
    assert_ne!(at, CAMP);
    let places: Vec<Hex> = red.iter().map(|id| pos(&game, *id)).collect();
    assert!(places.iter().all(|h| *h == at), "together: {places:?}");
}

#[test]
fn a_marauder_takes_a_city_before_a_stack() {
    let mut game = game();
    let red = put(&mut game, Side::Red, "soldiers", Hex::new(17, 15), 2);
    game.apply(Order::Maraud { units: red.clone(), on: true }).unwrap();
    let levy = put(&mut game, Side::Blue, "levy", Hex::new(16, 17), 1);
    let report = maraud(&mut game, Side::Red);
    assert!(report.events.iter().any(|e| matches!(e, Event::Taken { .. })), "{:?}", report.events);
    assert_eq!(game.map.city_at(GREYFORD).unwrap().side, Side::Red);
    assert!(game.map.units().get(levy[0]).is_some(), "the city first");
}

#[test]
fn a_marauder_leaves_alone_what_it_cannot_beat_and_goes_for_a_city() {
    let mut game = game();
    let red = marauders(&mut game, 2);
    // Knights it cannot beat, near; a levy it can; Greyford, empty, a turn
    // and more away.
    let knights = put(&mut game, Side::Blue, "knights", Hex::new(12, 14), 1);
    let levy = put(&mut game, Side::Blue, "levy", Hex::new(14, 10), 1);
    let report = maraud(&mut game, Side::Red);
    assert!(game.map.units().get(knights[0]).is_some(), "not the knights");
    assert!(game.map.units().get(levy[0]).is_none(), "the levy");
    assert_eq!(report.events.iter().filter(|e| matches!(e, Event::Battle { .. })).count(), 1);
    // The next turn (its player said Done): toward Greyford, and in.
    game.apply(Order::Done { units: red.clone() }).unwrap();
    let mut taken = false;
    for _ in 0..3 {
        round(&mut game);
        let report = maraud(&mut game, Side::Red);
        taken |= report.events.iter().any(|e| matches!(e, Event::Taken { .. }));
    }
    assert!(taken);
    assert_eq!(game.map.city_at(GREYFORD).unwrap().side, Side::Red);
    assert!(red.iter().all(|id| unit(&game, *id).marauding));
}

#[test]
fn a_marauder_attacked_waits_for_its_players_word_through_its_turn() {
    let mut game = game();
    let red = marauders(&mut game, 2);
    // Blue's levy attacks it, and loses.
    let levy = put(&mut game, Side::Blue, "levy", Hex::new(12, 14), 1);
    game.apply(Order::EndTurn).unwrap();
    let events = game.apply(Order::Move { units: levy, to: CAMP }).unwrap();
    assert!(events.iter().any(|e| matches!(e, Event::Battle { won: false, .. })));
    game.apply(Order::EndTurn).unwrap();
    // Red's turn: it waits for red's word.
    assert!(red.iter().all(|id| unit(&game, *id).fought));
    assert_eq!(stack_state(&game, CAMP), StackState::Fought);
    assert_eq!(game.waiting_stacks(Side::Red), [CAMP]);
    // Left alone, it goes on at the end of the turn (where the computer
    // leads the marauders).
    assert!(!maraud(&mut game, Side::Red).events.is_empty());
}

#[test]
fn under_total_fog_a_marauder_goes_to_see_more() {
    let mut game = game();
    let red = put(&mut game, Side::Red, "soldiers", CAMP, 1);
    let mut setup = game.setup().clone();
    setup.fog = Fog::Total;
    let mut game = game.with_setup(setup);
    game.apply(Order::Maraud { units: red.clone(), on: true }).unwrap();
    let known = game.sight(Side::Red).known_count();
    let report = maraud(&mut game, Side::Red);
    assert!(report.events.iter().any(|e| matches!(e, Event::Moved { .. })));
    assert!(game.sight(Side::Red).known_count() > known, "it saw more");
}

#[test]
fn an_agents_turn_leads_its_marauders_after_its_orders() {
    let mut game = game();
    let red = marauders(&mut game, 2);
    put(&mut game, Side::Blue, "levy", Hex::new(12, 16), 1);
    // Red is the computer's (a passive player).
    let mut setup = game.setup().clone();
    setup.set_players("red=passive,blue=passive").unwrap();
    let mut game = game.with_setup(setup);
    assert!(red.iter().all(|id| unit(&game, *id).marauding));
    let report = agents::play_turn(&mut game, &mut Passive);
    assert!(report.events.iter().any(|e| matches!(e, Event::Battle { .. })), "{:?}", report.events);
    assert_eq!(game.player(), Side::Blue, "and its turn is over");
}

#[test]
fn the_text_protocol_says_maraud_unmaraud_and_roam() {
    let mut game = game();
    let red = put(&mut game, Side::Red, "soldiers", CAMP, 2);
    for (line, order) in [
        ("maraud red.0.0,red.1.0", Order::Maraud { units: red.clone(), on: true }),
        ("unmaraud red.0.0", Order::Maraud { units: vec![red[0]], on: false }),
        ("roam red.0.0,red.1.0 12 14", Order::Roam { units: red.clone(), to: Hex::new(12, 14) }),
    ] {
        assert_eq!(parse_order(&game, line), Ok(Some(order.clone())));
        assert_eq!(order_line(&game, &order), line);
    }
    assert!(parse_order(&game, "roam red.0.0 12").is_err());
    game.apply(parse_order(&game, "maraud red.0.0,red.1.0").unwrap().unwrap()).unwrap();
    assert!(red.iter().all(|id| unit(&game, *id).marauding));
}

// The screen.

fn session(game: Game) -> Session {
    common::with_game(&assets(), Size::new(1024, 768), 96, game).expect("the session starts")
}

fn click_hex(s: &mut Session, hex: Hex) {
    s.screen.view.center_on(hex);
    let p = pixel_of(s, hex);
    click(s, p);
}

#[test]
fn u_hands_a_stack_to_the_computer_at_once() {
    let mut game = game();
    let red = put(&mut game, Side::Red, "soldiers", CAMP, 2);
    let levy = put(&mut game, Side::Blue, "levy", Hex::new(12, 16), 1);
    let mut s = session(game);
    click_hex(&mut s, CAMP);
    type_char(&mut s, 'u');
    let game = &s.screen.game;
    assert!(game.map.units().get(levy[0]).is_none(), "it struck at once");
    assert!(red.iter().all(|id| unit(game, *id).marauding && unit(game, *id).fought));
    assert!(s.battle().is_some(), "the battle is played out, as the person's own are");
    // It needs its player's word: the screen goes to it.
    assert_eq!(game.selected().map(|id| pos(game, id)), Some(Hex::new(12, 16)));
    assert_eq!(s.unit_line(), "2 units fought, marauding: D lets it go on; U calls it back.");
}

#[test]
fn u_again_calls_it_back_and_the_units_menu_has_it() {
    let mut game = game();
    let mut red = put(&mut game, Side::Red, "soldiers", CAMP, 2);
    red.extend(put(&mut game, Side::Red, "riders", CAMP, 1));
    let mut s = session(game);
    click_hex(&mut s, CAMP);
    choose(&mut s, MENU_UNITS_ID, MARAUD_ID);
    // Off toward Greyford, as one.
    let at = pos(&s.screen.game, red[0]);
    assert_ne!(at, CAMP);
    assert!(red.iter().all(|id| pos(&s.screen.game, *id) == at && unit(&s.screen.game, *id).marauding));
    // A click on them takes them all, though only the riders have moves
    // left: they go as one.
    click_hex(&mut s, at);
    assert_eq!(s.screen.game.selection().len(), 3);
    assert_eq!(s.unit_line(), "3 units, marauding: the computer leads it (U: call it back; a move ends it).");
    type_char(&mut s, 'u');
    assert!(red.iter().all(|id| !unit(&s.screen.game, *id).marauding));
    assert_eq!(s.screen.game.selection().len(), 3, "still selected, to be sent somewhere");
}

#[test]
fn the_screen_leads_a_persons_marauders_at_the_end_of_their_turn() {
    let mut game = game();
    let red = marauders(&mut game, 2);
    let mut s = session(game);
    // Red's first turn: they wait (red could still give them other orders);
    // at its end they go.
    assert_eq!(pos(&s.screen.game, red[0]), CAMP);
    assert!(s.next_turn());
    let first = pos(&s.screen.game, red[0]);
    assert_ne!(first, CAMP);
    assert!(s.next_turn());
    assert_ne!(pos(&s.screen.game, red[0]), first, "and again at the end of the next");
}

#[test]
fn in_hotseat_each_persons_marauders_go_as_their_turn_ends() {
    let mut game = game();
    let blue = put(&mut game, Side::Blue, "soldiers", Hex::new(8, 20), 2);
    let mut setup = game.setup().clone();
    setup.set_players("red=person,blue=person").unwrap();
    let mut game = game.with_setup(setup);
    // Blue hands its stack to the computer in its turn.
    game.apply(Order::EndTurn).unwrap();
    game.apply(Order::Maraud { units: blue.clone(), on: true }).unwrap();
    game.apply(Order::EndTurn).unwrap();
    let mut s = session(game);
    assert!(s.covered(), "red's turn, behind the cover");
    push_widget(&mut s, BEGIN_TURN_ID);
    assert_eq!(pos(&s.screen.game, blue[0]), Hex::new(8, 20), "not in red's turn");
    s.end_turn();
    assert!(s.covered(), "blue's turn, behind the cover");
    assert_eq!(pos(&s.screen.game, blue[0]), Hex::new(8, 20), "nor as blue's begins");
    push_widget(&mut s, BEGIN_TURN_ID);
    s.end_turn();
    assert_ne!(pos(&s.screen.game, blue[0]), Hex::new(8, 20), "blue's marauders went as its turn ended");
}
