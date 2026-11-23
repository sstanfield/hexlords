//! The number that says how a game stands (`Game::digest`, a saved game's
//! `check`) covers all of what the rules keep (the owner's word,
//! 2026-11-23: "you can change the format/checksums at this point"): it
//! left out the rally points, a city's work and where it sends its units,
//! what a side has seen and remembers under fog of war, and where a
//! standing order leads; a game that came out otherwise in those was taken
//! for the same.

use hexlords::game::Game;
use hexlords::orders::{Order, RallyPoint};
use hexlords::setup::{Fog, Mode, Setup};
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{Hex, QueueEnd, QueueItem, Sends, Side, UnitId};

/// 30 x 30 grass, red's city top left and blue's top right; soldiers (12
/// points, 2 a hex).
fn plain(mode: Mode, fog: Fog) -> Game {
    let mut rows = vec!["g".repeat(30); 30];
    rows[1] = format!("gc{}cg", "g".repeat(26));
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 5\nmoves 12\nenter grass 2, city 1\n";
    let cities = "city 1 1\nname Redhold\nside red\nproduce soldiers 2\ncity 1 28\nname Bluemoor\nside blue\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut setup = Setup::solo(&game.map, None, "passive");
    setup.set_players("red=passive,blue=passive").unwrap();
    (setup.mode, setup.fog, setup.combat.chance) = (mode, fog, 0);
    game.with_setup(setup)
}

fn put(game: &mut Game, side: Side, hex: Hex) -> UnitId {
    let kind = game.map.units().kind_named("soldiers").unwrap();
    let id = game.map.new_unit_of(side, kind).unwrap();
    game.map.place_unit(id, hex).unwrap();
    id
}

#[test]
fn the_number_tells_a_rally_point_and_a_citys_work() {
    let plain_game = plain(Mode::Turns, Fog::Off);
    let same = plain(Mode::Turns, Fog::Off);
    assert_eq!(plain_game.digest(), same.digest(), "two games set up alike");
    let city = plain_game.map.city_at(Hex::new(1, 1)).unwrap().id;
    let point = RallyPoint { at: Hex::new(5, 5), defend: false };
    let orders = [
        ("a rally point", Order::RallyPoint { n: 1, point: Some(point) }),
        ("a rally point that defends", Order::RallyPoint { n: 1, point: Some(RallyPoint { defend: true, ..point }) }),
        ("another rally point", Order::RallyPoint { n: 2, point: Some(point) }),
        ("where a city sends its units", Order::Rally { city, to: Sends::Defend }),
        ("a route for a city's units", Order::Rally { city, to: Sends::Route(vec![Hex::new(5, 5)]) }),
        ("another route", Order::Rally { city, to: Sends::Route(vec![Hex::new(5, 6)]) }),
        (
            "a city's queue",
            Order::Queue {
                city,
                items: vec![QueueItem { what: 0, count: 2, sends: Sends::Stay }],
                end: QueueEnd::Repeat { from: 0, times: None },
            },
        ),
        (
            "a queue of another length",
            Order::Queue {
                city,
                items: vec![QueueItem { what: 0, count: 3, sends: Sends::Stay }],
                end: QueueEnd::Stop,
            },
        ),
    ];
    let mut seen = vec![("nothing ordered", plain_game.digest())];
    for (what, order) in orders {
        let mut game = plain(Mode::Turns, Fog::Off);
        game.apply(order).unwrap_or_else(|why| panic!("{what}: {why}"));
        let digest = game.digest();
        for (other, was) in &seen {
            assert_ne!(digest, *was, "{what} is told from {other}");
        }
        seen.push((what, digest));
    }
    // A rally point set and taken away again: as it was.
    let mut game = plain(Mode::Turns, Fog::Off);
    game.apply(Order::RallyPoint { n: 1, point: Some(point) }).unwrap();
    game.apply(Order::RallyPoint { n: 1, point: None }).unwrap();
    assert_eq!(game.digest(), plain_game.digest());
}

/// Under total fog a unit that went out and came back has shown its side
/// land: a round later it stands as one that never went, but its side
/// knows more.
#[test]
fn the_number_tells_what_a_side_has_seen() {
    let (home, out) = (Hex::new(15, 10), Hex::new(15, 12));
    let build = || {
        let mut game = plain(Mode::Turns, Fog::Total);
        let scout = put(&mut game, Side::Red, home);
        (game, scout)
    };
    let ((mut went, scout), (mut stayed, _)) = (build(), build());
    went.apply(Order::Move { units: vec![scout], to: out }).unwrap();
    went.apply(Order::Move { units: vec![scout], to: home }).unwrap();
    for game in [&mut went, &mut stayed] {
        game.apply(Order::EndTurn).unwrap();
        game.apply(Order::EndTurn).unwrap();
        assert_eq!((game.turn(), game.player()), (2, Side::Red));
    }
    let unit = |game: &Game| {
        let unit = game.map.units().get(scout).unwrap();
        (unit.pos(), unit.movement, unit.moved, unit.done)
    };
    assert_eq!(unit(&went), unit(&stayed), "the unit stands as it did");
    assert!(went.sight(Side::Red).known_count() > stayed.sight(Side::Red).known_count());
    assert_ne!(went.digest(), stayed.digest());
}

/// A stack another side saw and no longer sees is remembered where it
/// was: that too is how the game stands.
#[test]
fn the_number_tells_what_a_side_remembers() {
    let (home, near, far) = (Hex::new(15, 10), Hex::new(15, 12), Hex::new(15, 19));
    let build = |seen: bool| {
        let mut game = plain(Mode::Turns, Fog::On);
        put(&mut game, Side::Red, home);
        // Blue's stack: where red sees it, or beyond.
        let blue = put(&mut game, Side::Blue, if seen { near } else { far });
        game.apply(Order::EndTurn).unwrap();
        // It goes (or is) out of red's sight, to the same hex in both games.
        if seen {
            game.apply(Order::Move { units: vec![blue], to: Hex::new(15, 15) }).unwrap();
        }
        game.apply(Order::EndTurn).unwrap();
        game.apply(Order::EndTurn).unwrap();
        if seen {
            game.apply(Order::Move { units: vec![blue], to: far }).unwrap();
        }
        game.apply(Order::EndTurn).unwrap();
        game.apply(Order::EndTurn).unwrap();
        game.apply(Order::EndTurn).unwrap();
        (game, blue)
    };
    let ((seen, blue), (unseen, _)) = (build(true), build(false));
    let unit = |game: &Game| {
        let unit = game.map.units().get(blue).unwrap();
        (unit.pos(), unit.movement, game.turn(), game.player())
    };
    assert_eq!(unit(&seen), unit(&unseen), "blue's stack stands as it would");
    let ghosts = |game: &Game| game.sight(Side::Red).ghosts().len();
    assert!(ghosts(&seen) > ghosts(&unseen), "red remembers it only where it saw it");
    assert_ne!(seen.digest(), unseen.digest());
}

/// Two marches of the same length to different hexes are two games (in
/// rounds nothing has moved yet when the order is given).
#[test]
fn the_number_tells_where_a_standing_order_leads() {
    let build = |to: Hex| {
        let mut game = plain(Mode::Rounds, Fog::Off);
        let unit = put(&mut game, Side::Red, Hex::new(15, 10));
        game.apply(Order::GoTo { units: vec![unit], to, via: Vec::new() }).unwrap();
        game
    };
    assert_ne!(build(Hex::new(15, 22)).digest(), build(Hex::new(16, 22)).digest());
    assert_eq!(build(Hex::new(15, 22)).digest(), build(Hex::new(15, 22)).digest());
}
