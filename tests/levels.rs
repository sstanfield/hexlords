//! A little of a role-playing game: units rise in level from the battles
//! they win (regular, veteran, elite), a city may train its units a level
//! up, a temple blesses who stops on it once, captains have names and rise
//! in level too, and a stack with no captain that wins a hard battle may see
//! one rise from its ranks. What the panels and the log say of it.

mod common;

use common::{assets, click, pixel_of, type_char, widget};
use hexgame::game::{Game, emerge_chance};
use hexgame::info;
use hexgame::orders::{Event, Order};
use hexgame::session::Session;
use hexgame::setup::Setup;
use hexgame::state::scenario::{self, Scenario};
use hexgame::state::{Blessing, Hex, LEVEL_EXPERIENCE, Side, TileType, UnitId, level_for};
use hexgame::temple_window::{TEMPLE_BLESS_ID, TEMPLE_CLOSE_ID};
use turbo_ferret::{Point, Size};

const REDHOLD: Hex = Hex::new(5, 5);
const BLUEMOOR: Hex = Hex::new(5, 9);
const TEMPLE: Hex = Hex::new(7, 5);

/// Red's Redhold with `red`, Southwick with nobody; blue's Bluemoor with
/// `blue` and Farwater with nobody; a temple two hexes south of Redhold.
fn game(red: &str, blue: &str, setup: impl FnOnce(&mut Setup)) -> Game {
    let mut rows = vec!["g".repeat(20); 20];
    rows[5] = "gggggcgggcgggggggggg".to_string();
    rows[7] = "gggggtgggggggggggggg".to_string();
    rows[15] = "gggggcgggggggggcgggg".to_string();
    let terrain = format!("20\n20\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nprice 5\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n\
                 unit captain\nname Captain\nstrength 2\nmoves 12\ncommander 1\nenter grass 2, city 1\n";
    let cities = format!(
        "city 5 5\nname Redhold\nside red\n{red}city 15 5\nname Southwick\nside red\n\
         city 5 9\nname Bluemoor\nside blue\n{blue}city 15 15\nname Farwater\nside blue\n"
    );
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(&cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut s = Setup::solo(&game.map, Some(Side::Red), "passive");
    setup(&mut s);
    game.with_setup(s)
}

fn at(game: &Game, hex: Hex) -> Vec<UnitId> {
    game.map.units_at(hex).to_vec()
}

#[test]
fn levels_come_with_experience_and_make_a_unit_stronger() {
    assert_eq!(LEVEL_EXPERIENCE, [2, 5]);
    assert_eq!([0, 1, 2, 4, 5, 9].map(level_for), [1, 1, 2, 2, 3, 3]);
    let mut game = game("garrison soldiers\n", "", |_| {});
    let id = at(&game, REDHOLD)[0];
    let unit = game.map.unit_mut(id).unwrap();
    assert_eq!((unit.level, unit.power()), (1, 3));
    assert_eq!(unit.gain(1), None);
    assert_eq!(unit.gain(1), Some(2), "a veteran");
    assert_eq!((unit.level, unit.power()), (2, 4));
    assert_eq!(unit.gain(5), Some(3), "elite");
    assert_eq!(unit.gain(9), None, "no higher");
    // Strength in a battle counts the level (in quarters of a point).
    let unit = game.map.units().get(id).unwrap().clone();
    assert_eq!(game.battle_strength(&unit, REDHOLD, false, &[id]), 4 * 5);
}

#[test]
fn who_lives_through_a_battle_won_gains_experience() {
    // Three soldiers against a levy, no chance: they win, an easy win (a
    // point each).
    let mut game = game("garrison soldiers 3\n", "garrison levy\n", |s| s.combat.chance = 0);
    let red = at(&game, REDHOLD);
    game.apply(Order::Move { units: red.clone(), to: BLUEMOOR }).unwrap();
    assert!(red.iter().all(|id| game.map.units().get(*id).unwrap().experience == 1));
    // A second: veterans, and the log says so.
    game.map.city_mut(game.map.city_at(BLUEMOOR).unwrap().id).unwrap().side = Side::Red;
    let levy = game.map.units().kind_named("levy").unwrap();
    let blue = game.map.new_unit_of(Side::Blue, levy).unwrap();
    game.map.place_unit(blue, Hex::new(5, 11)).unwrap();
    // (An attack ends the move: the second is a round later.)
    next_round(&mut game);
    let events = game.apply(Order::Move { units: red.clone(), to: Hex::new(5, 11) }).unwrap();
    let promoted: Vec<&Event> = events.iter().filter(|e| matches!(e, Event::Promoted { .. })).collect();
    assert_eq!(promoted.len(), 3, "{events:?}");
    assert!(red.iter().all(|id| game.map.units().get(*id).unwrap().level == 2));
    let record = game.history().iter().find(|r| matches!(r.event, Event::Promoted { .. })).unwrap();
    assert_eq!(info::record_lines(&game.map, record, Side::Red), ["Your Soldiers rose to veteran."]);
    // The battle's outcome tells of it.
    let battle = game.history().iter().rposition(|r| matches!(r.event, Event::Battle { .. })).unwrap();
    let outcome = info::battle_outcome(&game, battle, Side::Red);
    assert!(outcome.contains(&"Your Soldiers rose to veteran. (\u{d7}3)".to_string()), "{outcome:?}");
}

#[test]
fn captains_have_names_and_rise_in_power() {
    let game = game("garrison captain\ngarrison soldiers\n", "garrison captain\n", |s| s.seed = 7);
    let names: Vec<String> = [REDHOLD, BLUEMOOR]
        .iter()
        .map(|h| game.map.units().get(at(&game, *h)[0]).unwrap().given_name.clone().expect("named"))
        .collect();
    assert_ne!(names[0], names[1], "two captains, two names");
    let again = self::game("garrison captain\ngarrison soldiers\n", "garrison captain\n", |s| s.seed = 7);
    assert_eq!(
        again.map.units().get(at(&again, REDHOLD)[0]).unwrap().given_name.as_ref(),
        Some(&names[0]),
        "a seed names them the same"
    );
    let captain = game.map.units().get(at(&game, REDHOLD)[0]).unwrap().clone();
    assert_eq!(captain.title(), format!("Captain {}", names[0]));
    // Its level: its own strength, and at the highest, a point more for its
    // stack.
    let mut game = game;
    let (cap, soldiers) = (captain.id(), at(&game, REDHOLD)[1]);
    let stack = at(&game, REDHOLD);
    let soldier = |game: &Game| {
        let unit = game.map.units().get(soldiers).unwrap().clone();
        game.battle_strength(&unit, Hex::new(5, 6), false, &stack)
    };
    assert_eq!(soldier(&game), 4 * (3 + 1));
    game.map.unit_mut(cap).unwrap().gain(5);
    assert_eq!(game.map.units().get(cap).unwrap().level, 3);
    assert_eq!(soldier(&game), 4 * (3 + 2), "a captain of level 3 commands 2");
    let unit = game.map.units().get(cap).unwrap().clone();
    assert_eq!(
        info::unit_line(&game.map, &unit),
        format!(
            "Captain {} (red), level 3: strength 4 (2 +2), moves 12/12, upkeep 0, price 0, commands (+2)",
            names[0]
        )
    );
}

/// The owner's rules (2026-11-20): going onto a temple blesses nobody; the
/// stack there seeks the blessing (`Order::Bless`), and each unit the
/// temple has not blessed is blessed and stops for the turn; one it has
/// blessed keeps its movement; another temple blesses it again.
#[test]
fn a_temple_blesses_who_seeks_it_once_each_temple() {
    let mut game = game("garrison soldiers 2\n", "", |s| s.seed = 3);
    let red = at(&game, REDHOLD);
    let before: Vec<(u32, u32)> =
        red.iter().map(|id| game.map.units().get(*id).map(|u| (u.power(), u.base_move_points)).unwrap()).collect();
    assert_eq!(game.map.tile(TEMPLE).unwrap().kind, TileType::Temple);
    // Going there blesses nobody, and leaves them their movement.
    let events = game.apply(Order::Move { units: red.clone(), to: TEMPLE }).unwrap();
    assert!(!events.iter().any(|e| matches!(e, Event::Blessed { .. })));
    let left = |game: &Game, id: UnitId| game.map.units().get(id).unwrap().movement;
    assert!(red.iter().all(|id| left(&game, *id) > 0));
    // Seeking it, the first alone: blessed by chance, and stopped for the
    // turn; the other is not, and keeps its movement.
    let events = game.apply(Order::Bless { units: vec![red[0]] }).unwrap();
    assert_eq!(events.iter().filter(|e| matches!(e, Event::Blessed { .. })).count(), 1);
    assert_eq!(left(&game, red[0]), 0, "it stops at the temple for the turn");
    assert!(left(&game, red[1]) > 0);
    // The next turn the stack seeks it: the second is blessed and stops; the
    // first, blessed here already, keeps its movement.
    next_round(&mut game);
    let full = left(&game, red[0]);
    let events = game.apply(Order::Bless { units: red.clone() }).unwrap();
    assert_eq!(events.iter().filter(|e| matches!(e, Event::Blessed { unit, .. } if *unit == red[1])).count(), 1);
    assert_eq!(events.len(), 1, "{events:?}");
    assert_eq!((left(&game, red[0]), left(&game, red[1])), (full, 0));
    for (i, id) in red.iter().enumerate() {
        let unit = game.map.units().get(*id).unwrap();
        let gained = match unit.blessings[0].1 {
            Blessing::Strength => (before[i].0 + 1, before[i].1),
            Blessing::Moves => (before[i].0, before[i].1 + 2),
        };
        assert_eq!((unit.power(), unit.base_move_points), gained);
    }
    // Chance had a say: no undo.
    assert!(!game.can_undo());
    // The same temple again, a turn later: nothing, and they keep their
    // movement.
    next_round(&mut game);
    let full: Vec<u32> = red.iter().map(|id| left(&game, *id)).collect();
    assert!(game.apply(Order::Bless { units: red.clone() }).unwrap_err().contains("blessed them all"));
    assert_eq!(red.iter().map(|id| left(&game, *id)).collect::<Vec<_>>(), full);
    // Another temple blesses them again.
    let other = Hex::new(7, 7);
    game.map.tile_mut(other).unwrap().kind = TileType::Temple;
    game.apply(Order::Move { units: red.clone(), to: other }).unwrap();
    let events = game.apply(Order::Bless { units: red.clone() }).unwrap();
    assert_eq!(events.iter().filter(|e| matches!(e, Event::Blessed { .. })).count(), 2);
    assert!(red.iter().all(|id| game.map.units().get(*id).unwrap().blessings.len() == 2));
    next_round(&mut game);
    let record = game.history().iter().find(|r| matches!(r.event, Event::Blessed { .. })).unwrap();
    assert!(
        info::record_lines(&game.map, record, Side::Red)[0].starts_with("Your Soldiers were blessed at a temple: +")
    );
}

/// The owner's rule (2026-11-20): an attack needs 2 movement points left
/// beside the enemy, and uses up the attackers' movement for the turn.
#[test]
fn an_attack_needs_two_points_left_and_ends_the_move() {
    let mut game = game("garrison soldiers 2\n", "garrison levy\n", |s| s.seed = 5);
    let red = at(&game, REDHOLD);
    // Blue's levy two hexes out, on grass, in reach.
    let blue = at(&game, BLUEMOOR)[0];
    let field = Hex::new(5, 7);
    game.map.move_unit(blue, field).unwrap();
    game.refresh();
    let beside = Hex::new(5, 6);
    for id in &red {
        game.map.unit_mut(*id).unwrap().movement = 3;
    }
    // From beside it with 1 point left: no attack.
    game.apply(Order::Move { units: red.clone(), to: beside }).unwrap();
    let left = game.map.units().get(red[0]).unwrap().movement;
    assert_eq!(left, 1, "grass costs soldiers 2");
    assert!(game.apply(Order::Move { units: red.clone(), to: field }).is_err());
    // With 2 left it attacks, and whatever comes of it, the move is over.
    for id in &red {
        game.map.unit_mut(*id).unwrap().movement = 2;
    }
    let events = game.apply(Order::Move { units: red.clone(), to: field }).unwrap();
    assert!(events.iter().any(|e| matches!(e, Event::Battle { .. })), "{events:?}");
    for id in &red {
        if let Some(unit) = game.map.units().get(*id) {
            assert_eq!(unit.movement, 0, "an attack uses up the movement");
        }
    }
}

fn next_round(game: &mut Game) {
    game.end_turn();
    game.end_turn();
}

#[test]
fn a_captain_may_rise_from_a_stack_that_wins_a_hard_fight_without_one() {
    // The chance, in percent: half of how unlikely the win was.
    assert_eq!([0, 20, 50, 90, 100].map(emerge_chance), [50, 40, 25, 5, 0]);
    // Two soldiers against two soldiers behind walls, with chance: a hard
    // fight. Over seeds, a captain rises now and then, never in a stack that
    // had one.
    let (mut rose, mut fights) = (0, 0);
    for seed in 0..120 {
        let mut game = game("garrison soldiers 2\n", "walls 3\ngarrison soldiers 2\n", |s| {
            s.seed = seed;
            s.combat.chance = 5;
        });
        let red = at(&game, REDHOLD);
        let odds = game.battle_odds(&red, BLUEMOOR).unwrap();
        assert!(odds < 50, "{odds}: a hard fight");
        let events = game.apply(Order::Move { units: red.clone(), to: BLUEMOOR }).unwrap();
        fights += 1;
        if let Some(Event::Emerged { unit, side, at, name }) =
            events.iter().find(|e| matches!(e, Event::Emerged { .. }))
        {
            rose += 1;
            let captain = game.map.units().get(*unit).expect("there");
            // Where the winners stand, named, with no moves this turn.
            assert_eq!((captain.pos(), captain.movement, captain.given_name.is_some()), (*at, 0, true));
            assert!(game.map.is_commander(*unit));
            assert!(name.starts_with("Captain "), "{name}");
            let won = events.iter().any(|e| matches!(e, Event::Battle { won: true, .. }));
            assert_eq!(*side, if won { Side::Red } else { Side::Blue });
        }
    }
    assert!(rose > 0 && rose < fights / 2, "{rose} of {fights}");
    // Led by a captain: never.
    for seed in 0..60 {
        let mut game = game("garrison soldiers 2\ngarrison captain\n", "walls 3\ngarrison soldiers 3\n", |s| {
            s.seed = seed;
            s.combat.chance = 5;
        });
        let red = at(&game, REDHOLD);
        let events = game.apply(Order::Move { units: red, to: BLUEMOOR }).unwrap();
        let won = events.iter().any(|e| matches!(e, Event::Battle { won: true, .. }));
        assert!(!won || !events.iter().any(|e| matches!(e, Event::Emerged { side: Side::Red, .. })), "seed {seed}");
    }
}

#[test]
fn a_city_that_trains_makes_veterans() {
    let mut game = game("trains veteran\nproduce soldiers 1\n", "", |s| s.gold = 50);
    let redhold = game.map.city_at(REDHOLD).unwrap().id;
    game.apply(Order::Build { city: redhold, what: Some(0) }).unwrap();
    next_round(&mut game);
    let made = game.map.units().get(at(&game, REDHOLD)[0]).unwrap();
    assert_eq!((made.level, made.power()), (2, 4));
    let city = game.map.city_at(REDHOLD).unwrap();
    let details = info::production_details(&game.map, &city.units[0], city.trains);
    assert!(details.contains(&"Trained here: it starts veteran".to_string()), "{details:?}");
}

#[test]
fn the_fallen_are_counted_and_captains_named() {
    let mut game = game("garrison soldiers 4\n", "garrison levy 2\ngarrison captain\n", |s| s.combat.chance = 0);
    let name = game.map.units().get(at(&game, BLUEMOOR)[2]).unwrap().title();
    let red = at(&game, REDHOLD);
    // Veterans (4), against levies of 1 led by the captain (2).
    for id in &red {
        game.map.unit_mut(*id).unwrap().level = 2;
    }
    game.apply(Order::Move { units: red, to: BLUEMOOR }).unwrap();
    let record = game.history().iter().find(|r| matches!(r.event, Event::Battle { .. })).unwrap();
    let lines = info::record_lines(&game.map, record, Side::Red);
    assert_eq!(lines[1], format!("You lost nothing; they lost 2 Levy, {name}."));
}

#[test]
fn the_panels_show_a_units_level_and_blessing() {
    let mut game = game("garrison soldiers\n", "", |_| {});
    let id = at(&game, REDHOLD)[0];
    let unit = game.map.unit_mut(id).unwrap();
    unit.gain(2);
    unit.bless(TEMPLE, Blessing::Strength);
    let unit = game.map.units().get(id).unwrap().clone();
    assert_eq!(info::unit_numbers(&unit), "5 \u{b7} 12/12");
    assert!(info::unit_line(&game.map, &unit).starts_with("Soldiers (red), veteran: strength 5 (3 +2)"));
    let details = info::unit_details(&game.map, &unit);
    assert!(
        details.contains(&"Experience 2 (next level at 5); blessed at a temple: +1 strength".to_string()),
        "{details:?}"
    );
    // On the screen, its slot's icon has a veteran's ring.
    let s = Session::with_game(&assets(), Size::new(1024, 768), 96, game).unwrap();
    assert_eq!(s.stack(), ["5 \u{b7} 12/12"]);
}

#[test]
fn the_ring_says_the_level() {
    use hexgame::render::map_renderer::{ELITE_RING, RING_COLOR, VETERAN_RING, level_ring};
    use hexgame::render::unit_icon::unit_icon;
    assert_eq!([1, 2, 3].map(|l| level_ring(l).0), [RING_COLOR, VETERAN_RING, ELITE_RING]);
    // The icon's edge, halfway down its left: the ring's color.
    let edge = |level: u8| {
        let icon = unit_icon(None, Side::Red, level, 40);
        let i = (20 * 40 + 1) * 4;
        [icon.rgba[i], icon.rgba[i + 1], icon.rgba[i + 2]]
    };
    let byte = |c: [f32; 4]| [c[0], c[1], c[2]].map(|v| (v * 255.0).round() as u8);
    assert_eq!(edge(2), byte(VETERAN_RING));
    assert_eq!(edge(3), byte(ELITE_RING));
    assert_ne!(edge(1), edge(2));
}

fn click_hex(s: &mut Session, hex: Hex) {
    s.screen.view.center_on(hex);
    let p = pixel_of(s, hex);
    click(s, p);
}

fn push(s: &mut Session, name: &str) {
    let r = s.screen.area(widget(s, name));
    click(s, Point::new(r.x + r.w / 2, r.y + r.h / 2));
}

/// The owner (2026-11-20): when a stack goes onto a temple, a box asks the
/// stack whether it seeks the blessing, and then lists each unit and what it
/// was given, or that it was blessed there before (and keeps its moves).
#[test]
fn a_stack_on_a_temple_is_asked_and_told_what_each_was_given() {
    let game = game("garrison soldiers 2\n", "", |s| s.seed = 3);
    let red = at(&game, REDHOLD);
    let mut s = Session::with_game(&assets(), Size::new(1024, 768), 96, game).unwrap();
    click_hex(&mut s, REDHOLD);
    click_hex(&mut s, TEMPLE);
    let asks = s.temple().expect("the temple asks");
    assert!(asks.asks());
    assert_eq!(asks.lines, ["Soldiers: not yet blessed here", "Soldiers: not yet blessed here"]);
    push(&mut s, TEMPLE_BLESS_ID);
    let told = s.temple().expect("it tells");
    assert!(!told.asks());
    assert_eq!(told.lines.len(), 2);
    assert!(told.lines.iter().all(|l| l == "Soldiers: +1 strength" || l == "Soldiers: +2 moves"), "{:?}", told.lines);
    assert!(red.iter().all(|id| s.screen.game.map.units().get(*id).unwrap().movement == 0));
    push(&mut s, TEMPLE_CLOSE_ID);
    assert!(s.temple().is_none());
    // Blessed there, the stack is not asked again: B does nothing.
    s.screen.game.end_turn();
    s.screen.game.end_turn();
    click_hex(&mut s, TEMPLE);
    type_char(&mut s, 'b');
    assert!(s.temple().is_none());
}
