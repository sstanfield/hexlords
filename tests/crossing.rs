//! Two stacks of one side whose ways cross head-on, in rounds (the owner's
//! word, 2026-11-23: "we really don't want overloaded hexes"): no hex ever
//! holds more than a stack, whatever is ordered (the map itself refuses:
//! `Map::move_unit`), and two stacks that are sent to each other's hexes,
//! or meet on their ways with a step left, pass each other: they change
//! places at once (`Map::change_places`). Before, each waited for the
//! other to leave, and both stood still for the round.

use hexlords::game::Game;
use hexlords::orders::{Event, Order};
use hexlords::setup::{Fog, Mode, Setup};
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{Hex, MAX_STACK, Side, UnitId};

fn start(terrain: &str, units: &str, cities: &str, mode: Mode, fog: Fog) -> Game {
    let (map, warnings) = scenario::parse(terrain, Some(units), Some(cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut setup = Setup::solo(&game.map, None, "passive");
    setup.set_players("red=passive,blue=passive").unwrap();
    (setup.mode, setup.fog, setup.combat.chance) = (mode, fog, 0);
    game.with_setup(setup)
}

fn put(game: &mut Game, side: Side, kind: &str, hex: Hex) -> UnitId {
    let kind = game.map.units().kind_named(kind).unwrap();
    let id = game.map.new_unit_of(side, kind).unwrap();
    game.map.place_unit(id, hex).unwrap();
    id
}

fn pos(game: &Game, id: UnitId) -> Option<Hex> {
    game.map.units().get(id).map(|u| u.pos())
}

/// No hex holds more than a stack (and a commander), nor two sides.
fn sound(game: &Game, what: &str) {
    for unit in game.map.units().iter().filter(|u| game.map.contains(u.pos())) {
        let here = game.map.units_at(unit.pos());
        let leaders = here.iter().filter(|id| game.map.is_commander(**id)).count();
        assert!(leaders <= 1 && here.len() - leaders <= MAX_STACK, "{what}: {} on {:?}", here.len(), unit.pos());
    }
}

/// Both sides have done: the round plays out, a beat at a time.
fn round(game: &mut Game) -> Vec<Event> {
    let mut events = game.apply(Order::EndTurn).unwrap();
    events.extend(game.apply(Order::EndTurn).unwrap());
    while game.playing_out() {
        events.extend(game.play_out());
        sound(game, "in a beat");
    }
    events
}

fn plain() -> Game {
    let mut rows = vec!["g".repeat(30); 30];
    rows[1] = format!("gc{}cg", "g".repeat(26));
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 5\nmoves 12\nenter grass 2, city 1\n\
                 unit riders\nname Riders\nstrength 5\nmoves 20\nenter grass 2, city 1\n\
                 unit captain\nname Captain\nstrength 5\nmoves 12\ncommander 1\nenter grass 2, city 1\n";
    let cities = "city 1 1\nname Redhold\nside red\ncity 1 28\nname Bluemoor\nside blue\n";
    start(&terrain, units, cities, Mode::Rounds, Fog::Off)
}

fn movement(game: &Game, id: UnitId) -> u32 {
    game.map.units().get(id).unwrap().movement
}

/// Stacks on hexes side by side, too many for one hex, each sent to the
/// hex of the other: they change places, each paying its step, and every
/// unit is said to have moved.
#[test]
fn two_stacks_sent_to_each_others_hexes_change_places() {
    let (x, y) = (Hex::new(10, 10), Hex::new(9, 10));
    for (na, nb, kind_b, led) in [(5, 5, "soldiers", false), (8, 8, "soldiers", true), (8, 1, "riders", false)] {
        for toward in [false, true] {
            let what = format!("{na} soldiers and {nb} {kind_b}, led {led}, toward {toward}");
            let mut game = plain();
            let mut a: Vec<UnitId> = (0..na).map(|_| put(&mut game, Side::Red, "soldiers", x)).collect();
            let mut b: Vec<UnitId> = (0..nb).map(|_| put(&mut game, Side::Red, kind_b, y)).collect();
            if led {
                // (A hex holds one commander: they pass each other too.)
                a.push(put(&mut game, Side::Red, "captain", x));
                b.push(put(&mut game, Side::Red, "captain", y));
            }
            if toward {
                game.apply(Order::Toward { units: a.clone(), to: y }).expect(&what);
                game.apply(Order::Toward { units: b.clone(), to: x }).expect(&what);
            } else {
                // A move where a stack stays is refused; where it is leaving, not.
                let why = game.apply(Order::Move { units: a.clone(), to: y }).unwrap_err();
                assert!(why.contains("no room"), "{what}: {why}");
                game.apply(Order::Move { units: a.clone(), to: Hex::new(10, 11) }).expect(&what);
                game.apply(Order::Move { units: b.clone(), to: x }).expect(&what);
                game.apply(Order::Move { units: a.clone(), to: y }).expect(&what);
            }
            let events = round(&mut game);
            assert!(a.iter().all(|id| pos(&game, *id) == Some(y)), "{what}: the first stack is where the second was");
            assert!(b.iter().all(|id| pos(&game, *id) == Some(x)), "{what}: the second where the first was");
            assert_eq!((game.map.units_at(x).len(), game.map.units_at(y).len()), (b.len(), a.len()), "{what}");
            let moved = |id: &UnitId, from: Hex, to: Hex| {
                let is = |e: &&Event| matches!(e, Event::Moved { unit, from: f, to: t, cost: 2, .. } if unit == id && (*f, *t) == (from, to));
                events.iter().filter(is).count() == 1
            };
            assert!(a.iter().all(|id| moved(id, x, y)) && b.iter().all(|id| moved(id, y, x)), "{what}: {events:?}");
            // (The round is over and the next has begun: paid, then made up again.)
            assert_eq!(game.turn(), 2, "{what}");
        }
    }
}

/// What the step costs is paid (seen before the next round makes it up):
/// the round is played a beat at a time, and after the first both are
/// there with two points fewer.
#[test]
fn stacks_that_change_places_pay_their_step() {
    let (x, y) = (Hex::new(10, 10), Hex::new(9, 10));
    let mut game = plain();
    game.watch(true);
    let a: Vec<UnitId> = (0..5).map(|_| put(&mut game, Side::Red, "soldiers", x)).collect();
    let b: Vec<UnitId> = (0..5).map(|_| put(&mut game, Side::Red, "riders", y)).collect();
    // (Somebody with far to go: the round goes on after their step.)
    let far = put(&mut game, Side::Red, "soldiers", Hex::new(20, 20));
    game.apply(Order::Toward { units: vec![far], to: Hex::new(20, 26) }).unwrap();
    game.apply(Order::Toward { units: a.clone(), to: y }).unwrap();
    game.apply(Order::Toward { units: b.clone(), to: x }).unwrap();
    game.apply(Order::EndTurn).unwrap();
    game.apply(Order::EndTurn).unwrap();
    assert!(game.playing_out());
    // (The riders' step is up first: both stacks go then.)
    while game.playing_out() && pos(&game, a[0]) == Some(x) {
        game.play_out();
        sound(&game, "in a beat");
    }
    assert!(game.playing_out(), "seen before the round is over");
    assert_eq!((pos(&game, a[0]), pos(&game, b[0])), (Some(y), Some(x)));
    assert_eq!((movement(&game, a[0]), movement(&game, b[0])), (10, 18));
}

/// Two stacks on their ways toward each other, far apart, that come to
/// stand side by side with one step's movement left each (to go through
/// the other costs two): each takes that step. They stood still before.
#[test]
fn stacks_that_meet_with_a_step_left_pass_each_other() {
    let (south, north) = (Hex::new(20, 10), Hex::new(9, 10));
    let mut game = plain();
    let a: Vec<UnitId> = (0..5).map(|_| put(&mut game, Side::Red, "soldiers", south)).collect();
    let b: Vec<UnitId> = (0..5).map(|_| put(&mut game, Side::Red, "soldiers", north)).collect();
    game.apply(Order::Toward { units: a.clone(), to: north }).unwrap();
    game.apply(Order::Toward { units: b.clone(), to: south }).unwrap();
    round(&mut game);
    // Twelve points at two a hex: six hexes each, the sixth past the other.
    assert_eq!((pos(&game, a[0]), pos(&game, b[0])), (Some(Hex::new(14, 10)), Some(Hex::new(15, 10))));
}

/// Who stays on a hex keeps its place: the stacks pass each other only if,
/// with those that stay, each hex still holds no more than a stack. If
/// not, nobody goes; and no hex is ever over.
#[test]
fn who_stays_on_a_hex_keeps_the_room() {
    let (x, y) = (Hex::new(10, 10), Hex::new(9, 10));
    for (stay, passes) in [(3, true), (4, false)] {
        let mut game = plain();
        let a: Vec<UnitId> = (0..4).map(|_| put(&mut game, Side::Red, "soldiers", x)).collect();
        let stays: Vec<UnitId> = (0..stay).map(|_| put(&mut game, Side::Red, "soldiers", x)).collect();
        let b: Vec<UnitId> = (0..5).map(|_| put(&mut game, Side::Red, "soldiers", y)).collect();
        game.apply(Order::Defend { units: stays.clone(), on: true }).unwrap();
        game.apply(Order::Toward { units: a.clone(), to: y }).unwrap();
        game.apply(Order::Toward { units: b.clone(), to: x }).unwrap();
        round(&mut game);
        let (to_a, to_b) = if passes { (y, x) } else { (x, y) };
        assert!(a.iter().all(|id| pos(&game, *id) == Some(to_a)), "{stay} stay: the four");
        assert!(b.iter().all(|id| pos(&game, *id) == Some(to_b)), "{stay} stay: the five");
        assert!(stays.iter().all(|id| pos(&game, *id) == Some(x)), "{stay} stay: those that stay");
    }
}

/// Three full stacks in a row, the outer two sent to each other's hexes
/// through the middle one, which stays: they go through it and change
/// places, and no hex is ever over (checked at every beat, `round`).
#[test]
fn full_stacks_go_through_one_another_and_no_hex_is_ever_over() {
    let (south, mid, north) = (Hex::new(11, 10), Hex::new(10, 10), Hex::new(9, 10));
    let mut game = plain();
    let a: Vec<UnitId> = (0..8).map(|_| put(&mut game, Side::Red, "soldiers", south)).collect();
    let m: Vec<UnitId> = (0..8).map(|_| put(&mut game, Side::Red, "soldiers", mid)).collect();
    let b: Vec<UnitId> = (0..8).map(|_| put(&mut game, Side::Red, "riders", north)).collect();
    game.apply(Order::Toward { units: a.clone(), to: north }).unwrap();
    game.apply(Order::Toward { units: b.clone(), to: south }).unwrap();
    round(&mut game);
    sound(&game, "after the round");
    let at = |ids: &[UnitId]| pos(&game, ids[0]).map(|h| (h.row, h.col));
    assert_eq!(at(&m), Some((10, 10)), "the middle stays");
    // (Before, each waited for the other to leave, and both stood still.)
    assert_eq!((at(&a), at(&b)), (Some((9, 10)), Some((11, 10))));
}

/// Stacks that pass each other go on to where they were sent: the plan and
/// its way are theirs still after the step, the faster stack's too, whose
/// step was not up yet when the two changed places.
#[test]
fn stacks_that_pass_each_other_go_on_to_where_they_were_sent() {
    let (south, north) = (Hex::new(12, 10), Hex::new(11, 10));
    for kind in ["soldiers", "riders"] {
        let mut game = plain();
        let a: Vec<UnitId> = (0..5).map(|_| put(&mut game, Side::Red, "soldiers", south)).collect();
        let b: Vec<UnitId> = (0..5).map(|_| put(&mut game, Side::Red, kind, north)).collect();
        game.apply(Order::Move { units: a.clone(), to: Hex::new(8, 10) }).unwrap();
        game.apply(Order::Toward { units: b.clone(), to: Hex::new(15, 10) }).unwrap();
        let events = round(&mut game);
        assert!(a.iter().all(|id| pos(&game, *id) == Some(Hex::new(8, 10))), "{kind}: the soldiers");
        assert!(b.iter().all(|id| pos(&game, *id) == Some(Hex::new(15, 10))), "{kind}: the others");
        // Hex by hex: nobody went two at a step, through the other.
        let steps = events.iter().filter(|e| matches!(e, Event::Moved { unit, .. } if *unit == a[0])).count();
        assert_eq!(steps, 4, "{kind}: the soldiers' steps");
    }
}
