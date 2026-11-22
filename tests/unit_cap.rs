//! A cap on a unit's strength (the owner's idea, 2026-11-21: "cap each
//! individual unit (with bonusses) to 9 in strength ... That might control
//! stacks a bit but still allow a strong stack to shine"): a choice made
//! with the game, `combat::Rules::cap`, none unless chosen.

mod common;

use hexlords::agents::describe;
use hexlords::game::Game;
use hexlords::guide;
use hexlords::options_window::Options;
use hexlords::orders::Order;
use hexlords::planner;
use hexlords::session::Session;
use hexlords::setup::Setup;
use hexlords::state::combat::QUARTER;
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{Hex, Side, UnitId};
use turbo_ferret::Size;

const FROM: Hex = Hex::new(10, 9);
const HOLD: Hex = Hex::new(10, 10);

/// 20 x 20 grass: red's giant (strength 12) next to blue's two guards
/// (strength 4), with the cap `cap`.
fn game(cap: Option<u32>) -> (Game, UnitId) {
    let mut rows = vec!["g".repeat(20); 20];
    rows[2] = format!("{}c{}", "g".repeat(2), "g".repeat(17));
    rows[17] = format!("{}c{}", "g".repeat(17), "g".repeat(2));
    let terrain = format!("20\n20\n{}\n", rows.join("\n"));
    let units = "unit giant\nname Giant\nstrength 12\nmoves 12\nenter grass 2, city 1\n\
                 unit guard\nname Guard\nstrength 4\nmoves 12\nenter grass 2, city 1\n";
    let cities = "city 2 2\nname Redhold\nside red\ncity 17 17\nname Bluemoor\nside blue\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut giant = None;
    for (side, kind, hex) in [(Side::Red, "giant", FROM), (Side::Blue, "guard", HOLD), (Side::Blue, "guard", HOLD)] {
        let kind = game.map.units().kind_named(kind).unwrap();
        let id = game.map.new_unit_of(side, kind).unwrap();
        game.map.place_unit(id, hex).unwrap();
        giant = giant.or(Some(id));
    }
    let mut setup = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup.combat.cap = cap;
    (game.with_setup(setup), giant.unwrap())
}

#[test]
fn a_unit_fights_no_stronger_than_the_cap() {
    let (mut capped, giant) = game(Some(9));
    let (whole, _) = game(None);
    // Its odds are those of a 9, and it is weighed as one.
    let odds = |g: &Game| g.battle_odds(&[giant], HOLD).unwrap();
    assert!(odds(&capped) < odds(&whole), "{} against {}", odds(&capped), odds(&whole));
    assert_eq!(planner::worth(12, &capped), planner::worth(9, &capped));
    assert!(planner::worth(12, &whole) > planner::worth(9, &whole));
    // It fought as a 9.
    let _ = capped.apply(Order::Move { units: vec![giant], to: HOLD });
    let fight = (0..capped.history().len()).filter_map(|i| capped.fight(i)).next_back().unwrap();
    assert_eq!(fight.attackers[0].quarters, 9 * QUARTER);
    assert_eq!(fight.rules.cap, Some(9));
}

#[test]
fn the_battle_window_says_a_unit_is_held_to_the_cap() {
    let (game, _) = game(Some(9));
    let mut s = Session::with_options(&common::assets(), Size::new(1024, 768), 96, game, Options::manual()).unwrap();
    for hex in [FROM, HOLD] {
        s.screen.view.center_on(hex);
        let p = common::pixel_of(&s, hex);
        common::click(&mut s, p);
    }
    assert!(s.battle().is_some());
    let ui = &s.screen.ui;
    let text = |id: &str| ui.find(ui.root(), turbo_ferret::TbId::new(id)).and_then(|w| ui.text(w));
    assert_eq!(text("card_a_0_strength"), Some("9 (cap)"));
    assert_eq!(text("card_d_0_strength"), Some("4"));
}

#[test]
fn the_cap_is_told_to_programs_and_in_the_help() {
    let (game, _) = game(Some(9));
    assert_eq!(describe(&game, Side::Red).lines().nth(2), Some("battles lines chance 5 cap 9"));
    let words = |g: &Game| {
        let (_, sections) = guide::topic(g, "help_battles").unwrap();
        sections.iter().flat_map(|sec| sec.rows.iter().map(|(_, w)| w.clone())).collect::<Vec<_>>().join(" ")
    };
    assert!(words(&game).contains("No unit fights stronger than 9 in this game"), "{}", words(&game));
    let (whole, _) = self::game(None);
    assert!(words(&whole).contains("No cap"));
    assert_eq!(describe(&whole, Side::Red).lines().nth(2), Some("battles lines chance 5"));
}
