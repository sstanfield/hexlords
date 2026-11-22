//! Battles in the game: an attack is a move into another side's units; who
//! wins, who dies, the city taken; what the ground, the walls and a commander
//! add; stacks that move together; the same battles from the same seed; the
//! computer attacking; and what the screen says of it (the odds, the report,
//! the news of a battle fought while one waited).

mod common;

use common::{assets, click, pixel_of, press, widget};
use hexlords::agents::Simple;
use hexlords::battle_window::BATTLE_WINDOW_ID;
use hexlords::game::Game;
use hexlords::headless;
use hexlords::orders::{Event, Order};
use hexlords::session::Session;
use hexlords::setup::{Controller, Setup};
use hexlords::state::combat::System;
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{Hex, MAX_STACK, Side, TileType, UnitId};
use turbo_ferret::{Point, Size, SpecialKey};

const REDHOLD: Hex = Hex::new(5, 5);
const BLUEMOOR: Hex = Hex::new(5, 9);

/// Red's city with `red` in it, blue's with `blue`, and another of blue's far
/// off with nobody in it (so taking Bluemoor does not end the game). Every
/// side is passive, the battles have no chance, unless `setup` says otherwise.
fn game(red: &str, blue: &str, setup: impl FnOnce(&mut Setup)) -> Game {
    game_with(red, blue, setup)
}

fn game_with(red: &str, blue: &str, setup: impl FnOnce(&mut Setup)) -> Game {
    let mut rows = vec!["g".repeat(20); 20];
    rows[5] = "gggggcgggcgggggggggg".to_string();
    rows[15] = "gggggggggggggggcgggg".to_string();
    let terrain = format!("20\n20\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nenter grass 2, hill 4, forest 3, mountain 6, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n\
                 unit captain\nname Captain\nstrength 2\nmoves 12\ncommander 1\nenter grass 2, city 1\n";
    let mut cities = format!("city 5 5\nname Redhold\nside red\n{red}city 5 9\nname Bluemoor\nside blue\n{blue}");
    cities.push_str("city 15 15\nname Farwater\nside blue\n");
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(&cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut s = Setup::solo(&game.map, None, "passive");
    s.seats.iter_mut().for_each(|seat| seat.controller = Controller::Computer("passive".to_string()));
    s.combat.chance = 0;
    setup(&mut s);
    game.with_setup(s)
}

fn at(game: &Game, hex: Hex) -> Vec<UnitId> {
    game.map.units_at(hex).to_vec()
}

fn battle(events: &[Event]) -> (usize, usize, usize, usize, bool) {
    events
        .iter()
        .find_map(|e| match e {
            Event::Battle { attackers, defenders, attackers_lost, defenders_lost, won, .. } => {
                Some((*attackers, *defenders, *attackers_lost, *defenders_lost, *won))
            }
            _ => None,
        })
        .expect("a battle")
}

#[test]
fn an_attack_that_wins_goes_in_and_takes_the_city() {
    let mut game = game("garrison soldiers 3\n", "garrison levy\n", |_| {});
    let red = at(&game, REDHOLD);
    let blue = at(&game, BLUEMOOR);
    let events = game.apply(Order::Move { units: red.clone(), to: BLUEMOOR }).unwrap();
    // Three of strength 3 against one of 1, with no chance: the levy dies
    // and the soldiers lose nobody.
    assert_eq!(battle(&events), (3, 1, 0, 1, true));
    assert!(game.map.units().get(blue[0]).is_none(), "the dead are gone");
    assert_eq!(at(&game, BLUEMOOR), red, "the winners went in");
    assert!(events.iter().any(|e| matches!(e, Event::Taken { from: Side::Blue, by: Side::Red, .. })));
    assert_eq!(game.map.city_at(BLUEMOOR).unwrap().side, Side::Red);
    // The attack used up their movement for the turn (the owner's rule,
    // 2026-11-20).
    assert!(red.iter().all(|id| game.map.units().get(*id).unwrap().movement == 0));
    assert!(game.winner().is_none(), "blue has a city yet");
}

#[test]
fn an_attack_that_loses_dies_and_the_defenders_hold() {
    let mut game = game("garrison levy 2\n", "garrison soldiers\n", |_| {});
    let (red, blue) = (at(&game, REDHOLD), at(&game, BLUEMOOR));
    let events = game.apply(Order::Move { units: red.clone(), to: BLUEMOOR }).unwrap();
    assert_eq!(battle(&events), (2, 1, 2, 0, false));
    assert!(red.iter().all(|id| game.map.units().get(*id).is_none()));
    assert_eq!(at(&game, BLUEMOOR), blue);
    assert_eq!(game.map.city_at(BLUEMOOR).unwrap().side, Side::Blue);
    assert!(!events.iter().any(|e| matches!(e, Event::Moved { .. } | Event::Taken { .. })));
    // Some of a stack can go, the rest stay; out of reach is refused.
    let mut game = self::game("garrison soldiers 2\n", "garrison levy\n", |_| {});
    let red = at(&game, REDHOLD);
    assert!(
        game.apply(Order::Move { units: red.clone(), to: Hex::new(5, 18) }).unwrap_err().contains("cannot get there")
    );
    game.apply(Order::Move { units: vec![red[1]], to: BLUEMOOR }).unwrap();
    assert_eq!(at(&game, REDHOLD), [red[0]]);
    assert_eq!(at(&game, BLUEMOOR), [red[1]]);
}

#[test]
fn the_ground_and_the_walls_make_defenders_stronger() {
    // Strengths in a battle are in quarters of a point; walls 1 or 2 count
    // a point, 3 to 5 two (`walls_bonus`).
    let mut game = game("garrison soldiers\n", "walls 2\ngarrison levy\n", |_| {});
    let levy = at(&game, BLUEMOOR)[0];
    let soldiers = at(&game, REDHOLD)[0];
    let strength = |game: &Game, id: UnitId, defending: bool| {
        let unit = game.map.units().get(id).unwrap();
        game.battle_strength(unit, unit.pos(), defending, &[id])
    };
    // In its city behind walls 2: 1 and 1. Who attacks has no walls.
    assert_eq!(strength(&game, levy, true), 4 + 4);
    assert_eq!(strength(&game, levy, false), 4);
    // Out on the ground: hills and forests 1, mountains 2, grass nothing.
    for (kind, bonus) in [(TileType::Grass, 0), (TileType::Hill, 1), (TileType::Forest, 1), (TileType::Mountain, 2)] {
        game.map.tile_mut(Hex::new(5, 7)).unwrap().kind = kind;
        let unit = game.map.units().get(soldiers).unwrap();
        assert_eq!(game.battle_strength(unit, Hex::new(5, 7), true, &[soldiers]), 4 * (3 + bonus), "{kind:?}");
    }
    // With no chance the stronger wins: 3 beats the levy's 2 behind walls
    // 2, and loses to its 3 behind walls 3 (a tie is the defender's).
    let events = game.apply(Order::Move { units: vec![soldiers], to: BLUEMOOR }).unwrap();
    assert_eq!(battle(&events), (1, 1, 0, 1, true));
    let mut walled = self::game("garrison soldiers\n", "walls 3\ngarrison levy\n", |_| {});
    let soldiers = at(&walled, REDHOLD)[0];
    let events = walled.apply(Order::Move { units: vec![soldiers], to: BLUEMOOR }).unwrap();
    assert_eq!(battle(&events), (1, 1, 1, 0, false));
}

#[test]
fn a_commander_leads_its_stack_fights_last_and_has_a_place_of_its_own() {
    let mut game = game("garrison soldiers\n", "garrison levy\ngarrison captain\ngarrison soldiers\n", |_| {});
    let blue = at(&game, BLUEMOOR);
    let (levy, captain, soldiers) = (blue[0], blue[1], blue[2]);
    let strength = |id: UnitId| {
        let unit = game.map.units().get(id).unwrap();
        game.battle_strength(unit, BLUEMOOR, true, &blue)
    };
    // Every other unit of its stack is 1 stronger; not the captain itself.
    // (In quarters of a point; walls 0 count nothing.)
    assert_eq!((strength(levy), strength(soldiers), strength(captain)), (8, 16, 8));
    assert!(game.map.is_commander(captain) && !game.map.is_commander(levy));
    // In lines the captain stands last: red's veteran 4 beats the levy (2),
    // then loses to the soldiers (4: a tie is the defender's), and the
    // captain never fights.
    let red = at(&game, REDHOLD);
    game.map.unit_mut(red[0]).unwrap().level = 2;
    let events = game.apply(Order::Move { units: red, to: BLUEMOOR }).unwrap();
    assert_eq!(battle(&events), (1, 3, 1, 1, false));
    assert_eq!(at(&game, BLUEMOOR), [captain, soldiers]);

    // A hex holds eight units and a commander besides, not two.
    let mut game = self::game("garrison soldiers 8\ngarrison captain\n", "", |_| {});
    assert_eq!(at(&game, REDHOLD).len(), MAX_STACK + 1);
    let kind = |game: &Game, name: &str| game.map.units().kind_named(name).unwrap();
    let (soldiers, captain) = (kind(&game, "soldiers"), kind(&game, "captain"));
    let one = game.map.new_unit_of(Side::Red, soldiers).unwrap();
    let leader = game.map.new_unit_of(Side::Red, captain).unwrap();
    assert!(game.map.place_unit(one, REDHOLD).is_err() && game.map.place_unit(leader, REDHOLD).is_err());
    assert!(game.map.place_unit(one, Hex::new(5, 6)).is_ok() && game.map.place_unit(leader, Hex::new(5, 6)).is_ok());
    // Nor does a move fit more in.
    let refused = game.apply(Order::Move { units: vec![leader], to: REDHOLD }).unwrap_err();
    assert!(refused.contains("no room"), "{refused}");
}

#[test]
fn the_systems_fight_differently() {
    // Five levies against one soldier, with no chance: in lines each levy
    // meets the soldier alone and loses; by strength their 5 beat its 3.
    let fight = |system| {
        let mut game = game("garrison levy 5\n", "garrison soldiers\n", |s| s.combat.system = system);
        let red = at(&game, REDHOLD);
        battle(&game.apply(Order::Move { units: red, to: BLUEMOOR }).unwrap()).4
    };
    assert!(!fight(System::Lines));
    assert!(fight(System::Strength));
}

#[test]
fn a_seed_fights_the_same_battles_and_the_odds_leave_it_alone() {
    let play = |seed: u64, ask: bool| {
        let mut game =
            game("garrison soldiers 4\n", "garrison soldiers 3\n", |s| (s.combat.chance, s.seed) = (5, seed));
        let red = at(&game, REDHOLD);
        if ask {
            let odds = game.battle_odds(&red, BLUEMOOR).unwrap();
            assert_eq!(game.battle_odds(&red, BLUEMOOR), Some(odds), "the same question, the same answer");
            assert!((1..100).contains(&odds), "{odds}: the weaker may win, and the stronger lose");
        }
        let events = game.apply(Order::Move { units: red, to: BLUEMOOR }).unwrap();
        (battle(&events), game.map.revision())
    };
    assert_eq!(play(3, false), play(3, true));
    // Over many seeds both sides win some.
    let won = (0..60).filter(|seed| play(*seed, false).0.4).count();
    assert!((5..55).contains(&won), "red won {won} of 60");
    // Nobody to fight: no odds.
    let game = game("garrison soldiers\n", "", |_| {});
    assert_eq!(game.battle_odds(&at(&game, REDHOLD), BLUEMOOR), None);
}

#[test]
fn units_from_several_hexes_attack_together() {
    let mut game = game("garrison soldiers 2\n", "garrison soldiers\n", |_| {});
    let red = at(&game, REDHOLD);
    game.apply(Order::Move { units: vec![red[1]], to: Hex::new(6, 6) }).unwrap();
    game.end_turn();
    game.end_turn();
    let events = game.apply(Order::Move { units: red.clone(), to: BLUEMOOR }).unwrap();
    assert_eq!(battle(&events).0, 2);
    // A stack goes toward a far hex as far as its slowest gets.
    let mut game = self::game("garrison soldiers\ngarrison levy\n", "", |_| {});
    let red = at(&game, REDHOLD);
    game.apply(Order::Toward { units: red.clone(), to: Hex::new(5, 18) }).unwrap();
    let there = game.map.units().get(red[0]).unwrap().pos();
    assert_ne!(there, REDHOLD);
    assert_eq!(at(&game, there), red, "together");
}

#[test]
fn the_simple_agent_attacks_and_wins() {
    let mut game = game("produce soldiers 1\ngarrison soldiers 4\n", "garrison levy\n", |s| {
        s.combat.chance = 5;
        s.set_players("red=simple").unwrap();
    });
    let mut log = Vec::new();
    let outcome = headless::play(&mut game, 30, &mut |_, line| log.push(line.to_string())).unwrap();
    assert_eq!(outcome.winner, Some(Side::Red), "{log:#?}");
    assert!(log.iter().any(|l| l.starts_with("red attacks blue at (5, 9)")), "{log:#?}");
    // It keeps some at home.
    assert!(at(&game, REDHOLD).len() >= Simple::KEEP);
}

// The screen.

/// Click the middle of a widget.
fn push_id(s: &mut Session, name: &str) {
    let r = s.screen.area(widget(s, name));
    click(s, Point::new(r.x + r.w / 2, r.y + r.h / 2));
}

fn session(game: Game) -> Session {
    common::with_game(&assets(), Size::new(1024, 768), 96, game).unwrap()
}

#[test]
fn the_screen_tells_the_odds_and_reports_the_battle() {
    let mut s = session(game("garrison soldiers 3\n", "garrison levy\n", |s| s.set_players("red=person").unwrap()));
    let p = pixel_of(&s, REDHOLD);
    click(&mut s, p);
    assert_eq!(s.screen.game.selection().len(), 3, "the stack");
    // The pointer on the enemy: the odds.
    let p = pixel_of(&s, BLUEMOOR);
    common::hover(&mut s, p);
    assert!(s.status().ends_with("attack: 100% likely to win"), "{}", s.status());
    let p = pixel_of(&s, BLUEMOOR);
    click(&mut s, p);
    // The battle's window: played out, and then how it went.
    let battle = s.battle().expect("the battle, to watch");
    assert_eq!(battle.progress().0, 0);
    push_id(&mut s, hexlords::battle_window::BATTLE_SKIP_ID);
    assert_eq!(
        s.battle().unwrap().outcome(&s.screen.ui),
        [
            "You attacked blue at Bluemoor, 3 against 1, and won.",
            "You lost nothing; they lost 1 Levy.",
            "You took Bluemoor from blue."
        ]
    );
    // Esc closes it.
    press(&mut s, SpecialKey::Esc);
    assert!(s.battle().is_none());
    assert!(s.screen.ui.find(s.screen.ui.root(), turbo_ferret::TbId::new(BATTLE_WINDOW_ID)).is_none());
}

#[test]
fn who_was_attacked_while_they_waited_hears_of_it() {
    // Two people: red attacks blue, and blue hears of it when their turn
    // begins, behind the cover: their log opens, the news in its color.
    let mut s =
        session(game("garrison soldiers 3\n", "garrison levy\n", |s| s.set_players("red=person,blue=person").unwrap()));
    let begin = |s: &mut Session| {
        let r = s.screen.area(widget(s, hexlords::session::BEGIN_TURN_ID));
        click(s, Point::new(r.x + r.w / 2, r.y + r.h / 2));
    };
    begin(&mut s);
    let p = pixel_of(&s, REDHOLD);
    click(&mut s, p);
    let p = pixel_of(&s, BLUEMOOR);
    click(&mut s, p);
    assert!(s.battle().is_some());
    s.end_turn();
    assert!(s.covered() && s.battle().is_none(), "nothing shows over the cover");
    begin(&mut s);
    assert_eq!(
        s.log_new_lines(),
        [
            "Red attacked you at Bluemoor, 3 against 1: you were beaten.",
            "    You lost 1 Levy; they lost nothing.",
            "Red took Bluemoor from you."
        ]
    );
    // The computer's attacks too.
    let mut s = session(game("garrison soldiers 4\n", "garrison levy\n", |s| {
        s.combat.chance = 5;
        s.set_players("red=simple,blue=person").unwrap();
    }));
    for frame in 0..5 {
        s.frame(frame * 16);
    }
    assert_eq!(s.screen.game.player(), Side::Blue);
    let news = s.log_new_lines();
    assert!(news.iter().any(|l| l.starts_with("Red attacked you at Bluemoor")), "{news:?}");
}

#[test]
fn the_panels_say_what_a_commander_does() {
    let game = game("garrison captain\n", "", |_| {});
    let captain = game.map.units().get(at(&game, REDHOLD)[0]).unwrap();
    assert!(hexlords::info::unit_line(&game.map, captain).ends_with(", price 0, commands (+1)"));
    let details = hexlords::info::unit_details(&game.map, captain);
    assert_eq!(details[2], "Commands: +1 to its stack; fights last");
    assert!(details.len() <= hexlords::session::TIP_LINES, "{details:?}");
}

#[test]
fn an_attack_is_a_stack_at_most() {
    // Nine soldiers on two hexes: one too many to attack together; eight
    // and a captain may.
    let mut game = game("garrison soldiers 5\n", "garrison levy\n", |_| {});
    let kind = |game: &Game, name: &str| game.map.units().kind_named(name).unwrap();
    let mut red = at(&game, REDHOLD);
    for _ in 0..4 {
        let id = game.map.new_unit_of(Side::Red, kind(&game, "soldiers")).unwrap();
        game.map.place_unit(id, Hex::new(5, 6)).unwrap();
        red.push(id);
    }
    let refused = game.apply(Order::Move { units: red.clone(), to: BLUEMOOR }).unwrap_err();
    assert_eq!(refused, "at most 8 units and a commander attack together");
    let captain = game.map.new_unit_of(Side::Red, kind(&game, "captain")).unwrap();
    game.map.place_unit(captain, Hex::new(5, 6)).unwrap();
    red.pop();
    red.push(captain);
    assert_eq!(battle(&game.apply(Order::Move { units: red, to: BLUEMOOR }).unwrap()).0, 9);
}

#[test]
fn a_stack_goes_toward_somewhere_only_as_far_as_all_of_it_fits() {
    // Three soldiers toward the east; on the way, as far as they can go, a
    // hex with seven of red's levies has room for one more: they stop short.
    let mut game = game("garrison soldiers 3\n", "", |_| {});
    let levy = game.map.units().kind_named("levy").unwrap();
    let far = Hex::new(5, 11); // six grass steps from Redhold: all 12 points
    for _ in 0..7 {
        let id = game.map.new_unit_of(Side::Red, levy).unwrap();
        game.map.place_unit(id, far).unwrap();
    }
    let red = at(&game, REDHOLD);
    game.apply(Order::Toward { units: red.clone(), to: Hex::new(5, 18) }).unwrap();
    let there = game.map.units().get(red[0]).unwrap().pos();
    assert!(there != far && there != REDHOLD, "{there}");
    assert_eq!(at(&game, there), red, "all three, together");
}

#[test]
fn the_simple_agent_attacks_stacks_in_the_field() {
    // A blue levy out on the grass beside red's soldiers: red goes for it
    // (Bluemoor is too strong for it, Farwater too far).
    let mut game = game("garrison soldiers 4\n", "garrison soldiers 8\n", |s| s.set_players("red=simple").unwrap());
    let levy = game.map.units().kind_named("levy").unwrap();
    let blue = game.map.new_unit_of(Side::Blue, levy).unwrap();
    game.map.place_unit(blue, Hex::new(8, 5)).unwrap();
    let mut agents = hexlords::agents::Agents::for_game(&game).unwrap();
    let report = hexlords::agents::play_turn(&mut game, agents.get_mut(Side::Red).unwrap());
    assert!(
        report.events.iter().any(|e| matches!(e, Event::Battle { at, won: true, .. } if *at == Hex::new(8, 5))),
        "{:?}",
        report.events
    );
}

#[test]
fn games_on_the_continent_end() {
    // The agents against each other on the example map, with the game's
    // defaults: someone wins.
    let (game, _) = Game::load(&common::assets().join("maps/continent")).unwrap();
    let mut setup = Setup::solo(&game.map, None, "simple");
    setup.seats.iter_mut().for_each(|seat| seat.controller = Controller::Computer("simple".to_string()));
    setup.seed = 2;
    let mut game = game.with_setup(setup);
    let outcome = headless::play(&mut game, 200, &mut |_, _| {}).unwrap();
    assert!(outcome.winner.is_some(), "{}", headless::summary(&outcome));
}

#[test]
fn a_good_stack_takes_a_city_and_walls_do_not_make_it_a_fortress() {
    // With the game's chance (5): eight units, six of them soldiers, against
    // one soldier behind walls 1 nearly always win; against two behind walls
    // 3, about as often as not; equal stacks in the field, about even.
    let red = "garrison soldiers 6\ngarrison levy 2\n";
    let odds = |blue: &str| {
        let game = game(red, blue, |s| s.combat.chance = 5);
        game.battle_odds(&at(&game, REDHOLD), BLUEMOOR).unwrap()
    };
    let one = odds("walls 1\ngarrison soldiers\n");
    let two = odds("walls 3\ngarrison soldiers 2\n");
    assert!(one >= 90, "{one}");
    assert!((35..=85).contains(&two), "{two}");
    let field = {
        let mut game = game("garrison soldiers 4\n", "", |s| s.combat.chance = 5);
        let soldiers = game.map.units().kind_named("soldiers").unwrap();
        for _ in 0..4 {
            let id = game.map.new_unit_of(Side::Blue, soldiers).unwrap();
            game.map.place_unit(id, Hex::new(5, 7)).unwrap();
        }
        game.battle_odds(&at(&game, REDHOLD), Hex::new(5, 7)).unwrap()
    };
    assert!((40..=60).contains(&field), "{field}");
}

#[test]
fn a_side_that_lost_its_commander_builds_another() {
    use hexlords::agents::Agent;
    let mut game = game_with("produce soldiers 1\nproduce captain 3\ngarrison soldiers 2\n", "", |s| {
        s.set_players("red=simple").unwrap()
    });
    let redhold = game.map.city_at(REDHOLD).unwrap().id;
    let mut agent = Simple::default();
    assert_eq!(agent.next_order(&game, Side::Red), Order::Build { city: redhold, what: Some(1) });
    // With a captain, it builds soldiers.
    let captain = game.map.units().kind_named("captain").unwrap();
    let id = game.map.new_unit_of(Side::Red, captain).unwrap();
    game.map.place_unit(id, REDHOLD).unwrap();
    let mut agent = Simple::default();
    assert_eq!(agent.next_order(&game, Side::Red), Order::Build { city: redhold, what: Some(0) });
    // A captain done in a city that has one waits for room.
    game.map.set_production(redhold, Some(1));
    game.map.city_mut(redhold).unwrap().progress = 3;
    let city = game.map.city_at(REDHOLD).unwrap();
    assert_eq!(hexlords::info::progress_short(&game.map, city), "Captain: they wait for room");
}

/// A kind at home on some ground fights the stronger there (`bonus`),
/// attacking or defending; a siege engine breaks walls for its stack
/// (`siege`): the defenders' walls count that much less.
#[test]
fn a_kind_at_home_fights_the_stronger_and_siege_engines_break_walls() {
    let mut rows = vec!["g".repeat(20); 20];
    rows[5] = "gggggcggfcgggggggggg".to_string();
    rows[15] = "gggggggggggggggcgggg".to_string();
    let terrain = format!("20\n20\n{}\n", rows.join("\n"));
    let units = "unit rangers\nname Rangers\nstrength 3\nmoves 12\nbonus forest 1, city 2\nenter grass 2, forest 2, city 1\n\
                 unit ram\nname Ram\nstrength 1\nmoves 12\nsiege 2\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, forest 2, city 1\n";
    let cities = "city 5 5\nname Redhold\nside red\ngarrison rangers\ngarrison ram\n\
                  city 5 9\nname Bluemoor\nside blue\nwalls 6\ngarrison levy\n\
                  city 15 15\nname Farwater\nside blue\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let red = at(&game, REDHOLD);
    let (rangers, ram) = (red[0], red[1]);
    let levy = at(&game, BLUEMOOR)[0];
    let strength = |game: &Game, id: UnitId, at: Hex, defending: bool| {
        let unit = game.map.units().get(id).unwrap();
        game.battle_strength(unit, at, defending, &[id])
    };
    // In the forest the rangers are 1 stronger, attacking or defending; on
    // grass, not; in a city, 2.
    let forest = Hex::new(5, 8);
    assert_eq!(game.map.tile(forest).unwrap().kind, TileType::Forest);
    assert_eq!(strength(&game, rangers, forest, false), 4 * 4);
    assert_eq!(strength(&game, rangers, forest, true), 4 * (4 + 1), "and the forest's own 1 for a defender");
    assert_eq!(strength(&game, rangers, Hex::new(5, 7), false), 4 * 3);
    assert_eq!(strength(&game, rangers, BLUEMOOR, false), 4 * 5);
    // Behind walls 6 the levy is 3 stronger; against a stack with the ram, 1.
    assert_eq!(strength(&game, levy, BLUEMOOR, true), 4 * (1 + 3));
    assert_eq!(game.siege_of(&[rangers, ram]), 2);
    let parts = |siege| {
        let unit = game.map.units().get(levy).unwrap();
        game.strength_parts(unit, BLUEMOOR, true, &[levy], siege).quarters()
    };
    assert_eq!((parts(0), parts(2), parts(5)), (4 * 4, 4 * 2, 4), "never below nothing");
    // The battle counts it: with no chance, the rangers alone (5 in a city)
    // beat the levy (4); and the fight keeps what the siege broke.
    let mut s = Setup::solo(&game.map, Some(Side::Red), "passive");
    s.combat.chance = 0;
    game = game.with_setup(s);
    let forecast = game.battle_forecast(&[rangers, ram], BLUEMOOR).unwrap();
    assert_eq!(forecast.odds, 100);
    game.apply(Order::Move { units: vec![rangers, ram], to: BLUEMOOR }).unwrap();
    let record = game.history().iter().rposition(|r| matches!(r.event, Event::Battle { .. })).unwrap();
    let fight = game.fight(record).unwrap();
    assert_eq!((fight.siege, fight.defenders[0].walls, fight.attackers[0].home), (2, 4, 2));
    // In the open there are no walls to break: the battle says none broke.
    let levy = game.map.units().kind_named("levy").unwrap();
    let foe = game.map.new_unit_of(Side::Blue, levy).unwrap();
    game.map.place_unit(foe, Hex::new(6, 9)).unwrap();
    let mut red_unit = |kind: &str| {
        let k = game.map.units().kind_named(kind).unwrap();
        let id = game.map.new_unit_of(Side::Red, k).unwrap();
        game.map.place_unit(id, Hex::new(6, 8)).unwrap();
        id
    };
    let stack = vec![red_unit("rangers"), red_unit("ram")];
    game.apply(Order::Move { units: stack, to: Hex::new(6, 9) }).unwrap();
    let record = game.history().iter().rposition(|r| matches!(r.event, Event::Battle { .. })).unwrap();
    assert_eq!(game.fight(record).unwrap().siege, 0);
}
