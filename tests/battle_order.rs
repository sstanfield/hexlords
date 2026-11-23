//! The battle's window shows each stack the same way, however it stood and
//! however the battle is fought (the owner's word, 2026-11-22: "the combat
//! window should be consistent, probably order the units like in line mode
//! (even if not) from weakest to strongest with the captain always on one
//! side (probably right)").

mod common;

use hexlords::battle_window::line_order;
use hexlords::game::Game;
use hexlords::options_window::Options;
use hexlords::session::Session;
use hexlords::setup::Setup;
use hexlords::state::combat::System;
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{Hex, Side};
use turbo_ferret::Size;

const FROM: Hex = Hex::new(10, 9);
const HOLD: Hex = Hex::new(10, 10);

/// Red's stack next to blue's, each put down in an order that is not its
/// line's: the captain first, the strongest before the weakest.
fn game(system: System) -> Game {
    let mut rows = vec!["g".repeat(20); 20];
    rows[2] = format!("{}c{}", "g".repeat(2), "g".repeat(17));
    rows[17] = format!("{}c{}", "g".repeat(17), "g".repeat(2));
    let terrain = format!("20\n20\n{}\n", rows.join("\n"));
    let units = "unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n\
                 unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nenter grass 2, city 1\n\
                 unit knights\nname Knights\nstrength 7\nmoves 12\nenter grass 2, city 1\n\
                 unit captain\nname Captain\nstrength 2\nmoves 12\ncommander 1\nenter grass 2, city 1\n";
    let cities = "city 2 2\nname Redhold\nside red\ncity 17 17\nname Bluemoor\nside blue\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
    for (side, hex, kinds) in [
        (Side::Red, FROM, ["captain", "knights", "levy", "soldiers"].as_slice()),
        (Side::Blue, HOLD, ["soldiers", "captain", "levy"].as_slice()),
    ] {
        for kind in kinds {
            let kind = game.map.units().kind_named(kind).unwrap();
            let id = game.map.new_unit_of(side, kind).unwrap();
            game.map.place_unit(id, hex).unwrap();
        }
    }
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup.combat.system = system;
    game.with_setup(setup)
}

#[test]
fn each_stack_stands_from_its_weakest_to_its_strongest_with_its_commander_on_the_right() {
    for system in [System::Lines, System::Strength, System::Dice] {
        let mut s = Session::with_options(&common::assets(), Size::new(1024, 768), 96, game(system), Options::manual())
            .unwrap();
        for hex in [FROM, HOLD] {
            s.screen.view.center_on(hex);
            let p = common::pixel_of(&s, hex);
            common::click(&mut s, p);
        }
        let battle = s.battle().expect("the battle's window");
        let [red, blue] = battle.names(&s.screen.ui);
        // (A captain has a name of its own: the last card is the one
        // that is not a kind's.)
        let kinds = ["Levy", "Soldiers", "Knights"];
        assert_eq!(red[..3], kinds, "{system:?}: {red:?}");
        assert!(!kinds.contains(&red[3].as_str()), "{system:?}: the captain on the right: {red:?}");
        assert_eq!(blue[..2], ["Levy", "Soldiers"], "{system:?}: {blue:?}");
        assert!(!kinds.contains(&blue[2].as_str()), "{system:?}: {blue:?}");
    }
}

#[test]
fn the_hits_fall_on_the_cards_of_those_who_were_hit() {
    // The cards are moved, not the battle: once it is all shown, the cards
    // that are out are those of the units that died.
    let mut s =
        Session::with_options(&common::assets(), Size::new(1024, 768), 96, game(System::Lines), Options::manual())
            .unwrap();
    for hex in [FROM, HOLD] {
        s.screen.view.center_on(hex);
        let p = common::pixel_of(&s, hex);
        common::click(&mut s, p);
    }
    let mut t = 0;
    while s.battle().is_some_and(|b| !b.done()) && t < 120_000 {
        t += 100;
        s.frame(t);
    }
    let game = &s.screen.game;
    let fight = (0..game.history().len()).filter_map(|i| game.fight(i)).next_back().expect("a battle").clone();
    let hits = s.battle().unwrap().hits().clone();
    for (which, units) in [&fight.attackers, &fight.defenders].into_iter().enumerate() {
        assert_eq!(line_order(units).len(), units.len());
        for (i, unit) in units.iter().enumerate() {
            let dead = game.map.units().get(unit.unit).is_none();
            assert_eq!(hits[which][i] >= hexlords::state::combat::HITS, dead, "{}", unit.title);
        }
    }
}
