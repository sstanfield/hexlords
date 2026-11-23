//! A stack of flyers and units that walk stays together (the owner's word,
//! 2026-11-22: "I observed a stack of flyers and non-flyers splitting up
//! and only the flyers moving because the chosen route was over the water.
//! A mixed stack should always stay and move together"): its way is one
//! all of it can go, and it goes as one or not at all.

use hexlords::game::Game;
use hexlords::orders::Order;
use hexlords::setup::{Mode, Setup};
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{Hex, Side, UnitId};

const FROM: Hex = Hex::new(10, 4);
const OVER: Hex = Hex::new(10, 12);

/// 20 x 20 grass with a lake three hexes wide down the middle (columns 7
/// to 9), open at the top (rows 0 to 2): red's eagles (they fly: 16 points,
/// water as cheap as grass; fewer points than the soldiers, so the way was
/// theirs) and soldiers (18 points) west of it, in `mode`.
fn game(mode: Mode) -> (Game, Vec<UnitId>) {
    let mut rows = vec!["g".repeat(20); 20];
    for row in rows.iter_mut().skip(3) {
        *row = format!("{}www{}", "g".repeat(7), "g".repeat(10));
    }
    rows[0] = format!("c{}c", "g".repeat(18));
    let terrain = format!("20\n20\n{}\n", rows.join("\n"));
    let units = "unit eagles\nname Eagles\nstrength 3\nmoves 16\ntravels air\nenter grass 2, water 2, city 1\n\
                 unit soldiers\nname Soldiers\nstrength 3\nmoves 18\nenter grass 2, city 1\n";
    let cities = "city 0 0\nname Redhold\nside red\ncity 0 19\nname Bluemoor\nside blue\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut ids = Vec::new();
    for kind in ["eagles", "soldiers", "soldiers"] {
        let kind = game.map.units().kind_named(kind).unwrap();
        let id = game.map.new_unit_of(Side::Red, kind).unwrap();
        game.map.place_unit(id, FROM).unwrap();
        ids.push(id);
    }
    let mut setup = Setup::solo(&game.map, None, "passive");
    setup.set_players("red=passive,blue=passive").unwrap();
    setup.mode = mode;
    (game.with_setup(setup), ids)
}

fn places(game: &Game, ids: &[UnitId]) -> Vec<Hex> {
    ids.iter().map(|id| game.map.units().get(*id).unwrap().pos()).collect()
}

/// Both sides have done: the next round (in rounds, played out).
fn round(game: &mut Game) {
    game.apply(Order::EndTurn).unwrap();
    game.apply(Order::EndTurn).unwrap();
}

fn water(game: &Game, hex: Hex) -> bool {
    game.map.open_water(hex)
}

#[test]
fn in_turns_a_mixed_stack_sent_far_goes_round_the_water_together() {
    let (mut game, ids) = game(Mode::Turns);
    game.apply(Order::GoTo { units: ids.clone(), to: OVER, via: Vec::new() }).unwrap();
    // The eagles alone would be over the lake; with the soldiers they go
    // north, round it, and all three are on one hex each turn.
    for turn in 0..8 {
        let at = places(&game, &ids);
        assert!(at.iter().all(|hex| *hex == at[0]), "turn {turn}: together: {at:?}");
        assert!(!water(&game, at[0]), "turn {turn}: on land: {at:?}");
        if at[0] == OVER {
            break;
        }
        assert!(turn > 0 || at[0] != FROM, "they set off");
        round(&mut game);
    }
    assert_eq!(places(&game, &ids), [OVER; 3], "and they get there");
    // Toward, the same: as far as all of them get, together.
    let (mut game, ids) = self::game(Mode::Turns);
    game.apply(Order::Toward { units: ids.clone(), to: OVER }).unwrap();
    let at = places(&game, &ids);
    assert!(at.iter().all(|hex| *hex == at[0] && *hex != FROM && !water(&game, *hex)), "{at:?}");
}

#[test]
fn in_rounds_a_mixed_stack_goes_one_way_all_of_it_can() {
    // Sent far (Go to): round the lake, together at every round's end.
    let (mut game, ids) = game(Mode::Rounds);
    game.apply(Order::GoTo { units: ids.clone(), to: OVER, via: Vec::new() }).unwrap();
    for turn in 0..8 {
        round(&mut game);
        let at = places(&game, &ids);
        assert!(at.iter().all(|hex| *hex == at[0]), "round {turn}: together: {at:?}");
        assert!(!water(&game, at[0]), "round {turn}: on land: {at:?}");
        assert!(turn > 0 || at[0] != FROM, "they set off");
        if at[0] == OVER {
            break;
        }
    }
    assert_eq!(places(&game, &ids), [OVER; 3]);
}

#[test]
fn in_rounds_a_mixed_stack_is_offered_only_where_all_of_it_can_go() {
    let (mut game, ids) = game(Mode::Rounds);
    assert!(game.select_unit(ids[0]));
    for id in &ids[1..] {
        game.toggle_unit(*id);
    }
    assert_eq!(game.selection(), ids);
    // Not the lake, nor the far shore (the eagles alone would get there:
    // 8 hexes of 2 points); the grass on their side, as far as the eagles'
    // 16 points go, a point of the stack's a hex.
    let range = game.range().unwrap();
    assert!(range.cost(Hex::new(10, 8)).is_none() && range.cost(OVER).is_none());
    assert_eq!(range.cost(Hex::new(10, 2)), Some(4));
    assert_eq!(range.cost(Hex::new(10, 5)), Some(2));
    assert!(range.reachable().all(|(hex, cost)| !water(&game, hex) && cost <= 16));
    // A plan there: they go, together, and each pays its own price.
    let near = Hex::new(4, 5);
    game.apply(Order::Move { units: ids.clone(), to: near }).unwrap();
    round(&mut game);
    assert_eq!(places(&game, &ids), [near; 3]);
    // Across the water is refused for the stack; the eagles alone may.
    let (mut game, ids) = self::game(Mode::Rounds);
    let why = game.apply(Order::Move { units: ids.clone(), to: OVER }).unwrap_err();
    assert!(why.contains("together"), "{why}");
    game.apply(Order::Move { units: vec![ids[0]], to: OVER }).unwrap();
    round(&mut game);
    assert_eq!(places(&game, &ids), [OVER, FROM, FROM]);
}
