//! Rounds, the second way to play (`docs/gameplay.md`, "Rounds"; the
//! owner's word, 2026-11-22): every side gives its orders, nothing moves;
//! then the round plays out a tick at a time, every stack a hex in each,
//! the sides in the round's order. The rules, without a window.

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
                 unit riders\nname Riders\nstrength 3\nmoves 20\nenter grass 2, city 1\n";
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
    assert!(g.waiting_stacks(Side::Red).is_empty());
    // A hex it cannot get to this turn is refused, as a move is; farther is
    // Toward's, or Go to's.
    let far = Hex::new(10, 20);
    let refused = g.apply(Order::Move { units: vec![a], to: far });
    assert_eq!(refused, Err("Soldiers cannot get there this turn".to_string()));
    // The round: four steps, a tick each, then it has no plan left.
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
    // order, steps in first; blue's step into it is an attack.
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
    // Red steps first: it attacks blue on blue's hex, before blue has left.
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

#[test]
fn stacks_beside_the_hex_they_are_sent_against_attack_it_together() {
    // Three levies in Bluemoor... rather: blue's soldiers hold a hex; red's
    // two stacks of soldiers beside it are both sent against it. Alone,
    // each stack of one loses to the two (5 against 10); together they win.
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
    assert_eq!(battles(&events), [(target, Side::Red, true)], "one battle, 20 against 10");
    let fought = events.iter().find_map(|e| match e {
        Event::Battle { attackers, defenders, .. } => Some((*attackers, *defenders)),
        _ => None,
    });
    assert_eq!(fought, Some((4, 2)));
}

#[test]
fn a_march_stops_short_of_a_stack_in_its_way_and_a_plan_does_not() {
    // Go to never attacks what it sees: it goes round, or stops short.
    let (mut a, mut b) = (None, None);
    let mut g = game(|g| {
        a = Some(put(g, Side::Red, "soldiers", Hex::new(10, 5)));
        b = Some(put(g, Side::Blue, "levy", Hex::new(10, 8)));
    });
    let (a, b) = (a.unwrap(), b.unwrap());
    g.apply(Order::GoTo { units: vec![a], to: Hex::new(10, 8), via: Vec::new() }).unwrap();
    assert_eq!(pos(&g, a), Some(Hex::new(10, 5)), "not a step before the round");
    let events = play(&mut g);
    assert!(battles(&events).is_empty());
    assert_eq!((pos(&g, a), pos(&g, b)), (Some(Hex::new(10, 7)), Some(Hex::new(10, 8))), "beside it");
    assert!(events.iter().any(|e| matches!(e, Event::Stalled { side: Side::Red, .. })), "and it says so");
    let unit = g.map.units().get(a).unwrap();
    assert!(unit.stalled && unit.route == [Hex::new(10, 8)] && !unit.planned, "it keeps its order");
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
fn a_round_watched_plays_out_a_tick_at_a_time_to_the_same_end() {
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
    let (mut ticks, mut events) = (0, Vec::new());
    while watched.playing_out() {
        events.extend(watched.play_out());
        ticks += 1;
    }
    assert!(ticks >= 4, "{ticks}");
    assert_eq!(events, all, "the same events");
    assert_eq!((watched.turn(), watched.player()), (2, Side::Red));
    let places = |g: &Game| g.map.units().iter().map(|u| (u.id(), u.pos(), u.movement)).collect::<Vec<_>>();
    assert_eq!(places(&watched), places(&whole));
    // The journal has every event, the planning's handover too.
    let journal: Vec<Event> = watched.take_happened().into_iter().map(|(_, e)| e).collect();
    assert_eq!(journal[0], Event::Began { side: Side::Blue, round: 1 });
    assert_eq!(journal[1..], all[..]);
}

#[test]
fn what_units_see_on_their_way_stays_seen_while_the_next_orders_are_given() {
    let mut a = None;
    let mut g = game_with(|g| a = Some(put(g, Side::Red, "riders", Hex::new(10, 5))), |s| s.fog = Fog::On);
    let a = a.unwrap();
    let far = Hex::new(10, 15);
    g.apply(Order::Move { units: vec![a], to: far }).unwrap();
    let events = play(&mut g);
    assert_eq!(pos(&g, a), Some(far));
    // The middle of its way: out of sight of where it was and where it is.
    let middle = steps(&events)[4].1;
    assert!(middle.distance(far) > hexlords::game::UNIT_SIGHT && middle.distance(REDHOLD) > 4);
    assert!(g.sight(Side::Red).sees(middle), "its way, through the next orders");
    play(&mut g);
    assert!(!g.sight(Side::Red).sees(middle), "and no longer once that round has played");
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
