//! Rounds, the second way to play (`docs/gameplay.md`, "Rounds"; the
//! owner's word, 2026-11-22): every side gives its orders, nothing moves;
//! then the round plays out on a clock, every stack a step at a time at
//! its own speed, fighting who it finds on its way. The rules, without a
//! window.

use hexlords::agents::{self, Agents, describe};
use hexlords::game::Game;
use hexlords::headless;
use hexlords::orders::{Event, Order};
use hexlords::setup::{Fog, Mode, Setup};
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{Hex, Side, UnitId};

const REDHOLD: Hex = Hex::new(5, 5);
const BLUEMOOR: Hex = Hex::new(24, 24);

/// 30 x 30 grass: red's Redhold, blue's Bluemoor, nobody's Midway between
/// them; what `place` puts; red and blue passive computers, in rounds, with
/// no chance in battle (the stronger wins), and what `set` sets. Soldiers
/// (strength 5) and levies (strength 1) go 6 hexes a round, riders 10.
fn game_with(place: impl FnOnce(&mut Game), set: impl FnOnce(&mut Setup)) -> Game {
    let mut rows = vec!["g".repeat(30); 30];
    for (row, col) in [(5, 5), (24, 24), (15, 15)] {
        rows[row] = format!("{}c{}", "g".repeat(col), "g".repeat(29 - col));
    }
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 5\nmoves 12\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n\
                 unit riders\nname Riders\nstrength 3\nmoves 20\nenter grass 2, city 1\n\
                 unit captain\nname Captain\nstrength 2\nmoves 12\ncommander 1\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\nincome 10\nproduce levy 1\n\
                  city 24 24\nname Bluemoor\nside blue\nincome 10\nproduce levy 1\n\
                  city 15 15\nname Midway\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
    place(&mut game);
    let mut setup = Setup::solo(&game.map, None, "passive");
    setup.set_players("red=passive,blue=passive").unwrap();
    (setup.mode, setup.combat.chance) = (Mode::Rounds, 0);
    set(&mut setup);
    game.with_setup(setup)
}

fn game(place: impl FnOnce(&mut Game)) -> Game {
    game_with(place, |_| {})
}

fn put(game: &mut Game, side: Side, kind: &str, hex: Hex) -> UnitId {
    let kind = game.map.units().kind_named(kind).unwrap();
    let id = game.map.new_unit_of(side, kind).unwrap();
    game.map.place_unit(id, hex).unwrap();
    id
}

fn pos(game: &Game, id: UnitId) -> Option<Hex> {
    game.map.units().get(id).map(|u| u.pos())
}

/// The steps units took: from where to where, in their order.
fn steps(events: &[Event]) -> Vec<(Hex, Hex)> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Moved { from, to, .. } => Some((*from, *to)),
            _ => None,
        })
        .collect()
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

/// A hex beside `hex` that is none of `not`.
fn beside(game: &Game, hex: Hex, not: &[Hex]) -> Hex {
    game.map.neighbours(hex).find(|n| !not.contains(n)).unwrap()
}

/// Both sides have done: the round plays out.
fn play(game: &mut Game) -> Vec<Event> {
    assert_eq!(game.player(), Side::Red);
    assert!(battles(&game.end_turn()).is_empty(), "red has done: blue gives its orders");
    assert_eq!(game.player(), Side::Blue);
    game.end_turn()
}

#[test]
fn an_order_to_move_is_a_plan_and_the_round_plays_it_out() {
    let mut a = None;
    let mut g = game(|g| a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5))));
    let a = a.unwrap();
    assert!(g.rounds() && !g.playing_out());
    let to = Hex::new(10, 9);
    assert_eq!(g.apply(Order::Move { units: vec![a], to }), Ok(Vec::new()));
    // Nothing has moved: it has a plan, and its player has seen to it.
    let unit = g.map.units().get(a).unwrap();
    assert_eq!((unit.pos(), unit.movement, unit.planned), (Hex::new(10, 5), 12, true));
    assert_eq!(unit.route, [to]);
    // Its way is the one it will take, hex by hex.
    assert_eq!((unit.way.len(), unit.way.last()), (4, Some(&to)));
    assert!(g.waiting_stacks(Side::Red).is_empty());
    // A hex it cannot get to this turn is refused, as a move is; farther is
    // Toward's, or Go to's.
    let far = Hex::new(10, 20);
    let refused = g.apply(Order::Move { units: vec![a], to: far });
    assert_eq!(refused, Err("Soldiers cannot get there this turn".to_string()));
    // The round: four steps, one after the other, then it has no plan left.
    let events = play(&mut g);
    let unit = g.map.units().get(a).unwrap();
    assert_eq!((unit.pos(), unit.movement, unit.planned), (to, 12 + 2, false), "there, and a new round's moves");
    assert!(unit.route.is_empty());
    let steps = steps(&events);
    assert_eq!(steps.len(), 4, "a hex at a time: {steps:?}");
    assert_eq!((steps[0].0, steps[3].1), (Hex::new(10, 5), to));
    assert!(steps.iter().all(|(from, to)| from.distance(*to) == 1));
    assert!(steps.windows(2).all(|pair| pair[0].1 == pair[1].0), "one after the other");
    assert_eq!((g.turn(), g.player()), (2, Side::Red));
    assert_eq!(events.last(), Some(&Event::Began { side: Side::Red, round: 2 }));
}

#[test]
fn a_plan_is_taken_back_and_another_takes_its_place() {
    let mut a = None;
    let mut g = game(|g| a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5))));
    let a = a.unwrap();
    g.apply(Order::Move { units: vec![a], to: Hex::new(10, 9) }).unwrap();
    assert!(g.can_undo());
    g.apply(Order::Undo).unwrap();
    let unit = g.map.units().get(a).unwrap();
    assert!(unit.route.is_empty() && !unit.planned && !unit.moved, "as it was");
    assert_eq!(g.waiting_stacks(Side::Red), [Hex::new(10, 5)]);
    // Another plan in its place; Defend ends it.
    g.apply(Order::Move { units: vec![a], to: Hex::new(10, 9) }).unwrap();
    g.apply(Order::Move { units: vec![a], to: Hex::new(12, 5) }).unwrap();
    assert_eq!(g.map.units().get(a).unwrap().route, [Hex::new(12, 5)]);
    g.apply(Order::Defend { units: vec![a], on: true }).unwrap();
    let unit = g.map.units().get(a).unwrap();
    assert!(unit.route.is_empty() && !unit.planned && unit.defending);
    play(&mut g);
    assert_eq!(pos(&g, a), Some(Hex::new(10, 5)), "it defends where it stands");
}

#[test]
fn a_stack_sent_against_another_attacks_it_where_it_stands() {
    let (mut a, mut b) = (None, None);
    let mut g = game(|g| {
        a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5)));
        b = Some(put(g, Side::Blue, "levy", Hex::new(10, 8)));
    });
    let (a, b) = (a.unwrap(), b.unwrap());
    g.apply(Order::Move { units: vec![a], to: Hex::new(10, 8) }).unwrap();
    let events = play(&mut g);
    assert_eq!(battles(&events), [(Hex::new(10, 8), Side::Red, true)]);
    assert_eq!((pos(&g, a), pos(&g, b)), (Some(Hex::new(10, 8)), None), "won, and in");
}

#[test]
fn a_stack_sent_against_one_that_has_left_finds_the_hex_empty() {
    let (mut a, mut b) = (None, None);
    let mut g = game(|g| {
        a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5)));
        b = Some(put(g, Side::Blue, "riders", Hex::new(10, 8)));
    });
    let (a, b) = (a.unwrap(), b.unwrap());
    g.apply(Order::Move { units: vec![a], to: Hex::new(10, 8) }).unwrap();
    g.end_turn();
    // Blue rides off, away from red.
    g.apply(Order::Move { units: vec![b], to: Hex::new(10, 14) }).unwrap();
    let events = g.end_turn();
    assert!(battles(&events).is_empty(), "nobody met");
    assert_eq!((pos(&g, a), pos(&g, b)), (Some(Hex::new(10, 8)), Some(Hex::new(10, 14))));
}

#[test]
fn two_stacks_making_for_one_hex_fight_there_and_the_first_in_the_order_holds_it() {
    // Both two hexes from the hex between them: red, first in the round's
    // order (drawn: so it falls, with this seed), steps in first; blue's
    // step into it is an attack.
    let (mut a, mut b) = (None, None);
    let mut g = game(|g| {
        a = Some(put(g, Side::Red, "levy", Hex::new(10, 5)));
        b = Some(put(g, Side::Blue, "soldiers", Hex::new(10, 9)));
    });
    let (a, b) = (a.unwrap(), b.unwrap());
    let between = Hex::new(10, 7);
    g.apply(Order::Move { units: vec![a], to: between }).unwrap();
    g.end_turn();
    g.apply(Order::Move { units: vec![b], to: between }).unwrap();
    let events = g.end_turn();
    assert_eq!(g.order()[0], Side::Red, "the round's draw");
    assert_eq!(battles(&events), [(between, Side::Blue, true)], "blue attacks red, who got there first");
    assert_eq!((pos(&g, a), pos(&g, b)), (None, Some(between)));
}

#[test]
fn two_stacks_that_would_swap_hexes_fight() {
    let (mut a, mut b) = (None, None);
    let mut g = game(|g| {
        a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5)));
        b = Some(put(g, Side::Blue, "levy", Hex::new(10, 6)));
    });
    let (a, b) = (a.unwrap(), b.unwrap());
    g.apply(Order::Move { units: vec![a], to: Hex::new(10, 6) }).unwrap();
    g.end_turn();
    g.apply(Order::Move { units: vec![b], to: Hex::new(10, 5) }).unwrap();
    let events = g.end_turn();
    // Red steps first (the round's draw): it attacks blue on blue's hex,
    // before blue has left.
    assert_eq!(g.order()[0], Side::Red, "the round's draw");
    assert_eq!(battles(&events), [(Hex::new(10, 6), Side::Red, true)]);
    assert_eq!((pos(&g, a), pos(&g, b)), (Some(Hex::new(10, 6)), None));
}

#[test]
fn an_empty_city_is_taken_by_the_first_there_and_the_next_attacks_it() {
    // Midway (15, 15): red's levy two hexes away, blue's soldiers three.
    let (mut a, mut b) = (None, None);
    let mut g = game(|g| {
        a = Some(put(g, Side::Red, "levy", Hex::new(15, 13)));
        b = Some(put(g, Side::Blue, "soldiers", Hex::new(15, 18)));
    });
    let (a, b) = (a.unwrap(), b.unwrap());
    let midway = Hex::new(15, 15);
    g.apply(Order::Move { units: vec![a], to: midway }).unwrap();
    g.end_turn();
    g.apply(Order::Move { units: vec![b], to: midway }).unwrap();
    let events = g.end_turn();
    let taken: Vec<(Side, Side)> = events
        .iter()
        .filter_map(|e| match e {
            Event::Taken { from, by, .. } => Some((*from, *by)),
            _ => None,
        })
        .collect();
    assert_eq!(taken, [(Side::Grey, Side::Red), (Side::Red, Side::Blue)], "red takes it, then blue from red");
    assert_eq!(battles(&events), [(midway, Side::Blue, true)]);
    assert_eq!((pos(&g, a), pos(&g, b)), (None, Some(midway)));
    assert_eq!(g.map.city_at(midway).map(|c| c.side), Some(Side::Blue));
}

/// The owner's word, 2026-11-22: "fast units should get to spend the same
/// MP as slower units faster than them ... if I have an enemy city 3 MP
/// away from a Cavalry stack and the enemy has a heavy infantry stack 3 MP
/// away the cavalry can attack and possibly take the city before the heavy
/// infantry". Blue is second in the round's order: a hex a tick for
/// everyone, red's soldiers were in Midway first.
#[test]
fn the_faster_stack_is_there_first_whatever_the_order_of_the_sides() {
    let (mut a, mut b) = (None, None);
    let mut g = game(|g| {
        a = Some(put(g, Side::Red, "soldiers", Hex::new(15, 12)));
        b = Some(put(g, Side::Blue, "riders", Hex::new(15, 18)));
    });
    let (a, b) = (a.unwrap(), b.unwrap());
    let midway = Hex::new(15, 15);
    g.apply(Order::Move { units: vec![a], to: midway }).unwrap();
    g.end_turn();
    g.apply(Order::Move { units: vec![b], to: midway }).unwrap();
    let events = g.end_turn();
    let taken: Vec<(Side, Side)> = events
        .iter()
        .filter_map(|e| match e {
            Event::Taken { from, by, .. } => Some((*from, *by)),
            _ => None,
        })
        .collect();
    // Three hexes each, 5 points: a quarter of a round for the riders (20
    // a turn), 5 twelfths for the soldiers (12).
    assert_eq!(
        taken,
        [(Side::Grey, Side::Blue), (Side::Blue, Side::Red)],
        "the riders take it, the soldiers retake it"
    );
    assert_eq!(battles(&events), [(midway, Side::Red, true)], "the soldiers attack the riders in it");
    assert_eq!((pos(&g, a), pos(&g, b)), (Some(midway), None));
    // The riders' three steps come before the soldiers' second.
    let order: Vec<Side> = events
        .iter()
        .filter_map(|e| match e {
            Event::Moved { unit, .. } => Some(unit.side),
            _ => None,
        })
        .collect();
    assert_eq!(order, [Side::Blue, Side::Red, Side::Blue, Side::Blue, Side::Red, Side::Red], "{order:?}");
}

/// A commander keeps up with those it leads: a captain (12 a turn) with
/// riders (20) is there as soon as riders alone would be, before soldiers
/// (12) as far away, who are first in the round's order.
#[test]
fn a_captain_with_riders_rides() {
    let (mut a, mut b) = (None, Vec::new());
    let mut g = game(|g| {
        a = Some(put(g, Side::Red, "soldiers", Hex::new(15, 12)));
        b = vec![put(g, Side::Blue, "captain", Hex::new(15, 18)), put(g, Side::Blue, "riders", Hex::new(15, 18))];
    });
    let midway = Hex::new(15, 15);
    g.apply(Order::Move { units: vec![a.unwrap()], to: midway }).unwrap();
    g.end_turn();
    g.apply(Order::Move { units: b.clone(), to: midway }).unwrap();
    let events = g.end_turn();
    let first = events.iter().find_map(|e| match e {
        Event::Taken { by, .. } => Some(*by),
        _ => None,
    });
    assert_eq!(first, Some(Side::Blue), "the captain and the riders take Midway first");
}

/// "Stacks that run into each other should probably fight vs just block
/// each other." Neither plan ends on the other's stack: red goes east
/// along row 10, blue west along it. A stack stopped short of one in its
/// way before (and went round one it saw as it set off).
#[test]
fn stacks_that_run_into_each_other_fight() {
    let mut a = None;
    let mut g = game(|g| a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5))));
    let a = a.unwrap();
    g.apply(Order::Move { units: vec![a], to: Hex::new(10, 9) }).unwrap();
    // Red's way, four hexes; blue's levy stands beside the third, and is
    // sent onto it: there long before red.
    let way = g.map.units().get(a).unwrap().way.clone();
    assert_eq!((way.len(), way.last()), (4, Some(&Hex::new(10, 9))));
    let near = beside(&g, way[2], &way);
    let b = put(&mut g, Side::Blue, "levy", near);
    g.end_turn();
    g.apply(Order::Move { units: vec![b], to: way[2] }).unwrap();
    let events = g.end_turn();
    // Red's third step is an attack: it does not go round, and it does not
    // wait; and that is its round.
    assert_eq!(battles(&events), [(way[2], Side::Red, true)]);
    assert_eq!((pos(&g, a), pos(&g, b)), (Some(way[2]), None));
    assert_eq!(g.map.units().get(a).unwrap().movement, 12, "an attack uses up its movement: the new round's");
    assert_eq!(
        steps(&events).iter().filter(|(_, to)| way.contains(to)).count(),
        4,
        "two steps, the levy's, the attack"
    );
}

/// Each stack attacks when it gets there: two orders against a hex are two
/// attacks, however near each other the stacks are.
#[test]
fn stacks_sent_against_a_hex_each_on_its_own_attack_it_one_after_the_other() {
    let mut red = Vec::new();
    let mut g = game_with(
        |g| {
            let target = Hex::new(10, 10);
            for _ in 0..2 {
                put(g, Side::Blue, "soldiers", target);
            }
            for hex in [Hex::new(10, 9), Hex::new(10, 11)] {
                red.push(put(g, Side::Red, "soldiers", hex));
                red.push(put(g, Side::Red, "soldiers", hex));
            }
        },
        |s| s.combat.system = hexlords::state::combat::System::Strength,
    );
    let target = Hex::new(10, 10);
    g.apply(Order::Move { units: red[..2].to_vec(), to: target }).unwrap();
    g.apply(Order::Move { units: red[2..].to_vec(), to: target }).unwrap();
    let events = play(&mut g);
    assert_eq!(battles(&events), [(target, Side::Red, false), (target, Side::Red, false)], "10 against 10, twice");
}

/// The owner's word, 2026-11-22: "If you want to consolidate stacks then
/// you would order one stack to another then combine them and order them
/// as a new stack. That would cause the stack being joined to wait for the
/// other units before moving or attacking."
#[test]
fn a_stack_sent_to_another_joins_it_and_they_go_on_as_one() {
    let (mut near, mut far) = (Vec::new(), Vec::new());
    let (here, target) = (Hex::new(10, 9), Hex::new(10, 11));
    let mut g = game_with(
        |g| {
            for _ in 0..2 {
                put(g, Side::Blue, "soldiers", target);
                near.push(put(g, Side::Red, "soldiers", here));
                far.push(put(g, Side::Red, "soldiers", Hex::new(10, 6)));
            }
        },
        |s| s.combat.system = hexlords::state::combat::System::Strength,
    );
    // The far stack is sent to the near one's hex; then all four are sent
    // on, from there.
    g.apply(Order::Move { units: far.clone(), to: here }).unwrap();
    assert_eq!(g.joining(Side::Red, here), far);
    let all: Vec<UnitId> = near.iter().chain(&far).copied().collect();
    g.apply(Order::Move { units: all.clone(), to: target }).unwrap();
    let unit = |id: UnitId| g.map.units().get(id).unwrap();
    assert_eq!((unit(near[0]).route.clone(), unit(far[0]).route.clone()), (vec![target], vec![here, target]));
    assert_eq!(unit(far[0]).way.len(), 5, "three hexes to the others, two on");
    assert!(all.iter().all(|id| unit(*id).party == unit(all[0]).party), "one order, one stack");
    let events = play(&mut g);
    // The near stack waited: the first to move are the far ones, and one
    // battle is fought, the four together (20 against 10).
    let moved: Vec<UnitId> = events
        .iter()
        .filter_map(|e| match e {
            Event::Moved { unit, .. } => Some(*unit),
            _ => None,
        })
        .collect();
    assert!(moved[..6].iter().all(|id| far.contains(id)), "the far stack's three steps first: {moved:?}");
    assert_eq!(battles(&events), [(target, Side::Red, true)]);
    let fought = events.iter().find_map(|e| match e {
        Event::Battle { attackers, defenders, .. } => Some((*attackers, *defenders)),
        _ => None,
    });
    assert_eq!(fought, Some((4, 2)));
    assert!(all.iter().all(|id| pos(&g, *id) == Some(target)));
}

/// Those that were to be joined go on alone if the others cannot come any
/// more; and an undo gives every one of them back what it was doing.
#[test]
fn a_stack_waits_for_those_that_join_it_only_while_they_can_come() {
    let (mut near, mut far) = (None, None);
    let (here, on) = (Hex::new(10, 9), Hex::new(10, 12));
    let mut g = game(|g| {
        near = Some(put(g, Side::Red, "soldiers", here));
        far = Some(put(g, Side::Red, "levy", Hex::new(10, 6)));
    });
    let (near, far) = (near.unwrap(), far.unwrap());
    g.apply(Order::Move { units: vec![far], to: here }).unwrap();
    g.apply(Order::Move { units: vec![near, far], to: on }).unwrap();
    g.apply(Order::Undo).unwrap();
    let unit = |g: &Game, id: UnitId| g.map.units().get(id).unwrap().clone();
    assert!(unit(&g, near).route.is_empty() && !unit(&g, near).planned, "the near one has no plan again");
    assert_eq!((unit(&g, far).route.clone(), unit(&g, far).way.len()), (vec![here], 3), "the far one its way there");
    g.apply(Order::Move { units: vec![near, far], to: on }).unwrap();
    // Blue's soldiers come to stand on the levy's way: it runs into them,
    // and never comes.
    let way = unit(&g, far).way.clone();
    let next_to = beside(&g, way[1], &way);
    let foe = put(&mut g, Side::Blue, "soldiers", next_to);
    g.end_turn();
    g.apply(Order::Move { units: vec![foe], to: way[1] }).unwrap();
    let events = g.end_turn();
    assert_eq!(battles(&events), [(way[1], Side::Red, false)]);
    assert_eq!(pos(&g, far), None, "the levy never came");
    assert_eq!(pos(&g, near), Some(on), "and the soldiers went on without it");
}

#[test]
fn a_march_fights_who_stands_in_its_way_too() {
    // In turns Go to never attacks. In rounds whoever is on a stack's way
    // when it gets there is fought, a plan's or a march's.
    let mut a = None;
    let mut g = game(|g| a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5))));
    let a = a.unwrap();
    let far = Hex::new(10, 20);
    g.apply(Order::GoTo { units: vec![a], to: far, via: Vec::new() }).unwrap();
    assert_eq!(pos(&g, a), Some(Hex::new(10, 5)), "not a step before the round");
    // Its way this round, as it finds it when it sets off; blue's levy
    // comes to stand on it.
    let way = g.ways_this_round(Side::Red);
    let near = beside(&g, way[2], &way);
    let b = put(&mut g, Side::Blue, "levy", near);
    g.end_turn();
    g.apply(Order::Move { units: vec![b], to: way[2] }).unwrap();
    let events = g.end_turn();
    assert_eq!(battles(&events), [(way[2], Side::Red, true)]);
    assert_eq!((pos(&g, a), pos(&g, b)), (Some(way[2]), None));
    let unit = g.map.units().get(a).unwrap();
    assert!(unit.route == [far] && !unit.planned && !unit.stalled, "it keeps its order");
}

#[test]
fn a_march_goes_on_round_after_round() {
    let mut a = None;
    let mut g = game(|g| a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5))));
    let a = a.unwrap();
    let far = Hex::new(10, 20);
    g.apply(Order::GoTo { units: vec![a], to: far, via: Vec::new() }).unwrap();
    let events = play(&mut g);
    let at = pos(&g, a).unwrap();
    assert_eq!((steps(&events).len(), at.distance(Hex::new(10, 5)), at.distance(far)), (6, 6, 9), "six hexes a round");
    play(&mut g);
    play(&mut g);
    assert_eq!(pos(&g, a), Some(far));
    assert!(g.map.units().get(a).unwrap().route.is_empty(), "there: its order ends");
}

#[test]
fn a_patrol_goes_its_way_with_the_round_and_attacks_at_its_odds() {
    let (mut a, mut b) = (None, None);
    let mut g = game(|g| {
        a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5)));
        b = Some(put(g, Side::Blue, "levy", Hex::new(12, 12)));
    });
    let (a, b) = (a.unwrap(), b.unwrap());
    let waypoints = vec![Hex::new(10, 9), Hex::new(10, 5)];
    g.apply(Order::Patrol { units: vec![a], waypoints, attack: Some(60) }).unwrap();
    assert_eq!(pos(&g, a), Some(Hex::new(10, 5)));
    let events = play(&mut g);
    // Four hexes out, two back: nobody in reach to attack.
    assert!(battles(&events).is_empty());
    let at = pos(&g, a).unwrap();
    assert_eq!((steps(&events).len(), at.distance(Hex::new(10, 5)), at.distance(Hex::new(10, 9))), (6, 2, 2));
    // Blue's levy comes within its reach as the next round plays out: it
    // goes for it, in that round.
    g.end_turn();
    g.apply(Order::Move { units: vec![b], to: Hex::new(11, 9) }).unwrap();
    let events = g.end_turn();
    assert_eq!(battles(&events).len(), 1, "{events:?}");
    assert_eq!(battles(&events)[0].1, Side::Red, "the patrol attacks");
    assert!(g.map.units().get(b).is_none() && g.map.units().get(a).is_some_and(|u| u.patrol.is_some() && u.fought));
}

#[test]
fn every_side_is_paid_and_builds_as_a_round_begins() {
    let mut g = game(|_| {});
    for side in [Side::Red, Side::Blue] {
        let city = g.map.cities().iter().find(|c| c.side == side).unwrap().id;
        assert_eq!(g.player(), side);
        g.apply(Order::Build { city, what: Some(0) }).unwrap();
        if side == Side::Red {
            g.end_turn();
        }
    }
    let gold = g.gold(Side::Red);
    let events = g.end_turn();
    let built: Vec<Side> = events
        .iter()
        .filter_map(|e| match e {
            Event::Built { unit, .. } => Some(unit.side),
            _ => None,
        })
        .collect();
    assert_eq!(built, [Side::Red, Side::Blue], "both, as round 2 begins");
    assert_eq!((g.turn(), g.player()), (2, Side::Red));
    assert!(g.gold(Side::Red) > gold && g.gold(Side::Red) == g.gold(Side::Blue), "taxes for both");
    // The history says whose each was.
    let turns: Vec<(u32, Side)> = g.history().iter().map(|r| (r.round, r.turn)).collect();
    assert_eq!(turns, [(2, Side::Red), (2, Side::Blue)]);
}

#[test]
fn a_round_watched_plays_out_a_beat_at_a_time_to_the_same_end() {
    let place = |g: &mut Game| {
        put(g, Side::Red, "soldiers", Hex::new(10, 5));
        put(g, Side::Blue, "soldiers", Hex::new(10, 12));
        put(g, Side::Blue, "levy", Hex::new(12, 7));
    };
    let orders = |g: &mut Game| {
        let units = |g: &Game, side: Side| g.map.units().of_side(side).map(|u| u.id()).collect::<Vec<_>>();
        let red = units(g, Side::Red);
        g.apply(Order::Move { units: red, to: Hex::new(10, 9) }).unwrap();
        g.end_turn();
        for id in units(g, Side::Blue) {
            g.apply(Order::Move { units: vec![id], to: Hex::new(10, 9) }).unwrap();
        }
    };
    // Without a screen: all at once.
    let mut whole = game(place);
    orders(&mut whole);
    let all = whole.end_turn();
    assert!(!whole.playing_out() && whole.turn() == 2);
    // Watched: the last side's word only begins it.
    let mut watched = game(place);
    watched.watch(true);
    orders(&mut watched);
    assert_eq!(watched.end_turn(), []);
    assert!(watched.playing_out());
    assert_eq!(watched.player(), Side::Grey, "nobody's turn");
    let refused = watched.apply(Order::EndTurn);
    assert_eq!(refused, Err("the round is playing out".to_string()));
    let (mut beats, mut events, mut clock) = (0, Vec::new(), 0);
    while watched.playing_out() {
        let beat = watched.play_out();
        // A battle ends a beat: a screen shows it before the round goes on.
        let fought = beat.iter().filter(|e| matches!(e, Event::Battle { .. })).count();
        assert!(
            fought <= 1 && (fought == 0 || matches!(beat.last(), Some(Event::Battle { .. } | Event::Moved { .. })))
        );
        if let Some(now) = watched.round_clock() {
            assert!(now >= clock, "the clock goes on");
            clock = now;
        }
        events.extend(beat);
        beats += 1;
    }
    assert!(beats >= 3, "{beats}");
    assert_eq!(events, all, "the same events");
    assert_eq!((watched.turn(), watched.player()), (2, Side::Red));
    let places = |g: &Game| g.map.units().iter().map(|u| (u.id(), u.pos(), u.movement)).collect::<Vec<_>>();
    assert_eq!(places(&watched), places(&whole));
    // The journal has every event, the planning's handover too.
    let journal: Vec<Event> = watched.take_happened().into_iter().map(|(_, e)| e).collect();
    assert_eq!(journal[0], Event::Began { side: Side::Blue, round: 1 });
    assert_eq!(journal[1..], all[..]);
}

/// The owner's word, 2026-11-22: "you should only see what the unit can
/// see at the time and leave ghosts vs leave a lingering visibility
/// trail".
#[test]
fn a_stack_sees_what_is_in_its_sight_as_it_goes_and_remembers_the_stacks_it_passed() {
    let mut a = None;
    let mut g = game_with(|g| a = Some(put(g, Side::Red, "riders", Hex::new(10, 5))), |s| s.fog = Fog::On);
    let a = a.unwrap();
    let far = Hex::new(10, 15);
    g.apply(Order::Move { units: vec![a], to: far }).unwrap();
    // The middle of its way: out of sight of where it is and where it will
    // be. Blue's levy stands beside it.
    let way = g.map.units().get(a).unwrap().way.clone();
    let middle = way[4];
    assert!(middle.distance(far) > hexlords::game::UNIT_SIGHT + 1 && middle.distance(REDHOLD) > 4);
    let seen = beside(&g, middle, &way);
    put(&mut g, Side::Blue, "levy", seen);
    assert!(g.sight(Side::Red).ghost_at(seen).is_none() && !g.sight(Side::Red).sees(seen));
    play(&mut g);
    assert_eq!(pos(&g, a), Some(far));
    let sight = g.sight(Side::Red);
    assert!(!sight.sees(middle) && !sight.sees(seen), "what it passed is seen no longer");
    let ghost = sight.ghost_at(seen).expect("the levy it passed is remembered");
    assert_eq!((ghost.side, ghost.count, ghost.round), (Side::Blue, 1, 1));
}

/// With total fog a plan could only end on land already seen, a couple of
/// hexes on: in rounds a stack explored two hexes a round, and the
/// computer's sides never found each other (2026-11-22: of 40 games, none
/// ended and each had one battle). A stack may be sent toward land its
/// side has never seen: it goes over what it has seen, and finds its way as
/// the round shows it more.
#[test]
fn under_total_fog_a_stack_sent_toward_unseen_land_finds_its_way_as_it_sees_it() {
    let mut a = None;
    let mut g = game_with(|g| a = Some(put(g, Side::Red, "riders", Hex::new(10, 5))), |s| s.fog = Fog::Total);
    let a = a.unwrap();
    let far = Hex::new(10, 20);
    let sight = g.sight(Side::Red);
    assert!(!sight.knows(far) && !sight.knows(Hex::new(10, 9)), "black: never seen");
    // A move there is refused, as ever; so is Go to. Toward it, it sets off.
    let unseen = Err(hexlords::game::UNSEEN.to_string());
    assert_eq!(g.apply(Order::Move { units: vec![a], to: far }), unseen);
    assert_eq!(g.apply(Order::GoTo { units: vec![a], to: far, via: Vec::new() }), unseen);
    assert_eq!(g.apply(Order::Toward { units: vec![a], to: far }), Ok(Vec::new()));
    let events = play(&mut g);
    // Ten hexes, its whole round: far past what its side knew when it set
    // off, each step onto land it had come to see.
    let at = pos(&g, a).unwrap();
    assert_eq!(steps(&events).len(), 10, "{at:?}");
    assert!(at.distance(far) <= 8 && !sight.knows(at), "nearer by half, on land it had not seen: {at:?}");
    assert!(g.map.units().get(a).unwrap().route.is_empty(), "the plan ended with the round");
}

#[test]
fn under_total_fog_the_computers_sides_find_each_other_in_rounds() {
    let (mut game, _) = Game::load(std::path::Path::new("assets/maps/continent")).unwrap();
    let mut setup = Setup::solo(&game.map, None, "steady");
    for seat in &mut setup.seats {
        seat.controller = hexlords::setup::Controller::Computer("steady".to_string());
    }
    (setup.mode, setup.fog, setup.seed) = (Mode::Rounds, Fog::Total, 2);
    game = game.with_setup(setup);
    let mut lines = Vec::new();
    headless::play(&mut game, 90, &mut |_, line| lines.push(line.to_string())).unwrap();
    let fights = lines.iter().filter(|l| l.contains(" attacks ") && !l.contains("attacks neutral")).count();
    let taken = lines.iter().filter(|l| l.contains(" takes ")).count();
    assert!(fights >= 3 && taken >= 6, "{fights} battles between sides, {taken} cities taken");
}

#[test]
fn agents_play_rounds_to_an_end_and_a_seed_plays_the_same_game() {
    let play = |seed: u64| {
        let (mut game, _) = Game::load(std::path::Path::new("assets/maps/continent")).unwrap();
        let mut setup = Setup::solo(&game.map, None, "steady");
        for seat in &mut setup.seats {
            seat.controller = hexlords::setup::Controller::Computer("steady".to_string());
        }
        (setup.mode, setup.seed) = (Mode::Rounds, seed);
        game = game.with_setup(setup);
        let mut lines = Vec::new();
        let outcome = headless::play(&mut game, 60, &mut |_, line| lines.push(line.to_string())).unwrap();
        (outcome.rounds, outcome.winner, lines)
    };
    let (rounds, _, lines) = play(3);
    assert!(rounds > 10, "{rounds}");
    assert!(lines.iter().any(|l| l.contains(" attacks ")), "they fight");
    assert!(lines.iter().any(|l| l.contains(" takes ")), "and take cities");
    assert_eq!(play(3).2, lines, "the same seed, the same game");
    assert_ne!(play(4).2, lines);
}

#[test]
fn a_program_is_told_the_game_is_played_in_rounds() {
    let g = game(|_| {});
    let text = describe(&g, Side::Red);
    assert_eq!(text.lines().nth(1), Some("mode rounds"));
    // Agents plan a side at a time; the last one's word plays the round.
    let mut g = game(|g| {
        put(g, Side::Red, "soldiers", REDHOLD);
        put(g, Side::Blue, "soldiers", BLUEMOOR);
    });
    let mut agents = Agents::for_game(&g).unwrap();
    agents::play_computers(&mut g, &mut agents);
    assert_eq!((g.turn(), g.player()), (2, Side::Red));
}

/// The owner's word, 2026-11-24: "unexpected combat leaves the victor out
/// of MPs as if they attacked". Blue's soldiers are on their way east when
/// red's riders, faster, attack them before their first step: blue holds,
/// and that is its round, as an attack would have been (it went on east
/// before).
#[test]
fn a_stack_that_holds_against_an_attack_takes_no_more_steps_that_round() {
    let (mut a, mut b) = (None, None);
    let mut g = game(|g| {
        a = Some(put(g, Side::Red, "riders", Hex::new(10, 7)));
        b = Some(put(g, Side::Blue, "soldiers", Hex::new(10, 8)));
    });
    let (a, b) = (a.unwrap(), b.unwrap());
    g.apply(Order::Move { units: vec![a], to: Hex::new(10, 8) }).unwrap();
    g.end_turn();
    g.apply(Order::Move { units: vec![b], to: Hex::new(10, 12) }).unwrap();
    let events = g.end_turn();
    assert_eq!(battles(&events), [(Hex::new(10, 8), Side::Red, false)], "the riders lose");
    assert_eq!((pos(&g, a), pos(&g, b)), (None, Some(Hex::new(10, 8))), "blue holds where it stood");
    assert!(steps(&events).iter().all(|(from, _)| *from != Hex::new(10, 8)), "{events:?}");
    // Its movement was spent: none carried over to the new round.
    let soldiers = g.map.units().get(b).unwrap();
    assert_eq!((soldiers.movement, soldiers.planned), (12, false));
}

/// The owner's word, 2026-11-24: in rounds the order of the sides is always
/// drawn, as each round plays out (it says who of two stacks that get
/// somewhere at the same moment is first). Set up "the same" or "drawn",
/// the game is the same; and the order changes.
#[test]
fn in_rounds_the_order_is_drawn_whatever_the_setup_says() {
    let play = |order: hexlords::setup::TurnOrder| {
        let (mut game, _) = Game::load(std::path::Path::new("assets/maps/continent")).unwrap();
        let mut setup = Setup::solo(&game.map, None, "steady");
        for seat in &mut setup.seats {
            seat.controller = hexlords::setup::Controller::Computer("steady".to_string());
        }
        (setup.mode, setup.seed, setup.order) = (Mode::Rounds, 5, order);
        assert!(setup.drawn());
        game = game.with_setup(setup);
        let first = game.order().to_vec();
        let mut orders = vec![first];
        for _ in 0..8 {
            headless::play(&mut game, 1, &mut |_, _| {}).unwrap();
            orders.push(game.order().to_vec());
        }
        (orders, game.digest())
    };
    let (fixed, digest) = play(hexlords::setup::TurnOrder::Fixed);
    assert_eq!(play(hexlords::setup::TurnOrder::Drawn), (fixed.clone(), digest), "the same game");
    assert!(fixed.windows(2).any(|w| w[0] != w[1]), "drawn anew: {fixed:?}");
    // The first round's orders are given in the seats' order.
    assert_eq!(fixed[0], [Side::Red, Side::Green, Side::Blue, Side::Yellow]);
}
