//! Patrols (the owner, 2026-11-21: "a series of waypoints and the stack will
//! move between them and repeat"): back and forth, or round a loop; whether
//! it attacks, and at what odds at least; after a battle it waits for its
//! player's word; another order ends it; the text protocol; and the screen
//! (P, Shift+click the waypoints, the window that asks how it fights).

mod common;

use common::{assets, choose, click, pixel_of, press, push_widget, shift_click, type_char};
use hexlords::agents::parse_order;
use hexlords::game::{Game, StackState, Task};
use hexlords::headless::order_line;
use hexlords::orders::{Event, Order};
use hexlords::patrol_window::{PATROL_CANCEL_ID, PATROL_OK_ID, choice_id};
use hexlords::screen::Scene;
use hexlords::session::{MENU_UNITS_ID, PATROL_ID, Pick, Session};
use hexlords::setup::{Fog, Setup};
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{Hex, Side, UnitId};
use turbo_ferret::{Size, SpecialKey};

const REDHOLD: Hex = Hex::new(5, 5);
const BLUEMOOR: Hex = Hex::new(5, 25);
/// Three hexes along row 10, six apart: a soldier's turn from one to the next.
const WEST: Hex = Hex::new(10, 5);
const MIDDLE: Hex = Hex::new(10, 11);
const EAST: Hex = Hex::new(10, 17);
/// Six hexes south of the middle one: a triangle with the others.
const SOUTH: Hex = Hex::new(16, 11);

/// 30 x 30 grass: red's Redhold, blue's Bluemoor far to the east. Soldiers
/// (3 strong) go 6 hexes a turn; a levy is 1 strong, knights 9. No chance
/// in battle: the stronger wins.
fn game() -> Game {
    let mut rows = vec!["g".repeat(30); 30];
    rows[5] = format!("{}c{}c{}", "g".repeat(5), "g".repeat(19), "g".repeat(4));
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nprice 5\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n\
                 unit knights\nname Knights\nstrength 9\nmoves 12\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\ncity 5 25\nname Bluemoor\nside blue\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup.combat.chance = 0;
    game.with_setup(setup)
}

/// New units of `side`, of `kind`, on `hex`.
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

fn unit(game: &Game, id: UnitId) -> &hexlords::state::Unit {
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

fn battles(events: &[Event]) -> usize {
    events.iter().filter(|e| matches!(e, Event::Battle { .. })).count()
}

#[test]
fn a_patrol_goes_back_and_forth() {
    let mut game = game();
    let red = put(&mut game, Side::Red, "soldiers", WEST, 2);
    let order = Order::Patrol { units: red.clone(), waypoints: vec![WEST, MIDDLE, EAST], attack: None };
    game.apply(order).unwrap();
    // This turn's part at once: on from where they stand to the next.
    assert!(red.iter().all(|id| pos(&game, *id) == MIDDLE));
    assert_eq!(stack_state(&game, MIDDLE), StackState::Patrolling);
    assert!(game.waiting_stacks(Side::Red).is_empty(), "a patrol waits for nobody");
    // On at the end of each of red's turns, with the moves left (none, at
    // the end of this one).
    let mut seen = Vec::new();
    for _ in 0..6 {
        round(&mut game);
        seen.push(pos(&game, red[0]));
    }
    assert_eq!(seen, [MIDDLE, EAST, MIDDLE, WEST, MIDDLE, EAST]);
    assert!(red.iter().all(|id| pos(&game, *id) == pos(&game, red[0])), "together");
}

#[test]
fn a_patrol_whose_last_waypoint_is_the_first_goes_round() {
    let mut game = game();
    let red = put(&mut game, Side::Red, "soldiers", MIDDLE, 1);
    let order = Order::Patrol { units: red.clone(), waypoints: vec![MIDDLE, EAST, SOUTH, MIDDLE], attack: None };
    game.apply(order).unwrap();
    assert_eq!(pos(&game, red[0]), EAST);
    let patrol = unit(&game, red[0]).patrol.clone().unwrap();
    assert!(patrol.round);
    // East to south is more than a turn: it goes on from a waypoint with
    // what movement it has left. Round and round, never back.
    let mut going = vec![patrol.target()];
    for _ in 0..8 {
        round(&mut game);
        let target = unit(&game, red[0]).patrol.as_ref().unwrap().target();
        if going.last() != Some(&target) {
            going.push(target);
        }
    }
    assert_eq!(going[..5], [SOUTH, MIDDLE, EAST, SOUTH, MIDDLE], "{going:?}");
}

#[test]
fn waypoints_have_to_be_ones_the_stack_can_go_to() {
    let mut game = game();
    let red = put(&mut game, Side::Red, "soldiers", WEST, 1);
    let other = put(&mut game, Side::Red, "soldiers", EAST, 1);
    // One place is no patrol.
    let why = game.apply(Order::Patrol { units: red.clone(), waypoints: vec![WEST], attack: None }).unwrap_err();
    assert_eq!(why, "a patrol needs two places at least");
    // Not into another side's city.
    let into = Order::Patrol { units: red.clone(), waypoints: vec![WEST, BLUEMOOR], attack: None };
    assert_eq!(game.apply(into).unwrap_err(), "a patrol goes into no city but its side's");
    // Its own is fine.
    assert_eq!(game.check_waypoints(Side::Red, &red, &[WEST, REDHOLD]), Ok(()));
    // One stack, from one hex.
    let both: Vec<UnitId> = red.iter().chain(&other).copied().collect();
    assert_eq!(
        game.check_waypoints(Side::Red, &both, &[WEST, MIDDLE]).unwrap_err(),
        "the units are not all on one hex"
    );
    // Odds are in a hundred.
    let odds = Order::Patrol { units: red.clone(), waypoints: vec![WEST, MIDDLE], attack: Some(120) };
    assert!(game.apply(odds).is_err());
    assert!(unit(&game, red[0]).patrol.is_none());
}

#[test]
fn under_total_fog_waypoints_are_on_land_the_side_has_seen() {
    let mut game = game();
    let mut setup = game.setup().clone();
    setup.fog = Fog::Total;
    let mut game = {
        let red = put(&mut game, Side::Red, "soldiers", WEST, 1);
        let game = game.with_setup(setup);
        assert_eq!(game.map.units().get(red[0]).map(|u| u.pos()), Some(WEST));
        game
    };
    let red: Vec<UnitId> = game.map.units_at(WEST).to_vec();
    let far = Order::Patrol { units: red.clone(), waypoints: vec![WEST, EAST], attack: None };
    assert_eq!(game.apply(far).unwrap_err(), hexlords::game::UNSEEN);
    let near = Order::Patrol { units: red, waypoints: vec![WEST, Hex::new(10, 7)], attack: None };
    assert!(game.apply(near).is_ok());
}

#[test]
fn a_patrol_attacks_what_it_can_beat_at_its_odds_and_never_without_them() {
    // Never: it goes by a levy it could beat.
    let mut game = self::game();
    let red = put(&mut game, Side::Red, "soldiers", WEST, 2);
    let levy = put(&mut game, Side::Blue, "levy", Hex::new(12, 8), 1);
    let order = Order::Patrol { units: red.clone(), waypoints: vec![WEST, MIDDLE], attack: None };
    let events = game.apply(order).unwrap();
    assert_eq!(battles(&events), 0);
    assert!(game.map.units().get(levy[0]).is_some());
    assert!(red.iter().all(|id| unit(&game, *id).patrol.is_some() && !unit(&game, *id).fought));

    // At 50% or better: the levy, at once; then it waits for its player's
    // word (a task that needs orders, and Next stack goes to it).
    let mut game = game_with_levy();
    let red: Vec<UnitId> = game.map.units_at(WEST).to_vec();
    let order = Order::Patrol { units: red.clone(), waypoints: vec![WEST, MIDDLE], attack: Some(50) };
    let events = game.apply(order).unwrap();
    assert_eq!(battles(&events), 1);
    assert!(events.iter().any(|e| matches!(e, Event::Battle { won: true, .. })));
    let at = pos(&game, red[0]);
    assert_eq!(at, Hex::new(12, 8), "they went in");
    assert!(red.iter().all(|id| unit(&game, *id).fought && unit(&game, *id).patrol.is_some()));
    assert_eq!(stack_state(&game, at), StackState::Fought);
    assert_eq!(game.waiting_stacks(Side::Red), [at]);
    assert!(!game.can_undo(), "a battle is not to be taken back");

    // Knights are too strong for it: it goes by them.
    let mut game = self::game();
    let red = put(&mut game, Side::Red, "soldiers", WEST, 2);
    let knights = put(&mut game, Side::Blue, "knights", Hex::new(12, 8), 1);
    assert!(game.battle_odds(&red, Hex::new(12, 8)).unwrap() < 50);
    let order = Order::Patrol { units: red.clone(), waypoints: vec![WEST, MIDDLE], attack: Some(50) };
    assert_eq!(battles(&game.apply(order).unwrap()), 0);
    assert!(game.map.units().get(knights[0]).is_some());
}

/// Red soldiers on `WEST`, and a blue levy within their reach.
fn game_with_levy() -> Game {
    let mut game = game();
    put(&mut game, Side::Red, "soldiers", WEST, 2);
    put(&mut game, Side::Blue, "levy", Hex::new(12, 8), 1);
    game
}

#[test]
fn a_patrol_fights_once_a_turn_at_most_at_the_end_of_its_turn() {
    let mut game = game();
    let red = put(&mut game, Side::Red, "soldiers", WEST, 2);
    let order = Order::Patrol { units: red.clone(), waypoints: vec![WEST, MIDDLE, EAST], attack: Some(50) };
    game.apply(order).unwrap();
    assert_eq!(pos(&game, red[0]), MIDDLE);
    // Two levies near the way on: one battle, the likeliest won (alike: the
    // nearest).
    let near = put(&mut game, Side::Blue, "levy", Hex::new(11, 13), 1);
    let far = put(&mut game, Side::Blue, "levy", Hex::new(9, 15), 1);
    // (None at the end of this turn: it has no moves left.)
    assert_eq!(battles(&round(&mut game)), 0);
    let events = round(&mut game);
    assert_eq!(battles(&events), 1);
    assert!(game.map.units().get(near[0]).is_none());
    assert!(game.map.units().get(far[0]).is_some());
    assert_eq!(stack_state(&game, Hex::new(11, 13)), StackState::Fought);
}

#[test]
fn after_a_battle_done_lets_it_go_on_and_left_alone_it_goes_on_next_turn() {
    // Done.
    let mut game = game_with_levy();
    let red: Vec<UnitId> = game.map.units_at(WEST).to_vec();
    game.apply(Order::Patrol { units: red.clone(), waypoints: vec![WEST, MIDDLE], attack: Some(50) }).unwrap();
    let at = pos(&game, red[0]);
    // (It has no moves left to go on with this turn: an attack spends them.)
    assert_eq!(game.apply(Order::Done { units: red.clone() }), Ok(Vec::new()));
    assert!(red.iter().all(|id| !unit(&game, *id).fought && unit(&game, *id).patrol.is_some()));
    assert_eq!(stack_state(&game, at), StackState::Patrolling);
    assert!(game.waiting_stacks(Side::Red).is_empty());
    round(&mut game);
    round(&mut game);
    assert_ne!(pos(&game, red[0]), at, "on its way again");

    // Left alone: the same, at the end of the next turn.
    let mut game = game_with_levy();
    let red: Vec<UnitId> = game.map.units_at(WEST).to_vec();
    game.apply(Order::Patrol { units: red.clone(), waypoints: vec![WEST, MIDDLE], attack: Some(50) }).unwrap();
    let at = pos(&game, red[0]);
    round(&mut game);
    round(&mut game);
    assert_ne!(pos(&game, red[0]), at, "on its way again");
    assert!(!unit(&game, red[0]).fought);
}

#[test]
fn a_patrol_attacked_waits_a_turn_for_its_players_word() {
    let mut game = game();
    let red = put(&mut game, Side::Red, "soldiers", MIDDLE, 2);
    game.apply(Order::Patrol { units: red.clone(), waypoints: vec![MIDDLE, EAST], attack: None }).unwrap();
    assert_eq!(pos(&game, red[0]), EAST);
    // Blue's levy attacks them, and loses.
    let levy = put(&mut game, Side::Blue, "levy", Hex::new(10, 20), 1);
    game.apply(Order::EndTurn).unwrap();
    assert_eq!(game.player(), Side::Blue);
    let events = game.apply(Order::Move { units: levy.clone(), to: EAST }).unwrap();
    assert!(events.iter().any(|e| matches!(e, Event::Battle { won: false, .. })));
    assert!(red.iter().all(|id| unit(&game, *id).fought));
    // Red's turn: the patrol has not gone on; it waits for red's word.
    game.apply(Order::EndTurn).unwrap();
    assert_eq!(pos(&game, red[0]), EAST);
    assert_eq!(stack_state(&game, EAST), StackState::Fought);
    assert_eq!(game.waiting_stacks(Side::Red), [EAST]);
    // Done: it goes on, at once.
    let events = game.apply(Order::Done { units: red.clone() }).unwrap();
    assert!(!events.is_empty());
    assert_eq!(pos(&game, red[0]), MIDDLE);
    assert!(red.iter().all(|id| unit(&game, *id).patrol.is_some() && !unit(&game, *id).fought));
}

#[test]
fn a_patrol_with_no_way_on_waits_for_orders() {
    let mut game = game();
    let red = put(&mut game, Side::Red, "soldiers", WEST, 1);
    game.apply(Order::Patrol { units: red.clone(), waypoints: vec![WEST, MIDDLE], attack: None }).unwrap();
    // Blue knights stand on the waypoint: it stops short, and waits.
    put(&mut game, Side::Blue, "knights", WEST, 1);
    round(&mut game);
    let events = round(&mut game);
    assert!(events.iter().any(|e| matches!(e, Event::Stalled { .. })), "{events:?}");
    let at = pos(&game, red[0]);
    assert_eq!(stack_state(&game, at), StackState::Stalled);
    let line = hexlords::info::task_line(&game, &Task::Stack { at, units: 1, state: StackState::Stalled });
    assert_eq!(
        line,
        format!("Soldiers at row {}, column {}: stopped on patrol, on the way to row 10, column 5", at.row, at.col)
    );
    assert!(unit(&game, red[0]).patrol.is_some(), "it keeps its patrol");
}

#[test]
fn another_order_ends_a_patrol_and_undo_gives_it_back() {
    let mut game = game();
    let red = put(&mut game, Side::Red, "soldiers", MIDDLE, 1);
    game.apply(Order::Patrol { units: red.clone(), waypoints: vec![MIDDLE, EAST], attack: None }).unwrap();
    // Attacked in blue's turn, it waits in red's with its moves.
    let levy = put(&mut game, Side::Blue, "levy", Hex::new(10, 20), 1);
    game.apply(Order::EndTurn).unwrap();
    game.apply(Order::Move { units: levy, to: EAST }).unwrap();
    game.apply(Order::EndTurn).unwrap();
    assert_eq!((pos(&game, red[0]), unit(&game, red[0]).fought), (EAST, true));
    // A move ends its patrol; Undo gives it back.
    game.apply(Order::Move { units: red.clone(), to: Hex::new(11, 17) }).unwrap();
    assert!(unit(&game, red[0]).patrol.is_none() && !unit(&game, red[0]).fought);
    game.apply(Order::Undo).unwrap();
    assert_eq!(pos(&game, red[0]), EAST);
    assert!(unit(&game, red[0]).patrol.is_some() && unit(&game, red[0]).fought);
    // So do Defend, Go to and marauding.
    game.apply(Order::Defend { units: red.clone(), on: true }).unwrap();
    assert!(unit(&game, red[0]).patrol.is_none());
    game.apply(Order::Patrol { units: red.clone(), waypoints: vec![EAST, MIDDLE], attack: None }).unwrap();
    assert!(!unit(&game, red[0]).defending);
    game.apply(Order::Maraud { units: red.clone(), on: true }).unwrap();
    assert!(unit(&game, red[0]).patrol.is_none() && unit(&game, red[0]).marauding);
}

#[test]
fn a_patrol_that_went_one_leg_can_be_taken_back() {
    let mut game = game();
    let red = put(&mut game, Side::Red, "soldiers", WEST, 1);
    game.apply(Order::Patrol { units: red.clone(), waypoints: vec![WEST, MIDDLE], attack: None }).unwrap();
    assert!(game.can_undo());
    game.apply(Order::Undo).unwrap();
    assert_eq!(pos(&game, red[0]), WEST);
    assert!(unit(&game, red[0]).patrol.is_none());
}

#[test]
fn the_text_protocol_says_the_waypoints_and_the_odds() {
    let mut game = game();
    let red = put(&mut game, Side::Red, "soldiers", WEST, 2);
    let line = "patrol red.0.0,red.1.0 10 5 10 11 10 17 10 5 attack 65";
    assert_eq!(hexlords::agents::unit_name(red[0]), "red.0.0");
    let order = Order::Patrol { units: red.clone(), waypoints: vec![WEST, MIDDLE, EAST, WEST], attack: Some(65) };
    assert_eq!(parse_order(&game, line), Ok(Some(order.clone())));
    assert_eq!(order_line(&game, &order), line);
    let never = Order::Patrol { units: vec![red[0]], waypoints: vec![WEST, MIDDLE], attack: None };
    assert_eq!(parse_order(&game, "patrol red.0.0 10 5 10 11"), Ok(Some(never.clone())));
    assert_eq!(order_line(&game, &never), "patrol red.0.0 10 5 10 11");
    assert!(parse_order(&game, "patrol red.0.0 10 5 10").is_err());
    assert!(parse_order(&game, "patrol red.0.0 10 5 10 11 attack lots").is_err());
    // A program's patrol goes as a person's does.
    game.apply(parse_order(&game, "patrol red.0.0,red.1.0 10 5 10 11").unwrap().unwrap()).unwrap();
    assert_eq!(pos(&game, red[0]), MIDDLE);
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

fn shift_click_hex(s: &mut Session, hex: Hex) {
    s.screen.view.center_on(hex);
    let p = pixel_of(s, hex);
    shift_click(s, p);
}

#[test]
fn p_then_shift_click_the_waypoints_click_the_last_and_say_how_it_fights() {
    let mut game = game();
    let red = put(&mut game, Side::Red, "soldiers", WEST, 2);
    let mut s = session(game);
    click_hex(&mut s, WEST);
    type_char(&mut s, 'p');
    assert_eq!(s.picking(), Some(Pick::Patrol));
    assert!(s.unit_line().starts_with("Patrol from here: click where it goes and back"), "{}", s.unit_line());
    // Another side's city is no waypoint: the line says why.
    shift_click_hex(&mut s, BLUEMOOR);
    assert_eq!(
        s.unit_line(),
        "Patrol: a patrol goes into no city but its side's. Click another hex (Esc: never mind)."
    );
    shift_click_hex(&mut s, MIDDLE);
    assert!(s.unit_line().starts_with("Patrol from here, 1 waypoint so far: click the last"), "{}", s.unit_line());
    shift_click_hex(&mut s, EAST);
    assert_eq!(s.outlined(), [MIDDLE, EAST], "the map outlines them");
    // The stack's own hex last: round and round. A window asks how it
    // fights.
    click_hex(&mut s, WEST);
    assert_eq!(s.picking(), None);
    let window = s.patrol_window().expect("it asks");
    assert_eq!(window.waypoints, [WEST, MIDDLE, EAST, WEST]);
    assert!(red.iter().all(|id| unit(&s.screen.game, *id).patrol.is_none()), "not before it is told");
    // A choice is pressed (never attack, at first); Patrol gives the order.
    assert_eq!(window.chosen(), None);
    push_widget(&mut s, &choice_id(2));
    assert_eq!(s.patrol_window().map(|w| w.chosen()), Some(Some(75)), "chosen, not yet ordered");
    assert!(red.iter().all(|id| unit(&s.screen.game, *id).patrol.is_none()));
    push_widget(&mut s, PATROL_OK_ID);
    assert!(s.patrol_window().is_none());
    let patrol = unit(&s.screen.game, red[0]).patrol.clone().expect("patrolling");
    assert_eq!((patrol.attack, patrol.round, patrol.stops.len()), (Some(75), true, 3));
    assert!(red.iter().all(|id| pos(&s.screen.game, *id) == MIDDLE), "this turn's part at once");
}

#[test]
fn the_window_can_be_let_go_and_the_units_menu_starts_a_patrol() {
    let mut game = game();
    let red = put(&mut game, Side::Red, "soldiers", WEST, 1);
    let mut s = session(game);
    click_hex(&mut s, WEST);
    choose(&mut s, MENU_UNITS_ID, PATROL_ID);
    assert_eq!(s.picking(), Some(Pick::Patrol));
    // One click: between here and there.
    click_hex(&mut s, MIDDLE);
    assert_eq!(s.patrol_window().map(|w| w.waypoints.clone()), Some(vec![WEST, MIDDLE]));
    press(&mut s, SpecialKey::Esc);
    assert!(s.patrol_window().is_none());
    assert!(s.screen.game.selected().is_some(), "only the window went");
    type_char(&mut s, 'p');
    click_hex(&mut s, MIDDLE);
    push_widget(&mut s, PATROL_CANCEL_ID);
    assert!(s.patrol_window().is_none());
    assert!(unit(&s.screen.game, red[0]).patrol.is_none());
    assert_eq!(pos(&s.screen.game, red[0]), WEST);
}

#[test]
fn a_patrolling_stack_says_so_and_shows_its_waypoints() {
    let mut game = game();
    let red = put(&mut game, Side::Red, "soldiers", WEST, 2);
    game.apply(Order::Patrol { units: red.clone(), waypoints: vec![WEST, MIDDLE, EAST], attack: None }).unwrap();
    let mut s = session(game);
    click_hex(&mut s, MIDDLE);
    assert_eq!(s.screen.game.selection().len(), 2);
    assert_eq!(
        s.unit_line(),
        "2 units, patrolling 3 waypoints, never attacks, back and forth (P: another patrol; a move ends it)."
    );
    assert_eq!(s.outlined(), [WEST, MIDDLE, EAST]);
    let task = s.screen.game.tasks(Side::Red).into_iter().find(|t| matches!(t, Task::Stack { .. })).unwrap();
    let line = hexlords::info::task_line(&s.screen.game, &task);
    assert_eq!(line, "2 units at row 10, column 11: patrolling 3 waypoints, never attacks");
}

#[test]
fn the_help_says_p_and_u_and_the_orders_topic_explains_them() {
    let s = session(game());
    let keys: Vec<String> = s.screen.help().iter().flat_map(|h| h.rows.iter().map(|(k, _)| k.clone())).collect();
    assert!(keys.contains(&"P".to_string()) && keys.contains(&"U".to_string()), "{keys:?}");
    let (_, sections) = hexlords::guide::topic(&s.screen.game, "help_orders").expect("the topic");
    let rows: Vec<String> = sections.iter().flat_map(|h| h.rows.iter().map(|(k, _)| k.clone())).collect();
    for title in ["Patrol", "Maraud", "After a battle"] {
        assert!(rows.contains(&title.to_string()), "{title} is not in {rows:?}");
    }
}

#[test]
fn after_a_battle_next_goes_to_the_patrol_and_d_lets_it_go_on() {
    let mut game = game_with_levy();
    let red: Vec<UnitId> = game.map.units_at(WEST).to_vec();
    game.apply(Order::Patrol { units: red.clone(), waypoints: vec![WEST, MIDDLE], attack: Some(50) }).unwrap();
    let at = pos(&game, red[0]);
    let mut s = session(game);
    type_char(&mut s, 'n');
    assert_eq!(s.screen.game.selected().map(|id| pos(&s.screen.game, id)), Some(at));
    assert_eq!(s.unit_line(), "2 units fought on patrol: D lets it go on; another order ends the patrol.");
    type_char(&mut s, 'd');
    assert!(red.iter().all(|id| !unit(&s.screen.game, *id).fought && unit(&s.screen.game, *id).patrol.is_some()));
    assert!(s.screen.game.waiting_stacks(Side::Red).is_empty());
}

/// The owner: "patrol was not working ... it seems like it was in goto mode".
/// While the window asks how the patrol fights, the stack is not selected: a
/// click or Enter on the map gives it no order; Enter is the window's
/// primary choice.
#[test]
fn while_the_window_asks_the_map_gives_no_order_and_enter_chooses() {
    let mut game = game();
    let red = put(&mut game, Side::Red, "soldiers", WEST, 2);
    let mut s = session(game);
    click_hex(&mut s, WEST);
    type_char(&mut s, 'p');
    click_hex(&mut s, EAST);
    assert!(s.patrol_window().is_some());
    assert_eq!(s.screen.game.selected(), None, "nothing selected while it asks");
    assert_eq!(s.outlined(), [WEST, EAST], "the map outlines its waypoints");
    // A click on the map far off (beside the window): no move, no Go to;
    // the window still asks.
    let map = s.screen.map_rect();
    click(&mut s, turbo_ferret::Point::new(map.x + 20, map.y + map.h - 20));
    assert!(red.iter().all(|id| pos(&s.screen.game, *id) == WEST && unit(&s.screen.game, *id).route.is_empty()));
    assert!(s.patrol_window().is_some());
    // Enter at the cursor: the window's primary choice, not a move there.
    s.screen.game.set_cursor(SOUTH);
    press(&mut s, SpecialKey::Enter);
    assert!(s.patrol_window().is_none());
    assert!(red.iter().all(|id| unit(&s.screen.game, *id).goto().is_none()));
    assert!(red.iter().all(|id| unit(&s.screen.game, *id).patrol.is_some()), "patrolling");
}

/// A stack on patrol, selected to look at it, is not sent elsewhere by a
/// stray click: a click that would end its patrol asks first, saying what
/// it is doing.
#[test]
fn a_stray_click_does_not_end_a_patrol() {
    let mut game = game();
    let red = put(&mut game, Side::Red, "soldiers", WEST, 2);
    let mut s = session(game);
    click_hex(&mut s, WEST);
    type_char(&mut s, 'p');
    click_hex(&mut s, EAST);
    push_widget(&mut s, &choice_id(0));
    push_widget(&mut s, PATROL_OK_ID);
    let at = pos(&s.screen.game, red[0]);
    click_hex(&mut s, at);
    assert_eq!(s.screen.game.selection().len(), 2, "selected, all of it");
    // A hex beyond its reach: no Go to; a window asks first. Keep its
    // orders: it patrols on.
    click_hex(&mut s, SOUTH);
    assert!(red.iter().all(|id| unit(&s.screen.game, *id).patrol.is_some()), "still patrolling");
    assert!(s.asks_mission());
    let says = common::descendants(&s, hexlords::session::MISSION_ASK_ID)
        .into_iter()
        .filter_map(|w| s.screen.ui.text(w).map(str::to_string))
        .collect::<Vec<_>>();
    assert!(says.iter().any(|t| t.starts_with("These 2 units are patrolling 2 waypoints")), "{says:?}");
    push_widget(&mut s, hexlords::session::MISSION_KEEP_ID);
    assert!(!s.asks_mission());
    assert!(red.iter().all(|id| unit(&s.screen.game, *id).patrol.is_some()));
    // Again, and Send them: it goes there, and patrols no more.
    click_hex(&mut s, SOUTH);
    push_widget(&mut s, hexlords::session::MISSION_GO_ID);
    assert!(red.iter().all(|id| unit(&s.screen.game, *id).patrol.is_none()));
    assert_eq!(unit(&s.screen.game, red[0]).goto(), Some(SOUTH));
}
