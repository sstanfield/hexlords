//! Battles in the game: an attack is a move into another side's units; who
//! wins, who dies, the city taken; what the ground, the walls and a commander
//! add; stacks that move together; the same battles from the same seed; the
//! computer attacking; and what the screen says of it (the odds, the report,
//! the news of a battle fought while one waited).

mod common;

use common::{assets, click, pixel_of, press, widget};
use hexgame::agents::Simple;
use hexgame::game::Game;
use hexgame::headless;
use hexgame::orders::{Event, Order};
use hexgame::session::{NOTICE_ID, Session};
use hexgame::setup::{Controller, Setup};
use hexgame::state::combat::System;
use hexgame::state::scenario::{self, Scenario};
use hexgame::state::{Hex, MAX_STACK, Side, TileType, UnitId};
use turbo_ferret::{Point, Size, SpecialKey};

const REDHOLD: Hex = Hex::new(5, 5);
const BLUEMOOR: Hex = Hex::new(5, 9);

/// Red's city with `red` in it, blue's with `blue`, and another of blue's far
/// off with nobody in it (so taking Bluemoor does not end the game). Every
/// side is passive, the battles have no chance, unless `setup` says otherwise.
fn game(red: &str, blue: &str, setup: impl FnOnce(&mut Setup)) -> Game {
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
    // Each paid for the way: three steps of grass and the city.
    assert!(red.iter().all(|id| game.map.units().get(*id).unwrap().movement == 5));
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
    let mut game = game("garrison soldiers\n", "walls 2\ngarrison levy\n", |_| {});
    let levy = at(&game, BLUEMOOR)[0];
    let soldiers = at(&game, REDHOLD)[0];
    let strength = |game: &Game, id: UnitId, defending: bool| {
        let unit = game.map.units().get(id).unwrap();
        game.battle_strength(unit, unit.pos(), defending, &[id])
    };
    // In its city behind walls 2: 1 and 2. Who attacks has no walls.
    assert_eq!(strength(&game, levy, true), 3);
    assert_eq!(strength(&game, levy, false), 1);
    // Out on the ground: hills and forests 1, mountains 2, grass nothing.
    for (kind, bonus) in [(TileType::Grass, 0), (TileType::Hill, 1), (TileType::Forest, 1), (TileType::Mountain, 2)] {
        game.map.tile_mut(Hex::new(5, 7)).unwrap().kind = kind;
        let unit = game.map.units().get(soldiers).unwrap();
        assert_eq!(game.battle_strength(unit, Hex::new(5, 7), true, &[soldiers]), 3 + bonus, "{kind:?}");
    }
    // With no chance, a tie is the defender's: 3 against the levy's 3 behind
    // its walls loses.
    let events = game.apply(Order::Move { units: vec![soldiers], to: BLUEMOOR }).unwrap();
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
    assert_eq!((strength(levy), strength(soldiers), strength(captain)), (2, 4, 2));
    assert!(game.map.is_commander(captain) && !game.map.is_commander(levy));
    // In lines the captain stands last: red's 3 beats the levy (2), then
    // loses to the soldiers (4), and the captain never fights.
    let red = at(&game, REDHOLD);
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

fn session(game: Game) -> Session {
    Session::with_game(&assets(), Size::new(1024, 768), 96, game).unwrap()
}

fn notice(s: &Session) -> Vec<String> {
    s.notice()
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
    assert_eq!(
        notice(&s),
        ["Red attacked blue at Bluemoor, 3 against 1.", "Red lost 0, blue 1.", "Red won.", "Red took Bluemoor."]
    );
    // Esc closes it.
    press(&mut s, SpecialKey::Esc);
    assert!(notice(&s).is_empty());
    assert!(s.screen.ui.find(s.screen.ui.root(), turbo_ferret::TbId::new(NOTICE_ID)).is_none());
}

#[test]
fn who_was_attacked_while_they_waited_hears_of_it() {
    // Two people: red attacks blue, and blue hears of it when their turn
    // begins, behind the cover.
    let mut s =
        session(game("garrison soldiers 3\n", "garrison levy\n", |s| s.set_players("red=person,blue=person").unwrap()));
    let begin = |s: &mut Session| {
        let r = s.screen.area(widget(s, hexgame::session::BEGIN_TURN_ID));
        click(s, Point::new(r.x + r.w / 2, r.y + r.h / 2));
    };
    begin(&mut s);
    let p = pixel_of(&s, REDHOLD);
    click(&mut s, p);
    let p = pixel_of(&s, BLUEMOOR);
    click(&mut s, p);
    assert!(!notice(&s).is_empty());
    s.end_turn();
    assert!(s.covered() && notice(&s).is_empty(), "nothing shows over the cover");
    begin(&mut s);
    assert_eq!(
        notice(&s),
        ["Red attacked you at Bluemoor: you lost it, losing 1; they lost 0.", "Red took Bluemoor from you."]
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
    let news = notice(&s);
    assert!(news.iter().any(|l| l.starts_with("Red attacked you at Bluemoor")), "{news:?}");
}

#[test]
fn the_panels_say_what_a_commander_does() {
    let game = game("garrison captain\n", "", |_| {});
    let captain = game.map.units().get(at(&game, REDHOLD)[0]).unwrap();
    assert!(hexgame::info::unit_line(&game.map, captain).ends_with(", price 0, commands (+1)"));
    let details = hexgame::info::unit_details(&game.map, captain);
    assert_eq!(details[2], "Commands: its stack fights 1 stronger; it fights last");
    assert!(details.len() <= hexgame::session::TIP_LINES, "{details:?}");
}
