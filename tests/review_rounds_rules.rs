//! The review of the rounds' rules (2026-11-22; `src/game/rounds.rs` and
//! its hooks): the reviewer's probes, kept as tests. Each states what the
//! rules ought to do; those marked as findings failed before their fix
//! (stacks of a side in each other's way, an attack from several hexes
//! made piecemeal, a spent stack holding up another, the agents' scouts
//! and pilgrims, who steps first with a drawn order); the others are
//! coverage. Built like `tests/rounds.rs`.

use hexlords::agents::{self, Agents};
use hexlords::game::Game;
use hexlords::orders::{Event, Order, RallyPoint};
use hexlords::setup::{Controller, Fog, Mode, Setup, TurnOrder};
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{Hex, Side, UnitId};

const REDHOLD: Hex = Hex::new(5, 5);
const BLUEMOOR: Hex = Hex::new(24, 24);

/// What a probe's little world is made of.
#[derive(Clone, Default)]
struct World {
    /// Hexes that are not grass: (row, col, letter).
    terrain: Vec<(usize, usize, char)>,
    /// More kinds of unit (`units.txt` records).
    kinds: String,
    /// More cities (`cities.txt` records).
    cities: String,
}

/// 30 x 30 grass: red's Redhold (5, 5), blue's Bluemoor (24, 24), nobody's
/// Midway (15, 15), and what `world` adds; red and blue passive computers;
/// no chance in battle (the stronger wins); `mode`; what `set` sets.
/// Soldiers (strength 5) and levies (1) go 6 hexes of grass a round, riders
/// (3) 10.
fn game_in(mode: Mode, world: &World, place: impl FnOnce(&mut Game), set: impl FnOnce(&mut Setup)) -> Game {
    let mut rows: Vec<Vec<char>> = vec![vec!['g'; 30]; 30];
    for (row, col) in [(5, 5), (24, 24), (15, 15)] {
        rows[row][col] = 'c';
    }
    for (row, col, letter) in &world.terrain {
        rows[*row][*col] = *letter;
    }
    let rows: Vec<String> = rows.into_iter().map(|r| r.into_iter().collect()).collect();
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = format!(
        "unit soldiers\nname Soldiers\nstrength 5\nmoves 12\nenter grass 2, city 1\n\
         unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n\
         unit riders\nname Riders\nstrength 3\nmoves 20\nenter grass 2, city 1\n{}",
        world.kinds
    );
    let cities = format!(
        "city 5 5\nname Redhold\nside red\nincome 10\nproduce levy 1\n\
         city 24 24\nname Bluemoor\nside blue\nincome 10\nproduce levy 1\n\
         city 15 15\nname Midway\n{}",
        world.cities
    );
    let (map, warnings) = scenario::parse(&terrain, Some(&units), Some(&cities)).unwrap();
    let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
    place(&mut game);
    let mut setup = Setup::solo(&game.map, None, "passive");
    setup.set_players("red=passive,blue=passive").unwrap();
    (setup.mode, setup.combat.chance) = (mode, 0);
    set(&mut setup);
    game.with_setup(setup)
}

fn rounds(world: &World, place: impl FnOnce(&mut Game)) -> Game {
    game_in(Mode::Rounds, world, place, |_| {})
}

fn put(game: &mut Game, side: Side, kind: &str, hex: Hex) -> UnitId {
    let kind = game.map.units().kind_named(kind).unwrap();
    let id = game.map.new_unit_of(side, kind).unwrap();
    game.map.place_unit(id, hex).unwrap();
    id
}

fn put_n(game: &mut Game, side: Side, kind: &str, hex: Hex, n: usize) -> Vec<UnitId> {
    (0..n).map(|_| put(game, side, kind, hex)).collect()
}

fn pos(game: &Game, id: UnitId) -> Option<Hex> {
    game.map.units().get(id).map(|u| u.pos())
}

fn battles(events: &[Event]) -> Vec<(Hex, Side, bool)> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Battle { at, attacker, won, .. } => Some((*at, *attacker, *won)),
            _ => None,
        })
        .collect()
}

/// Every side still planning says it has done: the round plays out (red
/// first, then blue, as the probes' games are set up).
fn play(game: &mut Game) -> Vec<Event> {
    let round = game.turn();
    let mut events = Vec::new();
    for _ in 0..8 {
        if game.turn() != round || game.winner().is_some() {
            break;
        }
        events.extend(game.end_turn());
    }
    events
}

/// A corridor a hex wide along row 10, columns 2 to 12: water above and
/// below it (soldiers cannot enter water).
fn corridor() -> World {
    let mut world = World::default();
    for col in 1..=13 {
        world.terrain.push((9, col, 'w'));
        world.terrain.push((11, col, 'w'));
    }
    world.terrain.push((10, 1, 'w'));
    world.terrain.push((10, 13, 'w'));
    world
}

/// The corridor with a city of red's in it, at (10, 6).
fn gated() -> World {
    let mut world = corridor();
    world.terrain.push((10, 6, 'c'));
    world.cities = "city 10 6\nname Gate\nside red\n".to_string();
    world
}

// ---------------------------------------------------------------------
// Room on the way: a plan is a move, and a move goes through its side's
// stacks.
// ---------------------------------------------------------------------

/// The same order in turns: seven soldiers go through their own city (two
/// in it) to the far side of it.
#[test]
fn in_turns_a_move_goes_through_the_sides_own_city() {
    let world = gated();
    let mut army = Vec::new();
    let mut g = game_in(
        Mode::Turns,
        &world,
        |g| {
            army = put_n(g, Side::Red, "soldiers", Hex::new(10, 4), 7);
            put_n(g, Side::Red, "levy", Hex::new(10, 6), 2);
        },
        |_| {},
    );
    let to = Hex::new(10, 8);
    g.apply(Order::Move { units: army.clone(), to }).unwrap();
    assert!(army.iter().all(|id| pos(&g, *id) == Some(to)));
}

/// In rounds the plan is accepted (they can get there this turn), and then
/// never made: the army stops at its own city's gate, round after round.
#[test]
fn a_plan_goes_through_the_sides_own_city_as_a_move_does() {
    let world = gated();
    let mut army = Vec::new();
    let mut g = rounds(&world, |g| {
        army = put_n(g, Side::Red, "soldiers", Hex::new(10, 4), 7);
        put_n(g, Side::Red, "levy", Hex::new(10, 6), 2);
    });
    let to = Hex::new(10, 8);
    assert_eq!(g.apply(Order::Move { units: army.clone(), to }), Ok(Vec::new()), "the plan is accepted");
    play(&mut g);
    assert!(army.iter().all(|id| pos(&g, *id) == Some(to)), "sent to {to}, the army was at {:?}", pos(&g, army[0]));
}

/// The same with Go to: the march does get through (at the round's end).
#[test]
fn a_march_goes_through_the_sides_own_city() {
    let world = gated();
    let mut army = Vec::new();
    let mut g = rounds(&world, |g| {
        army = put_n(g, Side::Red, "soldiers", Hex::new(10, 4), 7);
        put_n(g, Side::Red, "levy", Hex::new(10, 6), 2);
    });
    let to = Hex::new(10, 8);
    g.apply(Order::GoTo { units: army.clone(), to, via: Vec::new() }).unwrap();
    play(&mut g);
    assert!(army.iter().all(|id| pos(&g, *id) == Some(to)), "at {:?}", pos(&g, army[0]));
}

/// Two stacks of a side that pass each other on a road a hex wide: in turns
/// each move goes through the other stack; in rounds both stop face to face
/// and neither plan is made.
#[test]
fn two_stacks_of_a_side_pass_each_other() {
    for mode in [Mode::Turns, Mode::Rounds] {
        let (mut east, mut west) = (Vec::new(), Vec::new());
        let mut g = game_in(
            mode,
            &corridor(),
            |g| {
                east = put_n(g, Side::Red, "soldiers", Hex::new(10, 3), 5);
                west = put_n(g, Side::Red, "soldiers", Hex::new(10, 6), 5);
            },
            |_| {},
        );
        g.apply(Order::Move { units: east.clone(), to: Hex::new(10, 8) }).unwrap();
        g.apply(Order::Move { units: west.clone(), to: Hex::new(10, 3) }).unwrap();
        if mode == Mode::Rounds {
            play(&mut g);
        }
        assert_eq!(
            (pos(&g, east[0]), pos(&g, west[0])),
            (Some(Hex::new(10, 8)), Some(Hex::new(10, 3))),
            "{mode:?}: where the two stacks are"
        );
    }
}

/// Riders sent where they can get this turn come up with soldiers of their
/// side who are on the same way to the same place and have no movement
/// left: from then on the two are one stack, which goes at the pace of the
/// one with no movement, and the riders' plan is not made.
#[test]
fn a_plan_is_not_held_up_by_a_stack_of_the_side_that_has_spent_its_movement() {
    for mode in [Mode::Turns, Mode::Rounds] {
        let (mut riders, mut soldiers) = (None, None);
        let mut g = game_in(
            mode,
            &corridor(),
            |g| {
                riders = Some(put(g, Side::Red, "riders", Hex::new(10, 2)));
                soldiers = Some(put(g, Side::Red, "soldiers", Hex::new(10, 4)));
            },
            |_| {},
        );
        let (riders, soldiers) = (riders.unwrap(), soldiers.unwrap());
        let to = Hex::new(10, 12);
        // The soldiers as far as they get (six hexes); the riders all the
        // way (ten hexes: their whole movement).
        g.apply(Order::Toward { units: vec![soldiers], to }).unwrap();
        g.apply(Order::Move { units: vec![riders], to }).unwrap();
        if mode == Mode::Rounds {
            play(&mut g);
        }
        assert_eq!(
            (pos(&g, soldiers), pos(&g, riders)),
            (Some(Hex::new(10, 10)), Some(to)),
            "{mode:?}: the soldiers, the riders"
        );
    }
}

/// With the order drawn anew each round "nobody knows" who steps first in
/// a tick (`docs/gameplay.md`). But the order of the ticks is the order the
/// sides plan in, drawn before they plan and told to every agent
/// (`describe`: "order ..."): everybody knows.
#[test]
fn with_a_drawn_order_nobody_knows_who_steps_first() {
    let mut told = 0;
    for seed in 1..=12 {
        let (mut a, mut b) = (None, None);
        let mut g = game_in(
            Mode::Rounds,
            &World::default(),
            |g| {
                a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5)));
                b = Some(put(g, Side::Blue, "soldiers", Hex::new(10, 9)));
            },
            |s| (s.order, s.seed) = (TurnOrder::Drawn, seed),
        );
        let (a, b) = (a.unwrap(), b.unwrap());
        // What a side is told as it plans, and who then holds the hex both
        // make for (the one that stepped in first).
        let first = g.order()[0];
        let line = format!("order {}", if first == Side::Red { "red blue" } else { "blue red" });
        assert!(agents::describe(&g.view_for(g.player()), g.player()).contains(&line));
        assert_eq!(g.player(), first, "and the first to plan is the first to step");
        for _ in 0..2 {
            let unit = if g.player() == Side::Red { a } else { b };
            g.apply(Order::Move { units: vec![unit], to: Hex::new(10, 7) }).unwrap();
            g.end_turn();
        }
        // (Equal stacks, no chance: the attacker, who came second, loses.)
        let holder = g.map.units_at(Hex::new(10, 7)).first().map(|id| id.side);
        told += usize::from(holder == Some(first));
    }
    assert!(told < 12, "in {told} games of 12 the side named first in the order it was told held the hex");
}

// ---------------------------------------------------------------------
// One order, one attack.
// ---------------------------------------------------------------------

/// One `Order::Move` of units on two hexes against a stack: in turns one
/// attack, all of them together (what the agents count on when they weigh
/// the odds). In rounds the nearer stack attacks alone when it gets there,
/// and the farther one alone after it.
#[test]
fn an_attack_ordered_from_two_hexes_is_one_attack() {
    for mode in [Mode::Turns, Mode::Rounds] {
        let (mut near, mut far) = (Vec::new(), Vec::new());
        let target = Hex::new(10, 10);
        let mut g = game_in(
            mode,
            &World::default(),
            |g| {
                put_n(g, Side::Blue, "soldiers", target, 3);
                near = put_n(g, Side::Red, "soldiers", Hex::new(10, 9), 2);
                far = put_n(g, Side::Red, "soldiers", Hex::new(10, 6), 2);
            },
            |s| s.combat.system = hexlords::state::combat::System::Strength,
        );
        let all: Vec<UnitId> = near.iter().chain(&far).copied().collect();
        let odds = g.battle_odds(&all, target);
        let mut events = g.apply(Order::Move { units: all, to: target }).unwrap();
        if mode == Mode::Rounds {
            events.extend(play(&mut g));
        }
        assert_eq!(
            battles(&events),
            [(target, Side::Red, true)],
            "{mode:?}: the odds were {odds:?} for the four together; the battles (where, whose, won)"
        );
    }
}

// ---------------------------------------------------------------------
// Rules that should hold in rounds as in turns (these pass: coverage).
// ---------------------------------------------------------------------

const SEA_KINDS: &str = "unit ship\nname Ship\nstrength 2\nmoves 12\ntravels sea\ncarries 4\nenter water 2, city 1\n\
                         unit captain\nname Captain\nstrength 2\nmoves 12\ncommander 1\nenter grass 2, city 1\n\
                         unit eagles\nname Eagles\nstrength 2\nmoves 16\ntravels air\nenter grass 2, water 2, city 1\n";

/// A lake: rows 18 to 22, columns 3 to 12.
fn lake() -> World {
    let mut world = World { kinds: SEA_KINDS.to_string(), ..World::default() };
    for row in 18..=22 {
        for col in 3..=12 {
            world.terrain.push((row, col, 'w'));
        }
    }
    world
}

#[test]
fn a_ship_carries_its_riders_with_the_round_and_flyers_fly_their_captain() {
    let (mut ship, mut riders, mut wing) = (None, Vec::new(), Vec::new());
    let mut g = rounds(&lake(), |g| {
        ship = Some(put(g, Side::Red, "ship", Hex::new(20, 4)));
        riders = put_n(g, Side::Red, "soldiers", Hex::new(20, 4), 3);
        wing = put_n(g, Side::Red, "eagles", Hex::new(17, 5), 2);
        wing.push(put(g, Side::Red, "captain", Hex::new(17, 5)));
    });
    let ship = ship.unwrap();
    // The ship alone would leave its riders in the water: refused, as a move.
    let alone = g.apply(Order::Move { units: vec![ship], to: Hex::new(20, 8) });
    assert!(alone.is_err(), "{alone:?}");
    let aboard: Vec<UnitId> = std::iter::once(ship).chain(riders.iter().copied()).collect();
    g.apply(Order::Move { units: aboard.clone(), to: Hex::new(20, 8) }).unwrap();
    // The eagles and their captain, over the lake (four hexes of water).
    g.apply(Order::Move { units: wing.clone(), to: Hex::new(23, 5) }).unwrap();
    let events = play(&mut g);
    assert!(aboard.iter().all(|id| pos(&g, *id) == Some(Hex::new(20, 8))), "{:?}", pos(&g, ship));
    assert!(wing.iter().all(|id| pos(&g, *id) == Some(Hex::new(23, 5))), "{:?}", pos(&g, wing[2]));
    assert!(!events.iter().any(|e| matches!(e, Event::Drowned { .. })));
    assert_eq!(g.map.units().of_side(Side::Red).count(), 7);
}

/// A third side, green, with a city at (20, 20).
fn three_sides() -> World {
    let mut world = World::default();
    world.terrain.push((20, 20, 'c'));
    world.cities = "city 20 20\nname Greenway\nside green\nincome 10\nproduce levy 1\n".to_string();
    world
}

#[test]
fn a_side_put_out_as_the_round_plays_takes_no_more_part_and_the_game_goes_on() {
    let (mut red, mut green) = (None, Vec::new());
    let mut g = game_in(
        Mode::Rounds,
        &three_sides(),
        |g| {
            red = Some(put(g, Side::Red, "riders", Hex::new(20, 17)));
            green = put_n(g, Side::Green, "soldiers", Hex::new(12, 20), 2);
            put(g, Side::Blue, "levy", Hex::new(24, 24));
        },
        |s| s.set_players("green=passive").unwrap(),
    );
    assert_eq!(g.order(), [Side::Red, Side::Green, Side::Blue]);
    let red = red.unwrap();
    // Red takes Greenway (empty) in the third tick; green's soldiers, on
    // their way to Midway, are gone with their side.
    g.apply(Order::Move { units: vec![red], to: Hex::new(20, 20) }).unwrap();
    g.end_turn();
    assert_eq!(g.player(), Side::Green);
    g.apply(Order::Move { units: green.clone(), to: Hex::new(15, 20) }).unwrap();
    g.end_turn();
    assert_eq!(g.player(), Side::Blue);
    let events = g.end_turn();
    assert!(events.contains(&Event::Out { side: Side::Green }), "{events:?}");
    assert!(g.winner().is_none() && g.out() == [Side::Green]);
    assert_eq!(g.map.units().of_side(Side::Green).count(), 0);
    assert_eq!((g.turn(), g.player()), (2, Side::Red));
    assert_eq!(events.last(), Some(&Event::Began { side: Side::Red, round: 2 }));
    // The next round: red, then blue; green has no say.
    assert_eq!(g.end_turn(), [Event::Began { side: Side::Blue, round: 2 }]);
    let events = g.end_turn();
    assert_eq!((g.turn(), g.player()), (3, Side::Red));
    assert!(!events.iter().any(|e| matches!(e, Event::Built { unit, .. } if unit.side == Side::Green)));
    round_begins_clean(&g).unwrap();
}

#[test]
fn the_last_city_taken_as_the_round_plays_ends_the_game_there() {
    let (mut red, mut blue) = (None, None);
    let mut g = rounds(&World::default(), |g| {
        red = Some(put(g, Side::Red, "riders", Hex::new(24, 21)));
        blue = Some(put(g, Side::Blue, "soldiers", Hex::new(10, 10)));
    });
    let (red, blue) = (red.unwrap(), blue.unwrap());
    g.apply(Order::Move { units: vec![red], to: BLUEMOOR }).unwrap();
    g.end_turn();
    g.apply(Order::Move { units: vec![blue], to: Hex::new(10, 5) }).unwrap();
    let events = g.end_turn();
    assert_eq!(g.winner(), Some(Side::Red));
    assert!(events.ends_with(&[Event::Out { side: Side::Blue }, Event::Won { side: Side::Red }]), "{events:?}");
    assert!(!g.playing_out());
    assert!(g.apply(Order::EndTurn).is_err());
    assert!(g.play_out().is_empty());
    assert!(g.map.units().iter().all(|u| !u.planned));
}

#[test]
fn every_side_is_paid_once_a_round_as_in_turns() {
    let run = |mode: Mode| {
        let mut g = game_in(
            mode,
            &World::default(),
            |g| {
                put_n(g, Side::Red, "soldiers", REDHOLD, 2);
                put_n(g, Side::Blue, "soldiers", BLUEMOOR, 3);
            },
            |_| {},
        );
        // Each side's gold and its units' movement as each of its turns
        // (its plannings) begins.
        let mut seen = Vec::new();
        for _ in 0..10 {
            g.end_turn();
            let side = g.player();
            let moves: Vec<u32> = g.map.units().of_side(side).map(|u| u.movement).collect();
            seen.push((g.turn(), side, g.gold(side), moves));
        }
        seen
    };
    let rounds = run(Mode::Rounds);
    assert_eq!(rounds, run(Mode::Turns));
    assert_eq!(rounds[9], (6, Side::Red, 100, vec![14, 14]), "ten gold a round");
}

#[test]
fn walls_are_raised_once_a_round_and_an_undo_gives_the_gold_back() {
    let mut g = game_in(Mode::Rounds, &World::default(), |_| {}, |s| s.gold = 1000);
    let city = g.map.city_at(REDHOLD).unwrap().id;
    g.apply(Order::Walls { city }).unwrap();
    assert!(g.apply(Order::Walls { city }).is_err());
    g.apply(Order::Undo).unwrap();
    assert_eq!(g.gold(Side::Red), 1000);
    g.apply(Order::Walls { city }).unwrap();
    g.end_turn();
    // Blue's planning: red's walls stay raised for the round.
    let theirs = g.map.city_at(BLUEMOOR).unwrap().id;
    g.apply(Order::Walls { city: theirs }).unwrap();
    g.end_turn();
    assert_eq!(g.turn(), 2);
    g.apply(Order::Walls { city }).unwrap();
    assert_eq!(g.map.city_at(REDHOLD).unwrap().walls, 2);
}

#[test]
fn undo_takes_back_each_kind_of_order_in_rounds_and_not_across_the_handover() {
    let mut a = None;
    let mut g = rounds(&World::default(), |g| a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5))));
    let a = a.unwrap();
    let as_it_was = state(&g);
    // Go to, a patrol, a plan on top of a Go to.
    g.apply(Order::GoTo { units: vec![a], to: Hex::new(10, 20), via: Vec::new() }).unwrap();
    g.apply(Order::Undo).unwrap();
    assert_eq!(state(&g), as_it_was, "Go to, taken back");
    g.apply(Order::Patrol { units: vec![a], waypoints: vec![Hex::new(10, 9), Hex::new(10, 5)], attack: None }).unwrap();
    g.apply(Order::Undo).unwrap();
    assert_eq!(state(&g), as_it_was, "a patrol, taken back");
    g.apply(Order::GoTo { units: vec![a], to: Hex::new(10, 20), via: Vec::new() }).unwrap();
    let marching = state(&g);
    g.apply(Order::Move { units: vec![a], to: Hex::new(10, 7) }).unwrap();
    g.apply(Order::Undo).unwrap();
    assert_eq!(state(&g), marching, "a plan over a Go to, taken back");
    // A patrol over a plan ends the plan; so does a Go to.
    g.apply(Order::Move { units: vec![a], to: Hex::new(10, 7) }).unwrap();
    g.apply(Order::Patrol { units: vec![a], waypoints: vec![Hex::new(10, 9), Hex::new(10, 5)], attack: None }).unwrap();
    let unit = g.map.units().get(a).unwrap();
    assert!(!unit.planned && unit.route.is_empty() && unit.patrol.is_some());
    g.apply(Order::Move { units: vec![a], to: Hex::new(10, 7) }).unwrap();
    g.apply(Order::GoTo { units: vec![a], to: Hex::new(10, 20), via: Vec::new() }).unwrap();
    let unit = g.map.units().get(a).unwrap();
    assert!(!unit.planned && unit.route == [Hex::new(10, 20)]);
    // Maraud on a plan, then off: no plan left with an empty route.
    g.apply(Order::Move { units: vec![a], to: Hex::new(10, 7) }).unwrap();
    g.apply(Order::Maraud { units: vec![a], on: true }).unwrap();
    let unit = g.map.units().get(a).unwrap();
    assert!(!unit.planned && unit.route.is_empty() && unit.marauding);
    g.apply(Order::Maraud { units: vec![a], on: false }).unwrap();
    g.apply(Order::Move { units: vec![a], to: Hex::new(10, 7) }).unwrap();
    // Not across the handover.
    g.end_turn();
    assert_eq!(g.player(), Side::Blue);
    assert!(!g.can_undo());
    assert_eq!(g.apply(Order::Undo), Err("there is nothing to undo".to_string()));
    assert!(g.map.units().get(a).unwrap().planned, "red's plan stands");
}

#[test]
fn orders_off_the_map_are_refused_not_a_panic() {
    let mut a = None;
    let mut g = rounds(&World::default(), |g| a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5))));
    let a = a.unwrap();
    for to in [Hex::new(999, 999), Hex::new(30, 0), Hex::new(0, 30), Hex::new(u16::MAX, u16::MAX)] {
        assert!(g.apply(Order::Move { units: vec![a], to }).is_err(), "move to {to}");
        assert!(g.apply(Order::Toward { units: vec![a], to }).is_err(), "toward {to}");
        g.apply(Order::Maraud { units: vec![a], on: true }).unwrap();
        assert!(g.apply(Order::Roam { units: vec![a], to }).is_err(), "roam to {to}");
        g.apply(Order::Maraud { units: vec![a], on: false }).unwrap();
    }
    assert!(g.apply(Order::Move { units: Vec::new(), to: Hex::new(10, 6) }).is_err());
    // A plan to where it stands is a plan that is over at once.
    let _ = g.apply(Order::Move { units: vec![a], to: Hex::new(10, 5) });
    play(&mut g);
    round_begins_clean(&g).unwrap();
}

#[test]
fn a_patrol_that_fought_waits_a_round_for_its_players_word_then_goes_on() {
    let (mut a, mut b) = (None, None);
    let mut g = rounds(&World::default(), |g| {
        a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5)));
        b = Some(put(g, Side::Blue, "levy", Hex::new(10, 7)));
    });
    let (a, b) = (a.unwrap(), b.unwrap());
    let waypoints = vec![Hex::new(10, 9), Hex::new(10, 5)];
    g.apply(Order::Patrol { units: vec![a], waypoints, attack: Some(60) }).unwrap();
    let events = play(&mut g);
    assert_eq!(battles(&events).len(), 1);
    assert!(g.map.units().get(b).is_none());
    let unit = g.map.units().get(a).unwrap();
    assert!(unit.fought && unit.patrol.is_some());
    assert!(g.waiting_stacks(Side::Red).contains(&unit.pos()), "it waits for its player's word");
    let at = unit.pos();
    // Left alone: it goes on with the next round.
    let events = play(&mut g);
    let unit = g.map.units().get(a).unwrap();
    assert!(!unit.fought && unit.pos() != at, "{events:?}");
}

#[test]
fn a_march_to_a_rally_point_defends_there_and_a_march_by_a_temple_is_blessed() {
    let mut world = World::default();
    world.terrain.push((10, 8, 't'));
    let (mut a, mut b) = (None, None);
    let mut g = rounds(&world, |g| {
        a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5)));
        b = Some(put(g, Side::Red, "soldiers", Hex::new(12, 5)));
    });
    let (a, b) = (a.unwrap(), b.unwrap());
    g.apply(Order::RallyPoint { n: 1, point: Some(RallyPoint { at: Hex::new(12, 9), defend: true }) }).unwrap();
    g.apply(Order::GoTo { units: vec![b], to: Hex::new(12, 9), via: Vec::new() }).unwrap();
    assert_eq!(g.map.units().get(b).unwrap().bound, Some(1));
    g.apply(Order::GoTo { units: vec![a], to: Hex::new(10, 12), via: vec![Hex::new(10, 8)] }).unwrap();
    let events = play(&mut g);
    let unit = g.map.units().get(b).unwrap();
    assert_eq!((unit.pos(), unit.defending, unit.bound, unit.route.len()), (Hex::new(12, 9), true, None, 0));
    let unit = g.map.units().get(a).unwrap();
    assert_eq!(unit.pos(), Hex::new(10, 8), "it stops at the temple for the round");
    assert!(unit.blessed_at(Hex::new(10, 8)) && unit.route == [Hex::new(10, 12)]);
    assert_eq!(events.iter().filter(|e| matches!(e, Event::Blessed { .. })).count(), 1);
    play(&mut g);
    play(&mut g);
    assert_eq!(pos(&g, a), Some(Hex::new(10, 12)));
}

#[test]
fn marauders_are_led_by_plans_and_maraud_on() {
    let (mut a, mut b) = (None, None);
    let mut g = rounds(&World::default(), |g| {
        a = Some(put(g, Side::Red, "soldiers", Hex::new(8, 22)));
        b = Some(put(g, Side::Blue, "levy", Hex::new(8, 24)));
    });
    let (a, b) = (a.unwrap(), b.unwrap());
    g.apply(Order::Maraud { units: vec![a], on: true }).unwrap();
    let led = hexlords::marauder::maraud(&mut g, Side::Red);
    assert!(led.refused.is_empty(), "{:?}", led.refused);
    let unit = g.map.units().get(a).unwrap();
    assert!(unit.planned && unit.marauding && unit.route.len() == 1, "one plan: {:?}", unit.route);
    assert!(!g.can_undo(), "the computer's plan is not the player's to take back");
    let events = play(&mut g);
    assert_eq!(battles(&events).len(), 1, "{events:?}");
    assert!(g.map.units().get(b).is_none());
    let unit = g.map.units().get(a).unwrap();
    assert!(unit.marauding && unit.fought && !unit.planned && unit.route.is_empty());
    // It waits for its player's word through the next planning; the
    // computer, leading the side's marauders at its end, lets it go on.
    assert!(g.waiting_stacks(Side::Red).contains(&unit.pos()));
    let led = hexlords::marauder::maraud(&mut g, Side::Red);
    assert!(led.refused.is_empty());
    let unit = g.map.units().get(a).unwrap();
    assert!(unit.planned && unit.marauding && !unit.fought);
    play(&mut g);
    assert!(g.map.units().get(a).is_some_and(|u| u.marauding && !u.planned));
}

/// With fog of war the agents are given their side's view: nothing in it
/// of the plans the sides before them made.
#[test]
fn a_sides_view_has_none_of_the_others_plans() {
    let (mut a, mut b) = (None, None);
    let mut g = game_in(
        Mode::Rounds,
        &World::default(),
        |g| {
            a = Some(put(g, Side::Red, "soldiers", Hex::new(22, 22)));
            b = Some(put(g, Side::Blue, "soldiers", Hex::new(24, 24)));
        },
        |_| {},
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    let clean = g.view_for(Side::Blue);
    g.apply(Order::Move { units: vec![a], to: BLUEMOOR }).unwrap();
    g.end_turn();
    let view = g.view_for(Side::Blue);
    let theirs = view.map.units().get(a).unwrap();
    assert!(!theirs.planned && theirs.route.is_empty() && !theirs.moved);
    assert_eq!(agents::describe(&view, Side::Blue), agents::describe(&clean, Side::Blue));
    assert!(view.ways_this_round(Side::Red).is_empty());
    let _ = b;
}

// ---------------------------------------------------------------------
// Whole games: the agents on the continent, in rounds.
// ---------------------------------------------------------------------

/// The continent, every side an agent (`players`, in the seats' order, the
/// last for the rest), in `mode`, as `set` sets it.
fn continent(mode: Mode, seed: u64, players: &[&str], set: impl FnOnce(&mut Setup)) -> Game {
    let (game, _) = Game::load(std::path::Path::new("assets/maps/continent")).unwrap();
    let mut setup = Setup::solo(&game.map, None, "steady");
    for (i, seat) in setup.seats.iter_mut().enumerate() {
        let name = players.get(i).or(players.last()).copied().unwrap_or("steady");
        seat.controller = Controller::Computer(name.to_string());
    }
    (setup.mode, setup.seed) = (mode, seed);
    set(&mut setup);
    game.with_setup(setup)
}

/// What a game has come to, to compare two of them.
fn state(game: &Game) -> String {
    let mut out =
        format!("round {} player {:?} winner {:?} out {:?}\n", game.turn(), game.player(), game.winner(), game.out());
    for unit in game.map.units().iter() {
        out.push_str(&format!(
            "{:?} at {} moves {} route {:?} planned {} moved {} done {} stalled {} fought {} defending {} \
             marauding {} patrol {} wounds {} level {}\n",
            unit.id(),
            unit.pos(),
            unit.movement,
            unit.route,
            unit.planned,
            unit.moved,
            unit.done,
            unit.stalled,
            unit.fought,
            unit.defending,
            unit.marauding,
            unit.patrol.is_some(),
            unit.wounds,
            unit.level
        ));
    }
    for city in game.map.cities().iter() {
        out.push_str(&format!(
            "{} {:?} walls {} builds {:?} {}\n",
            city.name, city.side, city.walls, city.building, city.progress
        ));
    }
    for seat in &game.setup().seats {
        out.push_str(&format!("{:?} gold {}\n", seat.side, game.gold(seat.side)));
    }
    out
}

/// The rounds' own promises about what a unit carries from one round into
/// the next, checked as a round begins: `Err` says which is broken.
fn round_begins_clean(game: &Game) -> Result<(), String> {
    for unit in game.map.units().iter() {
        let side = unit.side();
        let plays = game.order().contains(&side) && !game.out().contains(&side);
        if unit.planned {
            return Err(format!("{:?} still has a plan (route {:?}) in round {}", unit.id(), unit.route, game.turn()));
        }
        if plays && (unit.moved || unit.done) {
            return Err(format!(
                "{:?} moved {} done {} as round {} begins",
                unit.id(),
                unit.moved,
                unit.done,
                game.turn()
            ));
        }
        if unit.wounds > 0 {
            return Err(format!("{:?} is still wounded as round {} begins", unit.id(), game.turn()));
        }
    }
    Ok(())
}

/// The game played by its agents for `rounds` rounds, the screen's way: a
/// tick at a time (`watch`), or the headless way. Every event, and what
/// `check` says at each round's beginning.
fn play_agents(game: &mut Game, rounds: u32, watched: bool, check: &mut dyn FnMut(&Game, &[Event])) -> Vec<Event> {
    let mut agents = Agents::for_game(game).unwrap();
    game.watch(watched);
    let mut all = Vec::new();
    while game.winner().is_none() && game.turn() <= rounds {
        let round = game.turn();
        let mut events = agents::play_computers(game, &mut agents).events;
        let mut ticks = 0;
        while game.playing_out() {
            events.extend(game.play_out());
            ticks += 1;
            assert!(ticks < 1000, "a round that never ends");
        }
        if game.winner().is_none() {
            assert_eq!(game.turn(), round + 1, "a call of play_computers is a round");
        }
        check(game, &events);
        all.extend(events);
    }
    all
}

#[test]
fn a_round_leaves_no_plan_no_wound_and_no_flag_behind() {
    for (seed, players, fog, order) in [
        (1, vec!["steady"], Fog::Off, TurnOrder::Fixed),
        (2, vec!["simple", "bold", "careful", "steady"], Fog::On, TurnOrder::Drawn),
        (3, vec!["bold"], Fog::Total, TurnOrder::Drawn),
    ] {
        let mut g = continent(Mode::Rounds, seed, &players, |s| {
            (s.fog, s.order, s.combat.wounds) = (fog, order, true);
        });
        let mut broken = Vec::new();
        play_agents(&mut g, 40, false, &mut |g, _| {
            if g.winner().is_none() {
                broken.extend(round_begins_clean(g).err());
            }
        });
        assert!(broken.is_empty(), "seed {seed}: {:#?}", &broken[..broken.len().min(5)]);
    }
}

#[test]
fn a_round_watched_is_the_round_unwatched_with_agents_fog_and_a_drawn_order() {
    for (seed, players, fog, order) in [
        (1, vec!["steady"], Fog::Off, TurnOrder::Fixed),
        (5, vec!["simple", "bold", "careful", "steady"], Fog::On, TurnOrder::Drawn),
        (9, vec!["bold", "simple"], Fog::Total, TurnOrder::Drawn),
    ] {
        let set = |s: &mut Setup| {
            (s.fog, s.order, s.combat.wounds, s.combat.chance) = (fog, order, true, 5);
        };
        let mut whole = continent(Mode::Rounds, seed, &players, set);
        let mut ticked = continent(Mode::Rounds, seed, &players, set);
        let a = play_agents(&mut whole, 45, false, &mut |_, _| {});
        let b = play_agents(&mut ticked, 45, true, &mut |_, _| {});
        assert!(a.iter().any(|e| matches!(e, Event::Battle { .. })), "seed {seed}: there were battles");
        let first = a.iter().zip(&b).position(|(x, y)| x != y);
        assert_eq!(
            first,
            None,
            "seed {seed}: the first event that differs: {:?} / {:?}",
            first.map(|i| &a[i]),
            first.map(|i| &b[i])
        );
        assert_eq!(a.len(), b.len(), "seed {seed}");
        assert_eq!(state(&whole), state(&ticked), "seed {seed}");
        let history = |g: &Game| {
            g.history().iter().map(|r| format!("{} {:?} {:?}", r.round, r.turn, r.event)).collect::<Vec<_>>()
        };
        assert_eq!(history(&whole), history(&ticked), "seed {seed}");
        // And again, the same seed: the same game.
        let mut again = continent(Mode::Rounds, seed, &players, set);
        assert_eq!(play_agents(&mut again, 45, false, &mut |_, _| {}), a, "seed {seed}");
    }
}

/// Every battle the history keeps has its own fight beside it (to be
/// watched again), in a round where many are fought.
#[test]
fn every_battle_of_a_round_keeps_its_own_fight() {
    let mut g = continent(Mode::Rounds, 4, &["bold", "simple", "steady", "careful"], |s| s.combat.chance = 5);
    let mut wrong = Vec::new();
    let mut seen = 0;
    play_agents(&mut g, 60, false, &mut |g, _| {
        for (i, record) in g.history().iter().enumerate() {
            let Event::Battle { at, attacker, defender, won, attackers, defenders, .. } = &record.event else {
                continue;
            };
            if record.round + 2 < g.turn() {
                continue;
            }
            seen += 1;
            match g.fight(i) {
                None => wrong.push(format!("record {i} (round {}): no fight kept", record.round)),
                Some(f) => {
                    if (f.at, f.attacker, f.defender, f.won) != (*at, *attacker, *defender, *won)
                        || f.attackers.len() != *attackers
                        || f.defenders.len() != *defenders
                    {
                        wrong.push(format!(
                            "record {i} (round {}): battle at {at} by {attacker:?} won {won}, its fight at {} by \
                             {:?} won {}",
                            record.round, f.at, f.attacker, f.won
                        ));
                    }
                }
            }
        }
    });
    assert!(seen > 20, "{seen}");
    assert!(wrong.is_empty(), "{:#?}", &wrong[..wrong.len().min(5)]);
}

/// The history says in which round, and for which side, each thing happened.
#[test]
fn the_history_says_whose_each_battle_and_city_was() {
    let mut g = continent(Mode::Rounds, 4, &["bold", "simple", "steady", "careful"], |s| s.order = TurnOrder::Drawn);
    let mut wrong = Vec::new();
    let mut from = 0;
    play_agents(&mut g, 50, false, &mut |g, _| {
        // What this round added: the round that played out (g.turn() - 1,
        // or the one the game ended in), then the new one's pay-day.
        let now = g.turn();
        for record in &g.history()[from..] {
            let ok = match &record.event {
                Event::Battle { attacker, .. } => {
                    record.turn == *attacker && (record.round + 1 == now || g.winner().is_some())
                }
                Event::Taken { by, .. } => record.turn == *by,
                Event::Built { unit, .. } => record.turn == unit.side && record.round == now,
                _ => true,
            };
            if !ok {
                wrong.push(format!("in round {now}: {} {:?} {:?}", record.round, record.turn, record.event));
            }
        }
        from = g.history().len();
    });
    assert!(wrong.is_empty(), "{:#?}", &wrong[..wrong.len().min(5)]);
}

// ---------------------------------------------------------------------
// The agents, in rounds.
// ---------------------------------------------------------------------

/// With total fog the agents go to see more of the map, a few orders a turn
/// to each scout (each from where the last took it). In rounds those orders
/// are plans from one and the same hex, each taking the place of the last:
/// the side learns nothing of the map, and the game goes nowhere.
#[test]
fn agents_find_the_map_under_total_fog_in_rounds_as_in_turns() {
    let known = |mode: Mode| {
        let mut g = continent(mode, 2, &["steady"], |s| s.fog = Fog::Total);
        let events = play_agents(&mut g, 40, false, &mut |_, _| {});
        let side = g.setup().seats[0].side;
        let taken = events.iter().filter(|e| matches!(e, Event::Taken { .. })).count();
        (g.sight(side).known_count().unwrap_or(0), taken)
    };
    let (turns, rounds) = (known(Mode::Turns), known(Mode::Rounds));
    assert!(
        rounds.0 * 2 >= turns.0 && rounds.1 * 4 >= turns.1,
        "after 40 rounds, the hexes the first side knows and the cities taken by anyone: in turns {turns:?}, in \
         rounds {rounds:?}"
    );
}

/// The same, at its root: in one planning turn an agent gives a stack
/// several plans, to different places, and only the last counts.
#[test]
fn an_agent_gives_a_stack_one_plan_a_round() {
    let mut g = continent(Mode::Rounds, 2, &["steady"], |s| s.fog = Fog::Total);
    let mut most: (usize, Vec<Hex>) = (0, Vec::new());
    let mut agent = agents::by_name("steady").unwrap();
    for _ in 0..3 {
        // The first side's planning, by hand; the others pass.
        let side = g.player();
        let mut plans: Vec<(UnitId, Vec<Hex>)> = Vec::new();
        for _ in 0..agents::MAX_ORDERS {
            let order = agent.next_order(&g.view_for(side), side);
            if order == Order::EndTurn {
                break;
            }
            if let Order::Move { units, to } | Order::Toward { units, to } = &order {
                for id in units {
                    match plans.iter_mut().find(|(u, _)| u == id) {
                        Some((_, tos)) => tos.push(*to),
                        None => plans.push((*id, vec![*to])),
                    }
                }
            }
            let _ = g.apply(order);
        }
        for (_, tos) in plans {
            if tos.len() > most.0 {
                most = (tos.len(), tos);
            }
        }
        play(&mut g);
    }
    assert!(most.0 <= 1, "one unit was given {} plans in one planning turn: {:?}", most.0, most.1);
}

/// In the agents' own games: plans that stop on the way with movement to
/// spare and no battle near, the next hex of their way held by their own
/// side (the stacks that will not pass each other, above). And how long a
/// round is, in ticks.
#[test]
fn the_agents_stacks_do_not_stand_in_each_others_way() {
    let (mut plans, mut there, mut jammed, mut most_ticks) = (0, 0, 0, 0);
    let mut samples = Vec::new();
    for seed in 1..=3 {
        let mut g = continent(Mode::Rounds, seed, &["steady"], |_| {});
        let mut agents = Agents::for_game(&g).unwrap();
        g.watch(true);
        while g.winner().is_none() && g.turn() <= 60 {
            agents::play_computers(&mut g, &mut agents);
            let planned: Vec<(UnitId, Hex, Hex)> = g
                .map
                .units()
                .iter()
                .filter(|u| u.planned && u.route.first().is_some_and(|to| *to != u.pos()))
                .map(|u| (u.id(), u.pos(), u.route[0]))
                .collect();
            let round = g.turn();
            let (mut fought, mut ticks) = (Vec::new(), 0);
            while g.playing_out() {
                ticks += 1;
                for event in g.play_out() {
                    if let Event::Battle { at, .. } = event {
                        fought.push(at);
                    }
                }
            }
            most_ticks = most_ticks.max(ticks);
            for (id, from, to) in planned {
                plans += 1;
                let Some(unit) = g.map.units().get(id) else { continue };
                let at = unit.pos();
                if at == to {
                    there += 1;
                    continue;
                }
                // (A new round's movement is the kind's and up to two
                // points kept: two kept is a step it could have paid for.)
                let spare = unit.movement >= unit.base_move_points + 2;
                let own_ahead = g.map.neighbours(at).any(|h| {
                    h != to
                        && h.distance(to) < at.distance(to)
                        && g.map.units_at(h).iter().any(|other| other.side == id.side)
                        && !fought.contains(&h)
                });
                if spare && own_ahead && !fought.iter().any(|f| f.distance(at) <= 1) {
                    jammed += 1;
                    if samples.len() < 6 {
                        samples.push(format!("seed {seed} round {round}: {id:?} from {from} to {to} stopped at {at}"));
                    }
                }
            }
        }
    }
    assert!(most_ticks < 40, "{most_ticks} ticks in a round");
    // (Before the fix, 67 of 2879; the few left wait for others of their
    // side before an attack, or have no room past a full stack.)
    assert!(
        jammed * 100 < plans,
        "of {plans} plans in three games of 60 rounds ({there} got there), {jammed} stopped short behind their own \
         side with movement to spare: {samples:#?}"
    );
}

/// An agent sends a stack to a temple to be blessed there. In turns it
/// asks for the blessing on arriving; in rounds it has forgotten by the
/// time the stack is there.
#[test]
fn agents_are_blessed_at_the_temples_they_go_to_in_rounds_as_in_turns() {
    let count = |mode: Mode| {
        let mut g = continent(mode, 1, &["steady", "simple"], |_| {});
        // Units standing, as a round begins, on a temple that has not
        // blessed them: a visit that came to nothing.
        let mut idle = 0;
        let events = play_agents(&mut g, 40, false, &mut |g, _| {
            idle += g
                .map
                .units()
                .iter()
                .filter(|u| g.map.tile(u.pos()).is_some_and(|t| t.kind == hexlords::state::TileType::Temple))
                .filter(|u| !u.blessed_at(u.pos()))
                .count();
        });
        (events.iter().filter(|e| matches!(e, Event::Blessed { .. })).count(), idle)
    };
    let (turns, rounds) = (count(Mode::Turns), count(Mode::Rounds));
    assert!(
        rounds.0 * 4 >= turns.0,
        "after 40 rounds (blessings, units left unblessed on a temple at a round's start): in turns {turns:?}, in \
         rounds {rounds:?}"
    );
}
