//! Turns, players and the end of the game, without a window: whose turn it
//! is, what a side may do on it, cities taken, a side out, the winner; and
//! the computer's agents that play, in the game and without a window.

use hexlords::agents::{self, Agents, describe, parse_order, unit_name};
use hexlords::game::Game;
use hexlords::headless::{self, order_line};
use hexlords::orders::{Event, Order};
use hexlords::setup::{Controller, Setup, TurnOrder};
use hexlords::state::combat::System;
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{Hex, Side};

const RED_HOME: Hex = Hex::new(5, 5);
const BLUE_HOME: Hex = Hex::new(5, 9);
const GREEN_HOME: Hex = Hex::new(14, 14);
const NOBODYS: Hex = Hex::new(12, 4);

/// Red's city with a rider in it, blue's and green's cities with nobody in
/// them (green's builds nothing), and an empty city that is nobody's. Every side is the computer's
/// passive agent unless `setup` says otherwise.
fn game(setup: impl FnOnce(&mut Setup)) -> Game {
    let mut rows = vec!["g".repeat(20); 20];
    rows[5] = "gggggcgggcgggggggggg".to_string();
    rows[12] = "ggggcggggggggggggggg".to_string();
    rows[14] = "ggggggggggggggcggggg".to_string();
    let terrain = format!("20\n20\n{}\n", rows.join("\n"));
    let units = "unit riders\nname Riders\nstrength 4\nmoves 12\nprice 5\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\nproduce riders 1\ngarrison riders\n\
                  city 5 9\nname Bluemoor\nside blue\nproduce riders 2\n\
                  city 14 14\nname Greenford\nside green\n\
                  city 12 4\nname Waste\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut s = Setup::solo(&game.map, None, "passive");
    s.seats.iter_mut().for_each(|seat| seat.controller = Controller::Computer("passive".to_string()));
    setup(&mut s);
    game.with_setup(s)
}

fn rider(game: &Game) -> hexlords::state::UnitId {
    game.map.units().of_side(Side::Red).next().unwrap().id()
}

#[test]
fn the_sides_play_in_the_seats_order_and_rounds_go_by() {
    let mut game = game(|_| {});
    assert_eq!(game.order(), [Side::Red, Side::Green, Side::Blue]);
    assert_eq!((game.player(), game.turn()), (Side::Red, 1));
    assert_eq!(game.end_turn(), [Event::Began { side: Side::Green, round: 1 }]);
    game.end_turn();
    assert_eq!(game.player(), Side::Blue);
    // After the last side, the first, in the next round.
    game.end_turn();
    assert_eq!((game.player(), game.turn()), (Side::Red, 2));
    assert_eq!(game.order(), [Side::Red, Side::Green, Side::Blue], "a fixed order stays");
}

#[test]
fn a_drawn_order_is_new_each_round_and_the_same_for_a_seed() {
    let orders = |seed: u64| -> Vec<Vec<Side>> {
        let mut game = game(|s| (s.order, s.seed) = (TurnOrder::Drawn, seed));
        (0..6)
            .map(|_| {
                let order = game.order().to_vec();
                for _ in 0..3 {
                    game.end_turn();
                }
                order
            })
            .collect()
    };
    let (a, b) = (orders(11), orders(11));
    assert_eq!(a, b, "one seed, one game");
    // Every round has every side once, and the rounds are not all alike.
    for order in &a {
        let mut sorted = order.clone();
        sorted.sort();
        assert_eq!(sorted, [Side::Red, Side::Green, Side::Blue]);
    }
    assert!(a.windows(2).any(|w| w[0] != w[1]), "{a:?}");
    assert_ne!(orders(11), orders(12));
}

#[test]
fn a_side_may_move_only_its_own_on_its_turn() {
    let mut game = game(|_| {});
    let id = rider(&game);
    let to = Hex::new(5, 7);
    // On green's turn, red's rider does not move.
    game.end_turn();
    let refused = game.apply(Order::Move { units: vec![id], to }).unwrap_err();
    assert_eq!(refused, "it is red's, and green's turn");
    assert!(!game.select_unit(id));
    // On red's, it does; too far, not.
    game.end_turn();
    game.end_turn();
    assert_eq!(game.player(), Side::Red);
    assert!(game.apply(Order::Move { units: vec![id], to: Hex::new(18, 18) }).is_err());
    let events = game.apply(Order::Move { units: vec![id], to }).unwrap();
    assert!(
        matches!(&events[..], [Event::Moved { unit, from: RED_HOME, to: t, cost: 4, .. }] if *unit == id && *t == to)
    );
    // Another side's city: only its holder sets what it builds.
    let blues = game.map.city_at(BLUE_HOME).unwrap().id;
    assert_eq!(game.apply(Order::Build { city: blues, what: Some(0) }).unwrap_err(), "only blue sets what it builds");
}

#[test]
fn a_city_builds_on_its_holders_turns() {
    let mut game = game(|_| {});
    let home = game.map.city_at(RED_HOME).unwrap().id;
    game.apply(Order::Build { city: home, what: Some(0) }).unwrap();
    // Round 1 is the start: nothing yet. Red's turn of round 2: a rider.
    for _ in 0..3 {
        game.end_turn();
    }
    assert_eq!((game.player(), game.turn()), (Side::Red, 2));
    assert_eq!(game.map.units_at(RED_HOME).len(), 2);
}

#[test]
fn taking_the_last_city_puts_a_side_out_and_the_last_side_wins() {
    let mut game = game(|_| {});
    let id = rider(&game);
    // Into nobody's empty city: taken, nobody is out.
    let mut events = game.apply(Order::Toward { units: vec![id], to: NOBODYS }).unwrap();
    while game.map.units().get(id).unwrap().pos() != NOBODYS {
        for _ in 0..3 {
            game.end_turn();
        }
        events = game.apply(Order::Toward { units: vec![id], to: NOBODYS }).unwrap();
    }
    let waste = game.map.city_at(NOBODYS).unwrap().id;
    assert!(events.contains(&Event::Taken { city: waste, from: Side::Grey, by: Side::Red, walls: None }), "{events:?}");
    assert_eq!(game.map.city_at(NOBODYS).unwrap().side, Side::Red);
    assert!(game.out().is_empty());
    // Blue's only city: blue is out, and its units go with it.
    let blue_unit = game.map.new_unit_of(Side::Blue, game.map.units().kind_named("riders").unwrap()).unwrap();
    game.map.place_unit(blue_unit, Hex::new(18, 0)).unwrap();
    let bluemoor = game.map.city_at(BLUE_HOME).unwrap().id;
    let mut events = Vec::new();
    for _ in 0..20 {
        if game.player() == Side::Red {
            if let Ok(e) = game.apply(Order::Toward { units: vec![id], to: BLUE_HOME }) {
                events.extend(e);
            }
        }
        if game.map.city_at(BLUE_HOME).unwrap().side == Side::Red {
            break;
        }
        game.end_turn();
    }
    assert!(events.contains(&Event::Taken { city: bluemoor, from: Side::Blue, by: Side::Red, walls: None }));
    assert!(events.contains(&Event::Out { side: Side::Blue }));
    assert_eq!(game.out(), [Side::Blue]);
    assert!(game.map.units().get(blue_unit).is_none(), "blue's units went with it");
    assert_eq!(game.winner(), None, "green is still in");
    // Blue's turns are skipped from now on.
    for _ in 0..6 {
        game.end_turn();
        assert_ne!(game.player(), Side::Blue);
    }
    // Green's, the last: red wins, and the game takes no more orders.
    let greenford = game.map.city_at(GREEN_HOME).unwrap().id;
    game.map.city_mut(greenford).unwrap().side = Side::Red;
    let city = game.map.city_at(NOBODYS).unwrap().id;
    game.map.city_mut(city).unwrap().side = Side::Green;
    while game.player() != Side::Red {
        game.end_turn();
    }
    let mut won = Vec::new();
    for _ in 0..20 {
        if game.player() == Side::Red {
            if let Ok(e) = game.apply(Order::Toward { units: vec![id], to: NOBODYS }) {
                won.extend(e);
            }
        }
        if game.winner().is_some() {
            break;
        }
        game.end_turn();
    }
    assert!(won.contains(&Event::Won { side: Side::Red }), "{won:?}");
    assert_eq!(game.winner(), Some(Side::Red));
    assert!(game.apply(Order::EndTurn).unwrap_err().starts_with("the game is over"));
}

#[test]
fn the_simple_agent_takes_what_it_can_and_a_game_without_a_window_ends() {
    let play = |seed: u64| {
        let mut game = game(|s| {
            s.set_players("red=simple,green=simple,blue=simple").unwrap();
            s.seed = seed;
        });
        let mut log = Vec::new();
        let outcome = headless::play(&mut game, 60, &mut |_, line| log.push(line.to_string())).unwrap();
        (outcome, log)
    };
    let (outcome, log) = play(5);
    // Red's rider is the only unit at the start: red takes the empty cities.
    assert_eq!(outcome.winner, Some(Side::Red), "{log:#?}");
    assert!(log.iter().any(|l| l == "red takes Waste from nobody"), "{log:#?}");
    assert!(log.iter().any(|l| l.ends_with("is out")));
    // The same seed, the same game.
    assert_eq!(play(5), (outcome.clone(), log));
    assert!(headless::summary(&outcome).starts_with("red wins in round"));
}

#[test]
fn a_person_cannot_play_without_a_window() {
    let mut with_a_person = game(|s| s.set_players("red=person").unwrap());
    let refused = headless::play(&mut with_a_person, 10, &mut |_, _| {}).unwrap_err();
    assert!(refused.contains("red is played by a person"), "{refused}");
    // An agent that is not there is an error too.
    let unknown = game(|s| s.set_players("blue=nobody-knows").unwrap());
    assert!(Agents::for_game(&unknown).err().unwrap().contains("\"nobody-knows\" is not an agent"));
    assert!(agents::by_name("cmd:").is_err());
}

#[test]
fn the_game_as_text_and_orders_from_text() {
    let game = game(|_| {});
    let id = rider(&game);
    let text = describe(&game, Side::Red);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "turn 1 red");
    assert_eq!(lines[1], "order red green blue");
    assert_eq!(lines[2], "battles lines chance 5");
    // Strength against strength; wounds; the loser hitting back.
    let strength = crate::game(|s| {
        s.combat.system = System::Strength;
        (s.combat.wounds, s.combat.blows_back) = (true, true);
    });
    let text = describe(&strength, Side::Red);
    assert_eq!(text.lines().nth(2), Some("battles strength chance 5 wounds last blows back"));
    assert_eq!(lines[3], "fog off");
    assert_eq!(lines[4], "gold 50 income 10 upkeep 0", "its treasury, its taxes, its upkeep");
    assert_eq!(lines[5], "terrain");
    assert_eq!(lines[6..8], ["20", "20"]);
    assert!(lines.contains(&"end terrain") && lines.last() == Some(&"end"));
    assert!(
        lines.contains(
            &"kind riders strength 4 moves 12 upkeep 0 price 5 commander 0 travels land carries 0 siege 0 bonus none \
              enter grass 2 city 1"
        ),
        "{text}"
    );
    assert!(lines.contains(&"city 5 5 red walls 0 income 10 builds none progress 0 name Redhold"), "{text}");
    assert!(lines.contains(&"can 5 5 0 riders 1 5"), "its own cities say what they can build");
    assert!(!lines.iter().any(|l| l.starts_with("can 5 9")), "others' do not");
    let unit = format!("unit {} red 5 5 riders strength 4 moves 12/12 level 1", unit_name(id));
    assert!(lines.contains(&unit.as_str()), "{text}");
    // Orders, and back.
    for line in [
        format!("move {} 5 7", unit_name(id)),
        format!("toward {} 12 4", unit_name(id)),
        format!("move {},{} 5 7", unit_name(id), unit_name(id)),
        "build 5 5 0".into(),
        "build 5 9 none".into(),
        "end".into(),
    ] {
        let order = parse_order(&game, &line).unwrap().unwrap();
        assert_eq!(order_line(&game, &order), line);
    }
    assert_eq!(parse_order(&game, "  # a comment").unwrap(), None);
    assert_eq!(parse_order(&game, "").unwrap(), None);
    assert!(parse_order(&game, "fly away").is_err());
    assert!(parse_order(&game, "move red.0.0 99 99").unwrap_err().contains("not on the map"));
    assert!(parse_order(&game, "move blah 5 5").unwrap_err().contains("not a unit's name"));
    assert!(parse_order(&game, "move red.0.0,blah 5 5").unwrap_err().contains("\"blah\" is not a unit's name"));
    assert!(parse_order(&game, "build 6 6 0").unwrap_err().contains("no city there"));
}
