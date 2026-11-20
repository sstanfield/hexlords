//! The computer's players that plan (`planner`): their styles, how they read
//! the rules in effect, and that with fog of war they act on what their side
//! sees and nothing more.

use hexgame::agents::{self, Agents};
use hexgame::game::Game;
use hexgame::headless;
use hexgame::orders::Event;
use hexgame::planner::{self, Planner, STEADY};
use hexgame::setup::Setup;
use hexgame::state::combat::System;
use hexgame::state::scenario::{self, Scenario};
use hexgame::state::{Hex, Side};

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
        let mut game = game("garrison militia 2\ngarrison riders\n", |s| s.fog = fog);
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
