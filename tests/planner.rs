//! The computer's players that plan (`planner`): their styles, how they read
//! the rules in effect, and that with fog of war they act on what their side
//! sees and nothing more.

use hexgame::agents::{self, Agents};
use hexgame::game::Game;
use hexgame::headless;
use hexgame::orders::{Event, Order};
use hexgame::planner::{self, Planner, STEADY};
use hexgame::setup::{Fog, Setup};
use hexgame::state::combat::System;
use hexgame::state::scenario::{self, Scenario};
use hexgame::state::{Hex, Side, UnitId};

/// 30 x 30 grass: red's Redhold with `red`, blue's Bluemoor far off.
fn game(red: &str, setup: impl FnOnce(&mut Setup)) -> Game {
    let mut rows = vec!["g".repeat(30); 30];
    rows[5] = format!("{}c{}", "g".repeat(5), "g".repeat(24));
    rows[25] = format!("{}c{}", "g".repeat(25), "g".repeat(4));
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit militia\nname Militia\nstrength 2\nmoves 10\nprice 4\nupkeep 1\nenter grass 2, city 1\n\
                 unit riders\nname Riders\nstrength 4\nmoves 18\nprice 10\nupkeep 3\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 10\nenter grass 2, city 1\n";
    let cities = format!(
        "city 5 5\nname Redhold\nside red\nincome 30\nproduce militia 1\nproduce riders 3\n{red}\
         city 25 25\nname Bluemoor\nside blue\ngarrison levy\n"
    );
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(&cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut s = Setup::solo(&game.map, None, "steady");
    s.set_players("red=steady,blue=passive").unwrap();
    setup(&mut s);
    game.with_setup(s)
}

#[test]
fn a_unit_is_worth_what_the_rules_make_it() {
    let at = |chance: u8, system: System| {
        let game = game("", |s| (s.combat.chance, s.combat.system) = (chance, system));
        planner::worth(4, &game) / planner::worth(2, &game)
    };
    // With no chance in battle, twice the strength is worth many times more;
    // with all chance, hardly more; stacks that fight as one add up.
    assert!(at(0, System::Lines) > 10.0, "{}", at(0, System::Lines));
    assert!(at(10, System::Lines) < 1.5, "{}", at(10, System::Lines));
    assert!((at(5, System::Strength) - 2.0).abs() < 1e-9);
}

#[test]
fn what_its_cities_build_follows_the_rules() {
    let first_build = |chance: u8| {
        let mut game = game("", |s| s.combat.chance = chance);
        agents::play_turn(&mut game, &mut Planner::new(STEADY));
        game.map.city_at(Hex::new(5, 5)).unwrap().building
    };
    // Little chance: the strong riders; much chance: many militia.
    assert_eq!(first_build(1), Some(1));
    assert_eq!(first_build(10), Some(0));
}

#[test]
fn each_style_plays_a_game_to_its_end_and_the_same_game_from_the_same_seed() {
    for style in planner::STYLES {
        let play = |seed: u64| {
            let mut game = game("garrison militia\n", |s| {
                s.set_players(&format!("red={},blue=simple", style.name)).unwrap();
                s.seed = seed;
            });
            headless::play(&mut game, 150, &mut |_, _| {}).unwrap()
        };
        let (a, b) = (play(3), play(3));
        assert_eq!(a.winner, Some(Side::Red), "{}", style.name);
        assert_eq!((a.winner, a.rounds), (b.winner, b.rounds), "{}", style.name);
    }
    assert!(agents::by_name("steady").is_ok() && agents::by_name("bold").is_ok() && agents::by_name("careful").is_ok());
}

#[test]
fn with_fog_of_war_it_does_not_strike_what_it_cannot_see() {
    // A blue levy four hexes from Redhold: out of sight in the fog (a city
    // sees three), an easy kill to one that sees all.
    let fight = |fog: bool| {
        let mut game = game("garrison militia 2\ngarrison riders\n", |s| s.fog = if fog { Fog::On } else { Fog::Off });
        let levy = game.map.units().kind_named("levy").unwrap();
        let blue = game.map.new_unit_of(Side::Blue, levy).unwrap();
        game.map.place_unit(blue, Hex::new(5, 9)).unwrap();
        let mut agents = Agents::for_game(&game).unwrap();
        let red = agents.get_mut(Side::Red).unwrap();
        let events = agents::play_turn(&mut game, red).events;
        events.iter().any(|e| matches!(e, Event::Battle { at, .. } if *at == Hex::new(5, 9)))
    };
    assert!(fight(false), "it sees the levy, and takes it");
    assert!(!fight(true), "in the fog it does not know it is there");
}

// What the review found.

/// A road through water from Redhold, (5, 5), east; nothing else.
fn corridor(red: &str, others: &str, setup: impl FnOnce(&mut Setup)) -> Game {
    let mut rows = vec!["w".repeat(30); 30];
    rows[5] = format!("{}c{}c{}", "g".repeat(5), "g".repeat(18), "g".repeat(5));
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit militia\nname Militia\nstrength 2\nmoves 10\nprice 4\nupkeep 1\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 10\nenter grass 2, city 1\n";
    let cities = format!(
        "city 5 5\nname Redhold\nside red\nincome 30\nproduce militia 1\n{red}\
         city 5 24\nname Farfield\n{others}"
    );
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(&cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut s = Setup::solo(&game.map, None, "steady");
    s.set_players("red=steady").unwrap();
    s.fog = Fog::On;
    setup(&mut s);
    game.with_setup(s)
}

#[test]
fn it_hears_of_a_refusal_and_does_not_give_the_same_order_turn_after_turn() {
    // A blue levy on the road, unseen: the way east is shut, and the game
    // says so.
    let mut game = corridor("garrison militia 5\n", "garrison levy\n", |_| {});
    let levy = game.map.units().kind_named("levy").unwrap();
    let blue = game.map.new_unit_of(Side::Blue, levy).unwrap();
    game.map.place_unit(blue, Hex::new(5, 14)).unwrap();
    let mut steady = Planner::new(STEADY);
    let mut refused = 0;
    for _ in 0..8 {
        refused += agents::play_turn(&mut game, &mut steady).refused.len();
        game.apply(hexgame::orders::Order::EndTurn).ok();
        while game.player() != Side::Red {
            game.apply(hexgame::orders::Order::EndTurn).unwrap();
        }
    }
    assert!(refused <= 3, "{refused} refusals in 8 turns");
    // Going on as far as it knows, it came upon the levy, and it is gone:
    // the way is open.
    assert!(game.map.units().get(blue).is_none(), "the levy on the road is dealt with");
}

#[test]
fn a_city_it_cannot_see_it_goes_to_look_at_and_then_takes() {
    // Farfield, nobody's and empty, out of sight down the road; two militia
    // (fewer than steady's army) go and look, then take it.
    let mut game = corridor("garrison militia 3\n", "", |_| {});
    let (mut steady, mut battles) = (Planner::new(STEADY), 0);
    for _ in 0..12 {
        let events = agents::play_turn(&mut game, &mut steady).events;
        battles += events.iter().filter(|e| matches!(e, Event::Battle { .. })).count();
        if game.map.city_at(Hex::new(5, 24)).unwrap().side == Side::Red {
            break;
        }
        while game.player() != Side::Red {
            game.apply(hexgame::orders::Order::EndTurn).unwrap();
        }
    }
    assert_eq!(game.map.city_at(Hex::new(5, 24)).unwrap().side, Side::Red, "taken");
    assert_eq!(battles, 0);
}

#[test]
fn it_does_not_attack_blind() {
    // Farfield held by two levies, out of sight: the first a stack near it
    // does is look, not attack.
    let mut game = corridor("garrison militia 8\n", "garrison levy 2\n", |_| {});
    let mut steady = Planner::new(STEADY);
    for _ in 0..6 {
        let events = agents::play_turn(&mut game, &mut steady).events;
        let seen_before = game.sight(Side::Red).sees(Hex::new(5, 24));
        for event in &events {
            if let Event::Battle { at, .. } = event {
                assert!(*at != Hex::new(5, 24) || seen_before, "it attacked Farfield before it saw it");
            }
        }
        while game.player() != Side::Red {
            game.apply(hexgame::orders::Order::EndTurn).unwrap();
        }
    }
}

#[test]
fn a_battle_tried_in_the_view_says_nothing_of_the_game_s() {
    // An even fight, over seeds: in the view and in the game it goes one
    // way or the other, and not always the same way.
    let mut alike = 0;
    for seed in 0..40 {
        let mut game = game("garrison militia\n", |s| (s.seed, s.combat.chance) = (seed, 10));
        let kind = game.map.units().kind_named("militia").unwrap();
        let blue = game.map.new_unit_of(Side::Blue, kind).unwrap();
        game.map.place_unit(blue, Hex::new(6, 5)).unwrap();
        let red = game.map.units_at(Hex::new(5, 5))[0];
        let mut view = game.view_for(Side::Red);
        let try_it = |g: &mut Game| g.apply(hexgame::orders::Order::Move { units: vec![red], to: Hex::new(6, 5) });
        let (a, b) = (try_it(&mut view).unwrap(), try_it(&mut game).unwrap());
        let won = |events: &[Event]| events.iter().any(|e| matches!(e, Event::Battle { won: true, .. }));
        alike += usize::from(won(&a) == won(&b));
    }
    assert!(alike < 40, "the view foretold all 40");
}

#[test]
fn it_does_not_take_its_unblessed_into_another_side_s_stack_on_a_temple() {
    let mut game = game("garrison militia 3\n", |_| {});
    game.map.tile_mut(Hex::new(5, 8)).unwrap().kind = hexgame::state::TileType::Temple;
    let riders = game.map.units().kind_named("riders").unwrap();
    for _ in 0..6 {
        let blue = game.map.new_unit_of(Side::Blue, riders).unwrap();
        game.map.place_unit(blue, Hex::new(5, 8)).unwrap();
    }
    let events = agents::play_turn(&mut game, &mut Planner::new(planner::CAREFUL)).events;
    assert!(!events.iter().any(|e| matches!(e, Event::Battle { at, .. } if *at == Hex::new(5, 8))));
}

#[test]
fn worth_is_plain_arithmetic() {
    let at = |chance: u8| planner::worth(4, &game("", |s| s.combat.chance = chance));
    assert_eq!([at(5), at(0)], [16.0, 256.0]);
    // 4 to the 0.4th, its fifth root of 4 squared.
    assert!((at(9) - 16f64.powf(0.2)).abs() < 1e-9);
}

#[test]
fn under_total_fog_it_goes_to_see_and_finds_what_to_take() {
    for style in planner::STYLES {
        let mut game = game("garrison militia 2\ngarrison riders 2\n", |s| {
            s.set_players(&format!("red={},blue=simple", style.name)).unwrap();
            (s.fog, s.seed) = (Fog::Total, 5);
        });
        assert!(!game.sight(Side::Red).knows(Hex::new(25, 25)), "Bluemoor is not known at the start");
        let outcome = headless::play(&mut game, 150, &mut |_, _| {}).unwrap();
        assert_eq!(outcome.winner, Some(Side::Red), "{}", style.name);
    }
}

#[test]
fn a_stack_it_saw_near_its_city_keeps_its_defenders_home() {
    // Six militia in Redhold; six blue riders, seen four hexes off (or
    // never seen), go east into the fog on blue's turn. Red's next turn:
    let stayed = |seen: bool| {
        let mut game = game("garrison militia 6\n", |s| s.fog = Fog::On);
        let riders = game.map.units().kind_named("riders").unwrap();
        let start = if seen { Hex::new(5, 8) } else { Hex::new(5, 12) };
        let blue: Vec<UnitId> = (0..6)
            .map(|_| {
                let unit = game.map.new_unit_of(Side::Blue, riders).unwrap();
                game.map.place_unit(unit, start).unwrap();
                unit
            })
            .collect();
        game.apply(Order::EndTurn).unwrap();
        let away = Hex::new(start.row, start.col + 3);
        game.apply(Order::Move { units: blue, to: away }).unwrap();
        game.apply(Order::EndTurn).unwrap();
        assert!(!game.sight(Side::Red).sees(away));
        agents::play_turn(&mut game, &mut Planner::new(STEADY));
        game.map.units_at(Hex::new(5, 5)).len()
    };
    assert_eq!(stayed(true), 6, "the stack it remembers may come back: all stay");
    assert!(stayed(false) < 6, "with nothing seen, an army goes out");
}

/// A stack of another side's on the only way to its last city (a land
/// bridge one hex wide): the planner goes for that stack, and on to the
/// city. (It once waited for ever: a way never goes through a stack, so it
/// knew no way to any city, and the stack was too far to strike.)
#[test]
fn a_stack_in_the_only_way_is_gone_for() {
    let mut rows = vec!["g".repeat(30); 30];
    for row in rows.iter_mut().take(18).skip(13) {
        *row = format!("{}g{}", "w".repeat(15), "w".repeat(14));
    }
    rows[5] = format!("{}c{}", "g".repeat(5), "g".repeat(24));
    rows[25] = format!("{}c{}", "g".repeat(25), "g".repeat(4));
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit militia\nname Militia\nstrength 2\nmoves 10\nprice 4\nupkeep 1\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 10\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\nincome 30\ngarrison militia 8\n\
                  city 25 25\nname Bluemoor\nside blue\ngarrison levy\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let levy = game.map.units().kind_named("levy").unwrap();
    for _ in 0..2 {
        let unit = game.map.new_unit_of(Side::Blue, levy).unwrap();
        game.map.place_unit(unit, Hex::new(15, 15)).unwrap();
    }
    let mut s = Setup::solo(&game.map, None, "steady");
    s.set_players("red=steady,blue=passive").unwrap();
    let mut game = game.with_setup(s);
    let outcome = headless::play(&mut game, 40, &mut |_, _| {}).unwrap();
    assert_eq!(outcome.winner, Some(Side::Red));
}
