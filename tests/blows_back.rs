//! A way of fighting to try (the owner's word, 2026-11-21): the loser of an
//! exchange may hit back, a quarter of the time at most
//! (`combat::Rules::blows_back`). (The dice, tried with it, were taken out
//! on 2026-11-24.)

mod common;

use hexlords::game::Game;
use hexlords::guide;
use hexlords::options_window::Options;
use hexlords::orders::Order;
use hexlords::session::Session;
use hexlords::setup::Setup;
use hexlords::state::combat::System;
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{Hex, Side, UnitId};
use turbo_ferret::Size;

const FROM: Hex = Hex::new(10, 9);
const HOLD: Hex = Hex::new(10, 10);

/// 20 x 20 grass: red's two soldiers (strength 5) next to blue's two guards
/// (strength 4), under `rules` and `seed`.
fn game(seed: u64, rules: impl Fn(&mut Setup)) -> (Game, Vec<UnitId>) {
    let mut rows = vec!["g".repeat(20); 20];
    rows[2] = format!("{}c{}", "g".repeat(2), "g".repeat(17));
    rows[17] = format!("{}c{}", "g".repeat(17), "g".repeat(2));
    let terrain = format!("20\n20\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 5\nmoves 12\nenter grass 2, city 1\n\
                 unit guard\nname Guard\nstrength 4\nmoves 12\nenter grass 2, city 1\n";
    let cities = "city 2 2\nname Redhold\nside red\ncity 17 17\nname Bluemoor\nside blue\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut reds = Vec::new();
    for (side, kind, hex) in [
        (Side::Red, "soldiers", FROM),
        (Side::Red, "soldiers", FROM),
        (Side::Blue, "guard", HOLD),
        (Side::Blue, "guard", HOLD),
    ] {
        let kind = game.map.units().kind_named(kind).unwrap();
        let id = game.map.new_unit_of(side, kind).unwrap();
        game.map.place_unit(id, hex).unwrap();
        if side == Side::Red {
            reds.push(id);
        }
    }
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup.seed = seed;
    rules(&mut setup);
    (game.with_setup(setup), reds)
}

/// Red attacks: the battle's record.
fn attack(game: &mut Game, reds: &[UnitId]) -> hexlords::game::Fight {
    let _ = game.apply(Order::Move { units: reds.to_vec(), to: HOLD });
    let mut fights = (0..game.history().len()).filter_map(|i| game.fight(i));
    fights.next_back().cloned().expect("a battle")
}

fn strength_back(s: &mut Setup) {
    (s.combat.system, s.combat.blows_back) = (System::Strength, true);
}

#[test]
fn the_battle_window_shows_the_loser_hitting_back() {
    // The first seed whose battle has a blow back in it.
    let seed = (1..200)
        .find(|seed| {
            let (mut game, reds) = game(*seed, strength_back);
            attack(&mut game, &reds).trace.iter().any(|b| b.back)
        })
        .expect("a seed");
    let (game, _) = game(seed, strength_back);
    let mut s = Session::with_options(&common::assets(), Size::new(1024, 768), 96, game, Options::manual()).unwrap();
    for hex in [FROM, HOLD] {
        s.screen.view.center_on(hex);
        let p = common::pixel_of(&s, hex);
        common::click(&mut s, p);
    }
    let mut said: Vec<[String; 2]> = Vec::new();
    let mut t = 0;
    while s.battle().is_some_and(|b| !b.done()) && t < 60_000 {
        t += 100;
        s.frame(t);
        let lines = s.battle().unwrap().status(&s.screen.ui);
        if said.last() != Some(&lines) {
            said.push(lines);
        }
    }
    // 10 against 8, the stacks' whole strengths.
    assert_eq!(said[1][0], "Red 10 against blue 8.", "{said:?}");
    // The loser hits back, and the winner is hit.
    let back = said.iter().position(|l| l[0].ends_with("hits back.")).expect("a blow back");
    let (fought, hit) = (&said[back - 1], &said[back]);
    let red_hit = hit[1].to_lowercase().contains("red's soldiers");
    assert!(red_hit || hit[1].to_lowercase().contains("blue's guard"), "{hit:?}");
    // The one that hits back is the side the exchange before hit.
    assert_eq!(hit[0], if red_hit { "Blue hits back." } else { "Red hits back." }, "{said:?}");
    assert!(fought[1].to_lowercase().contains(if red_hit { "blue's guard" } else { "red's soldiers" }), "{fought:?}");
}

#[test]
fn a_winner_hit_back_keeps_its_wound_when_wounds_last() {
    let rules = |s: &mut Setup| {
        (s.combat.system, s.combat.blows_back, s.combat.wounds) = (System::Strength, true, true);
    };
    let mut seen = 0;
    for seed in 1..100 {
        let (mut game, reds) = game(seed, rules);
        let fight = attack(&mut game, &reds);
        // Red's units the defenders hit back, and the hits each took.
        for (i, red) in fight.attackers.iter().enumerate() {
            let hits = fight.trace.iter().filter(|b| b.hit_attacker && b.unit == i).count();
            let backs = fight.trace.iter().filter(|b| b.back && b.hit_attacker && b.unit == i).count();
            if let (Some(unit), true) = (game.map.units().get(red.unit), backs > 0) {
                assert_eq!((hits, unit.wounds), (1, 1), "seed {seed}");
                seen += 1;
            }
        }
    }
    assert!(seen > 3, "{seen}");
}

#[test]
fn the_help_says_that_losers_hit_back() {
    let words = |rules: fn(&mut Setup)| {
        let (game, _) = game(1, rules);
        let (_, sections) = guide::topic(&game, "help_battles").unwrap();
        sections.iter().flat_map(|sec| sec.rows.iter().map(|(_, w)| w.clone())).collect::<Vec<_>>().join(" ")
    };
    let all = words(strength_back);
    assert!(all.contains("Strength against strength") && !all.contains("dice"), "{all}");
    assert!(all.contains("may hit back") && all.contains("never more than 25%"), "{all}");
    let all = words(|s| s.combat.system = System::Strength);
    assert!(all.contains("never hit"), "{all}");
}
