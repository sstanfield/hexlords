//! What the review of 2026-11-23 found in the rules, after waypoints and
//! mixed stacks: a refused move shows nothing (and the game is made again
//! from its steps); walkers are not taken onto the water by the flyers'
//! way (a mixed stack goes one way, in turns too); a waypoint gone through
//! is behind the stack; a mixed stack is offered every hex it has a way
//! to that all of it can pay for; a stack sent where another leaves gets
//! there once it has left; a plan that comes back to where it set off has
//! spent its way.

use hexlords::game::Game;
use hexlords::orders::{Event, Order};
use hexlords::setup::{Fog, Mode, Setup};
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{Hex, Side, UnitId};

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

/// Both sides have done: in rounds, the round plays out.
fn round(game: &mut Game) -> Vec<Event> {
    let mut events = game.apply(Order::EndTurn).unwrap();
    events.extend(game.apply(Order::EndTurn).unwrap());
    while game.playing_out() {
        events.extend(game.play_out());
    }
    events
}

/// 30 x 30 grass, red's city top left and blue's top right; soldiers (12
/// points, 2 a hex) and riders (20).
fn plain(mode: Mode, fog: Fog) -> Game {
    let mut rows = vec!["g".repeat(30); 30];
    rows[1] = format!("gc{}cg", "g".repeat(26));
    let terrain = format!("30\n30\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 5\nmoves 12\nenter grass 2, city 1\n\
                 unit riders\nname Riders\nstrength 5\nmoves 20\nenter grass 2, city 1\n";
    let cities = "city 1 1\nname Redhold\nside red\ncity 1 28\nname Bluemoor\nside blue\n";
    start(&terrain, units, cities, mode, fog)
}

/// Finding 1: A move refused for want of room has already looked along its way
/// (`see_along` comes before the refusal in `move_units`): the side sees
/// what it did not, and since a refused order is no step of the game's
/// record, the game made again from its steps is another game.
#[test]
fn a_refused_move_shows_nothing_and_the_game_is_made_again_from_its_steps() {
    let (x, y) = (Hex::new(15, 10), Hex::new(15, 15));
    let build = || {
        let mut game = plain(Mode::Turns, Fog::Total);
        let scout = put(&mut game, Side::Red, "soldiers", x);
        for _ in 0..8 {
            put(&mut game, Side::Red, "soldiers", y);
        }
        (game, scout)
    };
    let (mut game, scout) = build();
    let (mut again, _) = build();
    game.record(true);
    again.record(true);
    let dark = Hex::new(13, 13);
    assert!(!game.sight(Side::Red).knows(dark), "never seen: out of sight of both stacks");
    let known = game.sight(Side::Red).known_count();
    // Onto the side's own full stack: refused.
    let why = game.apply(Order::Move { units: vec![scout], to: y }).unwrap_err();
    assert!(why.contains("no room"), "{why}");
    let known_after = game.sight(Side::Red).known_count();
    // What the refused order showed can now be gone to.
    let sent = game.apply(Order::Move { units: vec![scout], to: dark });
    let mut redone = Vec::new();
    for step in game.take_steps() {
        redone.push(format!("{step:?}: {:?}", again.redo(&step).map(|_| ())));
    }
    assert!(
        known == known_after && sent.is_err() && game.digest() == again.digest(),
        "land known before the refused order {known:?}, after it {known_after:?}; then a move into the dark: \
         {:?}; the steps done again: {redone:?}; the same game: {}",
        sent.map(|_| ()),
        game.digest() == again.digest()
    );
}

/// Finding 2: In turns, with fog: a stack of flyers and units that walk goes to a
/// hex all of it can reach. `move_units` looks for unseen units on the way
/// of the first of them only, its own cheapest way: the eagles' is over a
/// pond with blue's boats on it, out of sight. All of the stack is taken to
/// the boat's hex, the soldiers too, and they drown there.
#[test]
fn walkers_are_not_taken_onto_the_water_by_the_flyers_way() {
    let mut wrong = Vec::new();
    for order in ["move", "goto"] {
        let mut rows = vec!["g".repeat(30); 30];
        for row in rows.iter_mut().take(13).skip(8) {
            *row = format!("{}w{}", "g".repeat(5), "g".repeat(24));
        }
        rows[0] = format!("c{}c", "g".repeat(28));
        let terrain = format!("30\n30\n{}\n", rows.join("\n"));
        let units = "unit eagles\nname Eagles\nstrength 9\nmoves 24\ntravels air\nenter grass 2, water 2, city 1\n\
                     unit soldiers\nname Soldiers\nstrength 3\nmoves 60\nenter grass 2, city 1\n\
                     unit boat\nname Boat\nstrength 1\nmoves 12\ntravels sea\nenter water 2, city 2\n";
        let cities = "city 0 0\nname Redhold\nside red\ncity 0 29\nname Bluemoor\nside blue\n";
        let mut game = start(&terrain, units, cities, Mode::Turns, Fog::On);
        let (from, end) = (Hex::new(10, 2), Hex::new(10, 7));
        let ids = vec![put(&mut game, Side::Red, "eagles", from), put(&mut game, Side::Red, "soldiers", from)];
        for row in 8..=12 {
            put(&mut game, Side::Blue, "boat", Hex::new(row, 5));
        }
        let sight = game.sight(Side::Red);
        assert!((8..=12).all(|row| !sight.sees(Hex::new(row, 5))), "the boats are not seen");
        let events = match order {
            "move" => game.apply(Order::Move { units: ids.clone(), to: end }),
            _ => game.apply(Order::GoTo { units: ids.clone(), to: end, via: Vec::new() }),
        };
        let events = events.unwrap();
        let soldiers = pos(&game, ids[1]);
        let drowned = events.iter().any(|e| matches!(e, Event::Drowned { .. }));
        let met = events.iter().find_map(|e| match e {
            Event::Surprise { at, .. } => Some(*at),
            _ => None,
        });
        if drowned || soldiers.is_none_or(|hex| game.map.open_water(hex)) {
            wrong.push(format!("{order}: met on {met:?}; the soldiers: {soldiers:?}; drowned: {drowned}"));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

/// Finding 3: In rounds, a waypoint that is full when the stack comes by (others of
/// its side got there first) is gone through, as a move goes through its
/// side's stacks; but it stays the plan's next stop, and from the plan's
/// end the stack turns back for it.
#[test]
fn a_waypoint_gone_through_is_behind_the_stack() {
    let mut game = plain(Mode::Rounds, Fog::Off);
    let (from, via, end) = (Hex::new(10, 10), Hex::new(9, 10), Hex::new(7, 10));
    let stack = vec![put(&mut game, Side::Red, "soldiers", from), put(&mut game, Side::Red, "soldiers", from)];
    // Six of the side's stand on the waypoint; two riders beside it are sent
    // there too, and are there first: it is full when the soldiers come by.
    for _ in 0..6 {
        put(&mut game, Side::Red, "soldiers", via);
    }
    let beside = Hex::new(8, 10);
    let riders = vec![put(&mut game, Side::Red, "riders", beside), put(&mut game, Side::Red, "riders", beside)];
    game.apply(Order::Move { units: stack.clone(), to: via }).unwrap();
    game.apply(Order::Onward { units: stack.clone(), to: end }).unwrap();
    assert_eq!(game.map.units().get(stack[0]).unwrap().way, [via, beside, end]);
    game.apply(Order::Move { units: riders.clone(), to: via }).unwrap();
    let events = round(&mut game);
    let went: Vec<Hex> = events
        .iter()
        .filter_map(|e| match e {
            Event::Moved { unit, to, .. } if *unit == stack[0] => Some(*to),
            _ => None,
        })
        .collect();
    assert_eq!(pos(&game, riders[0]), Some(via), "the riders are on the waypoint");
    assert_eq!(pos(&game, stack[0]), Some(end), "the soldiers went {went:?}");
}

/// Finding 4: In rounds, archers and highlanders (the continent's kinds) on one
/// hex, and one way on: a hill, a forest, a forest. It costs the archers 7
/// of their 10 points and the highlanders 8 of their 8: every one of them
/// can pay for it, each alone is let go, and in turns the two go together.
/// In rounds the hex is not offered to the two, and the order is refused.
#[test]
fn a_mixed_stack_is_offered_a_way_every_one_of_it_can_pay_for() {
    let corridor = |mode: Mode| {
        let mut rows = vec!["m".repeat(20); 20];
        rows[0] = format!("c{}c", "m".repeat(18));
        let mut set = |r: usize, c: usize, ch: &str| rows[r].replace_range(c..=c, ch);
        set(10, 5, "g");
        set(9, 5, "h");
        set(8, 5, "f");
        set(7, 5, "f");
        let terrain = format!("20\n20\n{}\n", rows.join("\n"));
        let units = "unit archers\nname Archers\nstrength 3\nmoves 10\nenter grass 2, forest 2, hill 3, city 1\n\
                     unit highlanders\nname Highlanders\nstrength 3\nmoves 8\nenter grass 2, forest 3, hill 2, city 1\n";
        let cities = "city 0 0\nname Redhold\nside red\ncity 0 19\nname Bluemoor\nside blue\n";
        let mut game = start(&terrain, units, cities, mode, Fog::Off);
        let a = put(&mut game, Side::Red, "archers", Hex::new(10, 5));
        let h = put(&mut game, Side::Red, "highlanders", Hex::new(10, 5));
        (game, vec![a, h], Hex::new(7, 5))
    };
    // Each alone, in rounds: it gets there.
    for i in 0..2 {
        let (mut game, ids, end) = corridor(Mode::Rounds);
        game.apply(Order::Move { units: vec![ids[i]], to: end }).unwrap();
        round(&mut game);
        assert_eq!(pos(&game, ids[i]), Some(end));
    }
    // The two, in turns: they get there.
    let (mut game, ids, end) = corridor(Mode::Turns);
    game.apply(Order::Move { units: ids.clone(), to: end }).unwrap();
    assert_eq!((pos(&game, ids[0]), pos(&game, ids[1])), (Some(end), Some(end)));
    // The two, in rounds.
    let (mut game, ids, end) = corridor(Mode::Rounds);
    assert!(game.select_unit(ids[0]));
    game.toggle_unit(ids[1]);
    assert_eq!(game.selection(), ids);
    let shown = game.can_move_to(end);
    let done = game.apply(Order::Move { units: ids.clone(), to: end });
    assert!(shown && done.is_ok(), "shown in reach: {shown}; the order: {done:?}");
}

/// Finding 5: A stack sent onto the hex of a bigger one that leaves by way of its
/// own hex: in turns both moves are made. In rounds each waits for the
/// other; when the wait is broken (`hasty`) the big one goes through, and
/// the small one, looked at in the same pass while the big one still
/// stands there, is stopped for the round: it never goes to the hex that
/// is empty a moment later.
#[test]
fn a_stack_sent_where_another_leaves_gets_there_once_it_has_left() {
    let (p, r, far) = (Hex::new(10, 10), Hex::new(9, 10), Hex::new(8, 10));
    for mode in [Mode::Turns, Mode::Rounds] {
        let mut game = plain(mode, Fog::Off);
        let big: Vec<UnitId> = (0..8).map(|_| put(&mut game, Side::Red, "soldiers", p)).collect();
        let one = put(&mut game, Side::Red, "soldiers", r);
        // The eight go north through the one's hex; the one goes to where they stood.
        game.apply(Order::Move { units: big.clone(), to: far }).unwrap();
        game.apply(Order::Move { units: vec![one], to: p }).unwrap();
        if mode == Mode::Rounds {
            round(&mut game);
        }
        assert_eq!((pos(&game, big[0]), pos(&game, one)), (Some(far), Some(p)), "{mode:?}: the eight, the one");
    }
}

/// Finding 6: A plan that comes back to the hex its stack stands on (out two
/// hexes, back two: 8 of 12 points) is taken for no plan at all when it is
/// sent on: what is left there is counted as all the stack has, and the
/// legs so far are dropped.
#[test]
fn a_plan_that_comes_back_to_where_it_set_off_has_spent_its_way() {
    let mut game = plain(Mode::Rounds, Fog::Off);
    let (from, out) = (Hex::new(10, 10), Hex::new(8, 10));
    let ids = vec![put(&mut game, Side::Red, "soldiers", from)];
    game.apply(Order::Move { units: ids.clone(), to: out }).unwrap();
    game.apply(Order::Onward { units: ids.clone(), to: from }).unwrap();
    assert_eq!(game.plan_end(&ids), Some(from));
    let selected = game.select_onward(&ids);
    let (near, far) = (Hex::new(12, 10), Hex::new(15, 10));
    let costs = game.range().map(|r| (r.cost(near), r.cost(far)));
    let done = game.apply(Order::Onward { units: ids.clone(), to: far });
    let route = game.map.units().get(ids[0]).unwrap().route.clone();
    assert!(
        done.is_err() && costs == Some((Some(4), None)),
        "selected to go on: {selected}; two hexes on and five hexes on cost {costs:?}; five hexes on: {done:?}; \
         the route is {route:?}"
    );
}
