//! Wounds that last (the owner's word, 2026-11-21): "damage to units should
//! only heal at the end of that side's turn ... if you are attacked in waves
//! on the other player's turn your damage won't heal until the end of your
//! turn", and a wounded unit fights at half its strength, rounded up. A
//! choice made with the game (`combat::Rules::wounds`), off unless chosen.

mod common;

use hexlords::game::Game;
use hexlords::info;
use hexlords::options_window::Options;
use hexlords::orders::Order;
use hexlords::session::Session;
use hexlords::setup::Setup;
use hexlords::state::combat::{self, System};
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{Hex, Side, UnitId};
use turbo_ferret::Size;

const HOLD: Hex = Hex::new(10, 10);

/// 20 x 20 grass; red's soldiers (strength 5) on `HOLD`, blue's levies
/// (strength 1) around it, each to attack in turn; strength counts for
/// little (chance 8), so the levies land hits.
fn game(wounds: bool, system: System, seed: u64) -> (Game, Vec<UnitId>, Vec<UnitId>) {
    let mut rows = vec!["g".repeat(20); 20];
    rows[2] = format!("{}c{}", "g".repeat(2), "g".repeat(17));
    rows[17] = format!("{}c{}", "g".repeat(17), "g".repeat(2));
    let terrain = format!("20\n20\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 5\nmoves 12\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n";
    let cities = "city 2 2\nname Redhold\nside red\ncity 17 17\nname Bluemoor\nside blue\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut put = |side: Side, kind: &str, hex: Hex| {
        let kind = game.map.units().kind_named(kind).unwrap();
        let id = game.map.new_unit_of(side, kind).unwrap();
        game.map.place_unit(id, hex).unwrap();
        id
    };
    let reds: Vec<UnitId> = (0..3).map(|_| put(Side::Red, "soldiers", HOLD)).collect();
    let around = [Hex::new(9, 10), Hex::new(11, 10), Hex::new(10, 9), Hex::new(10, 11), Hex::new(11, 11)];
    let blues: Vec<UnitId> = around.iter().map(|h| put(Side::Blue, "levy", *h)).collect();
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    (setup.combat.chance, setup.combat.system, setup.combat.wounds, setup.seed) = (8, system, wounds, seed);
    (game.with_setup(setup), reds, blues)
}

fn wounds(game: &Game, ids: &[UnitId]) -> u32 {
    ids.iter().filter_map(|id| game.map.units().get(*id)).map(|u| u.wounds).sum()
}

/// Blue's levies attack red's stack one after the other in blue's turn.
fn waves(game: &mut Game, blues: &[UnitId]) {
    game.end_turn();
    assert_eq!(game.player(), Side::Blue);
    for levy in blues {
        let _ = game.apply(Order::Move { units: vec![*levy], to: HOLD });
    }
}

/// A game (the first seed that makes one) whose waves hit red's stack and
/// left it standing, and its units.
fn after_waves(wounds: bool, system: System) -> (Game, Vec<UnitId>) {
    for seed in 1..200 {
        let (mut game, reds, blues) = game(wounds, system, seed);
        waves(&mut game, &blues);
        let held = reds.iter().any(|id| game.map.units().get(*id).is_some());
        let fights = (0..game.history().len()).filter_map(|i| game.fight(i));
        let living = |blow: &&combat::Exchange| !blow.hit_attacker;
        let hit =
            fights.flat_map(|f| f.trace.iter().filter(living).map(|b| f.defenders[b.unit].unit)).collect::<Vec<_>>();
        if held && hit.iter().any(|id| game.map.units().get(*id).is_some()) {
            return (game, reds);
        }
    }
    panic!("no seed makes it");
}

#[test]
fn wounds_taken_in_the_others_turn_last_through_the_sides_own_and_heal_as_it_ends() {
    for system in System::ALL {
        let (mut game, reds) = after_waves(true, system);
        let hurt = wounds(&game, &reds);
        let wounded = reds.iter().filter_map(|id| game.map.units().get(*id)).find(|u| u.wounds > 0).unwrap();
        let shows = format!("{}* ", combat::wounded(wounded.power()));
        assert!(info::unit_numbers(wounded).starts_with(&shows), "{}", info::unit_numbers(wounded));
        // Blue's turn ends: red's wounds stay through red's turn.
        game.end_turn();
        assert_eq!(game.player(), Side::Red);
        assert_eq!(wounds(&game, &reds), hurt, "{system:?}: still wounded in its own turn");
        // Red's turn ends: they heal.
        game.end_turn();
        assert_eq!(wounds(&game, &reds), 0, "{system:?}: whole again");
    }
}

#[test]
fn without_wounds_that_last_a_unit_is_whole_after_each_battle() {
    for system in System::ALL {
        // Red held, and those of it that were hit and lived are whole.
        let (game, reds) = after_waves(false, system);
        assert_eq!(wounds(&game, &reds), 0, "{system:?}");
    }
}

#[test]
fn a_wounded_stack_is_easier_to_beat() {
    let (mut game, reds) = after_waves(true, System::Strength);
    assert!(wounds(&game, &reds) > 0);
    // Blue's soldiers against the stack wounded, then the same stack healed.
    let kind = game.map.units().kind_named("soldiers").unwrap();
    let mut probe = Vec::new();
    for _ in 0..3 {
        let id = game.map.new_unit_of(Side::Blue, kind).unwrap();
        game.map.place_unit(id, Hex::new(9, 10)).unwrap();
        probe.push(id);
    }
    let wounded = game.battle_odds(&probe, HOLD).unwrap();
    for id in &reds {
        if let Some(unit) = game.map.unit_mut(*id) {
            unit.wounds = 0;
        }
    }
    let whole = game.battle_odds(&probe, HOLD).unwrap();
    assert!(wounded > whole + 5, "{wounded} against {whole}");
}

/// The review (2026-11-21): nobody's units have no turn, so they never
/// healed; a garrison hit once fought at half strength for the rest of the
/// game. Theirs heal as a round ends.
#[test]
fn nobodys_units_heal_as_the_round_ends() {
    let (mut game, _, _) = game(true, System::Lines, 1);
    let kind = game.map.units().kind_named("soldiers").unwrap();
    let grey = game.map.new_unit_of(Side::Grey, kind).unwrap();
    game.map.place_unit(grey, Hex::new(15, 3)).unwrap();
    game.map.unit_mut(grey).unwrap().wounds = 1;
    game.end_turn();
    assert_eq!(game.map.units().get(grey).unwrap().wounds, 1, "not in the round");
    game.end_turn();
    assert_eq!(game.turn(), 2);
    assert_eq!(game.map.units().get(grey).unwrap().wounds, 0, "as it ends");
}

/// The review (2026-11-21): the battle window said a unit hit earlier in
/// the battle fought whole; it fought wounded.
#[test]
fn the_battle_window_says_a_unit_hit_in_the_battle_fights_wounded() {
    let mut rows = vec!["g".repeat(20); 20];
    rows[2] = format!("{}c{}", "g".repeat(2), "g".repeat(17));
    rows[17] = format!("{}c{}", "g".repeat(17), "g".repeat(2));
    let terrain = format!("20\n20\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 5\nmoves 12\nenter grass 2, city 1\n\
                 unit guard\nname Guard\nstrength 4\nmoves 12\nenter grass 2, city 1\n";
    let cities = "city 2 2\nname Redhold\nside red\ncity 17 17\nname Bluemoor\nside blue\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
    for (side, kind, hex) in [(Side::Red, "soldiers", Hex::new(10, 9)), (Side::Blue, "guard", HOLD)] {
        let kind = game.map.units().kind_named(kind).unwrap();
        let id = game.map.new_unit_of(side, kind).unwrap();
        game.map.place_unit(id, hex).unwrap();
    }
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    // No chance: the 5 wins every exchange; the 4, once hit, is a 2.
    (setup.combat.chance, setup.combat.system, setup.combat.wounds) = (0, System::Lines, true);
    let options = Options::manual();
    let mut s =
        Session::with_options(&common::assets(), Size::new(1024, 768), 96, game.with_setup(setup), options).unwrap();
    for hex in [Hex::new(10, 9), HOLD] {
        s.screen.view.center_on(hex);
        let p = common::pixel_of(&s, hex);
        common::click(&mut s, p);
    }
    let mut said = Vec::new();
    let mut t = 0;
    while s.battle().is_some_and(|b| !b.done()) && t < 60_000 {
        t += 100;
        s.frame(t);
        let line = s.battle().unwrap().status(&s.screen.ui)[0].clone();
        if said.last() != Some(&line) && !line.is_empty() {
            said.push(line);
        }
    }
    let fought =
        ["Ready...", "Red's Soldiers (5) against blue's Guard (4).", "Red's Soldiers (5) against blue's Guard (2)."];
    assert_eq!(said, fought);
}
