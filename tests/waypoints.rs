//! Waypoints in rounds (the owner's word, 2026-11-22: "In rounds games we
//! need to be able to use goto more than once. This will let setting
//! waypoints to force a stack on a path we want and not waste mp/turns ...
//! setting a goto point could leave the stack active with MP to spend from
//! that point ... always start fresh when a stack is selected"): a stack
//! with a plan is sent on from where it ends (`Order::Onward`), and the
//! screen keeps it selected to be.

mod common;

use hexlords::game::{Game, SelectOutcome};
use hexlords::options_window::Options;
use hexlords::orders::{Event, Order};
use hexlords::session::Session;
use hexlords::setup::{Mode, Setup};
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{Hex, Side, UnitId};
use turbo_ferret::Size;

const FROM: Hex = Hex::new(10, 10);
/// Three hexes north of the stack, and three hexes east of that: six
/// steps, all the soldiers have; the straight way to `END` does not pass
/// `VIA`.
const VIA: Hex = Hex::new(7, 10);
const END: Hex = Hex::new(7, 13);

/// 30 x 30 grass, red's two soldiers (12 points, 2 a hex: six hexes) in the
/// middle and a blue levy four hexes south of them; `mode`, red a person
/// if `person`.
fn game(mode: Mode, person: bool) -> (Game, Vec<UnitId>) {
    let mut rows = vec!["g".repeat(30); 30];
    rows[1] = format!("gc{}cg", "g".repeat(26));
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 5\nmoves 12\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 12\nenter grass 2, city 1\n";
    let cities = "city 1 1\nname Redhold\nside red\ncity 1 28\nname Bluemoor\nside blue\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut ids = Vec::new();
    for (side, kind, hex) in
        [(Side::Red, "soldiers", FROM), (Side::Red, "soldiers", FROM), (Side::Blue, "levy", Hex::new(14, 10))]
    {
        let kind = game.map.units().kind_named(kind).unwrap();
        let id = game.map.new_unit_of(side, kind).unwrap();
        game.map.place_unit(id, hex).unwrap();
        if side == Side::Red {
            ids.push(id);
        }
    }
    let mut setup = Setup::solo(&game.map, person.then_some(Side::Red), "passive");
    (setup.mode, setup.combat.chance) = (mode, 0);
    (game.with_setup(setup), ids)
}

fn unit(game: &Game, id: UnitId) -> &hexlords::state::Unit {
    game.map.units().get(id).unwrap()
}

/// Both sides have done: the round plays out. The hexes red's first unit
/// went over.
fn round(game: &mut Game, id: UnitId) -> Vec<Hex> {
    let mut events = game.apply(Order::EndTurn).unwrap();
    events.extend(game.apply(Order::EndTurn).unwrap());
    while game.playing_out() {
        events.extend(game.play_out());
    }
    let mut went = Vec::new();
    for event in events {
        if let Event::Moved { unit, way, .. } = event {
            if unit == id {
                went.extend(way.into_iter().skip(1));
            }
        }
    }
    went
}

#[test]
fn a_plan_is_sent_on_from_where_it_ends_and_the_stack_goes_that_way() {
    let (mut game, ids) = game(Mode::Rounds, false);
    game.apply(Order::Move { units: ids.clone(), to: VIA }).unwrap();
    assert_eq!(game.plan_end(&ids), Some(VIA));
    game.apply(Order::Onward { units: ids.clone(), to: END }).unwrap();
    assert_eq!(unit(&game, ids[0]).route, [VIA, END], "by way of the first, to the second");
    assert_eq!(game.plan_end(&ids), Some(END));
    assert!(ids.iter().all(|id| unit(&game, *id).pos() == FROM), "nothing moves yet");
    // The round: both go by the waypoint, and end where the plan does,
    // with nothing left (six hexes; the straight way is four).
    let went = round(&mut game, ids[0]);
    assert_eq!(went.len(), 6, "{went:?}");
    assert_eq!(went[2], VIA, "{went:?}");
    assert!(ids.iter().all(|id| unit(&game, *id).pos() == END));
    assert!(ids.iter().all(|id| unit(&game, *id).route.is_empty() && !unit(&game, *id).planned));
}

#[test]
fn only_as_far_as_the_way_so_far_leaves_them_and_only_after_a_plan() {
    let (mut game, ids) = game(Mode::Rounds, false);
    // No plan: nothing to go on from.
    let why = game.apply(Order::Onward { units: ids.clone(), to: END }).unwrap_err();
    assert!(why.contains("no plan"), "{why}");
    game.apply(Order::Move { units: ids.clone(), to: VIA }).unwrap();
    // Three hexes used, three left: four from the waypoint is too far,
    // though it is within six of where they stand.
    let far = Hex::new(7, 14);
    assert!(FROM.distance(far) <= 6 && VIA.distance(far) == 4);
    let why = game.apply(Order::Onward { units: ids.clone(), to: far }).unwrap_err();
    assert!(why.contains("cannot get there"), "{why}");
    assert_eq!(unit(&game, ids[0]).route, [VIA], "the plan is as it was");
    // One of them alone has no plan of its own with another way: it may
    // go on by itself (the other keeps the plan).
    game.apply(Order::Onward { units: vec![ids[0]], to: END }).unwrap();
    assert_eq!((unit(&game, ids[0]).route.clone(), unit(&game, ids[1]).route.clone()), (vec![VIA, END], vec![VIA]));
    // The two no longer have one plan together.
    assert_eq!(game.plan_end(&ids), None);
    assert!(game.apply(Order::Onward { units: ids.clone(), to: Hex::new(6, 10) }).is_err());
    // Taken back: the last leg only.
    game.apply(Order::Undo).unwrap();
    assert_eq!(unit(&game, ids[0]).route, [VIA]);
    assert_eq!(game.plan_end(&ids), Some(VIA));
    // A plan that ends in a battle has nothing after it.
    let (mut game, ids) = self::game(Mode::Rounds, false);
    let blue = Hex::new(14, 10);
    game.apply(Order::Move { units: ids.clone(), to: blue }).unwrap();
    let why = game.apply(Order::Onward { units: ids.clone(), to: Hex::new(15, 10) }).unwrap_err();
    assert!(why.contains("battle"), "{why}");
}

#[test]
fn in_turns_onward_is_a_move() {
    let (mut game, ids) = game(Mode::Turns, false);
    game.apply(Order::Move { units: ids.clone(), to: VIA }).unwrap();
    game.apply(Order::Onward { units: ids.clone(), to: END }).unwrap();
    assert!(ids.iter().all(|id| unit(&game, *id).pos() == END && unit(&game, *id).movement == 0));
}

#[test]
fn a_stack_selected_to_go_on_is_offered_what_its_plan_leaves_it() {
    let (mut game, ids) = game(Mode::Rounds, false);
    assert!(!game.select_onward(&ids), "no plan yet");
    assert!(matches!(game.select_hex(FROM), SelectOutcome::Selected(_)));
    assert!(game.can_move_to(Hex::new(4, 10)) && game.onward_from().is_none());
    assert!(matches!(game.select_hex(VIA), SelectOutcome::Moved { .. }));
    assert!(game.selection().is_empty());
    // Selected to go on: three hexes from the waypoint, at 2 a hex.
    assert!(game.select_onward(&ids));
    assert_eq!((game.selection(), game.onward_from()), (ids.as_slice(), Some(VIA)));
    let range = game.range().unwrap();
    assert_eq!((range.cost(END), range.cost(Hex::new(7, 14)), range.cost(Hex::new(6, 10))), (Some(6), None, Some(2)));
    assert_eq!(range.cost(FROM), Some(6), "back the way they came is all they have");
    // The end itself: nothing. A hex in reach: a waypoint more.
    assert!(matches!(game.select_hex(VIA), SelectOutcome::Selected(_)));
    assert_eq!(unit(&game, ids[0]).route, [VIA]);
    assert!(matches!(game.select_hex(Hex::new(7, 11)), SelectOutcome::Moved { .. }));
    assert_eq!(unit(&game, ids[0]).route, [VIA, Hex::new(7, 11)]);
    // Undo: the leg is gone, and they are selected to go on from the
    // first again.
    game.apply(Order::Undo).unwrap();
    assert_eq!((unit(&game, ids[0]).route.clone(), game.onward_from()), (vec![VIA], Some(VIA)));
    // Their own hex: afresh, from where they stand; the next hex is a
    // new plan.
    assert!(matches!(game.select_hex(FROM), SelectOutcome::Selected(_)));
    assert!(game.onward_from().is_none() && game.can_move_to(Hex::new(4, 10)));
    assert!(matches!(game.select_hex(Hex::new(12, 10)), SelectOutcome::Moved { .. }));
    assert_eq!(unit(&game, ids[0]).route, [Hex::new(12, 10)]);
    // With nothing left where the plan ends, they are not kept.
    game.select_hex(FROM);
    game.select_hex(Hex::new(4, 10));
    assert!(!game.select_onward(&ids) && game.selection().is_empty());
    // Beyond what is left: sent there over the rounds, by way of the
    // plan's stops.
    game.select_hex(FROM);
    game.select_hex(VIA);
    assert!(game.select_onward(&ids));
    let far = Hex::new(7, 25);
    assert!(matches!(game.select_hex(far), SelectOutcome::Sent(_)));
    assert_eq!(unit(&game, ids[0]).route, [VIA, far]);
    assert!(!unit(&game, ids[0]).planned);
    let went = round(&mut game, ids[0]);
    assert_eq!(went[2], VIA, "{went:?}");
}

#[test]
fn on_the_screen_a_planned_stack_stays_selected_to_be_sent_on() {
    let (game, ids) = game(Mode::Rounds, true);
    let mut s = Session::with_options(&common::assets(), Size::new(1024, 768), 96, game, Options::manual()).unwrap();
    s.screen.view.center_on(FROM);
    let click = |s: &mut Session, hex: Hex| {
        let p = common::pixel_of(s, hex);
        common::click(s, p);
        s.frame(0);
    };
    click(&mut s, FROM);
    click(&mut s, VIA);
    // Planned, and still selected: the range is from the waypoint.
    assert_eq!(s.screen.game.selection(), ids);
    assert_eq!(s.screen.game.onward_from(), Some(VIA));
    assert!(s.unit_line().contains("on from there"), "{}", s.unit_line());
    assert!(s.screen.game.can_move_to(END) && !s.screen.game.can_move_to(Hex::new(13, 10)));
    click(&mut s, END);
    assert_eq!(unit(&s.screen.game, ids[0]).route, [VIA, END]);
    assert!(s.screen.game.selection().is_empty(), "nothing left there: on to the next");
    // Selected again: afresh. Another click is another plan.
    click(&mut s, FROM);
    assert_eq!((s.screen.game.selection(), s.screen.game.onward_from()), (ids.as_slice(), None));
    assert!(s.unit_line().contains("another click: another plan"), "{}", s.unit_line());
    click(&mut s, Hex::new(12, 10));
    assert_eq!(unit(&s.screen.game, ids[0]).route, [Hex::new(12, 10)]);
    assert_eq!(s.screen.game.onward_from(), Some(Hex::new(12, 10)));
    // The stack's own hex: afresh again, without letting go of it.
    click(&mut s, FROM);
    assert_eq!((s.screen.game.selection(), s.screen.game.onward_from()), (ids.as_slice(), None));
}
