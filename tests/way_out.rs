//! A full city lets out what it sends elsewhere (the owner's word,
//! 2026-11-23: "produce it if it can leave the city and let it go otherwise
//! don't produce it"): the new unit stands on the first hex of its way, the
//! best beside the city that is free (room for it, nobody else's units,
//! ground it can go on), its step paid for, and goes on as any unit its
//! city sends does. A city whose units stay, or whose unit has no such hex
//! to step out to, waits as before.

use hexlords::game::{Game, Wait};
use hexlords::orders::{Event, Order, RallyPoint};
use hexlords::setup::{Mode, Setup};
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{Hex, Sends, Side, UnitId};

const ALDER: Hex = Hex::new(5, 5);
const POINT: Hex = Hex::new(5, 12);

/// Red's Alder (taxes 10) with eight levies in it, building levies (a
/// turn, 3 gold); `rows` over the plain grass map; blue's Bluemoor far off.
fn game(rows: &[(usize, &str)], setup: impl FnOnce(&mut Setup)) -> Game {
    let mut terrain = vec!["g".repeat(20); 20];
    terrain[5] = "gggggcgggggggggggggg".to_string();
    terrain[15] = "gggggggggggggggcgggg".to_string();
    for (row, text) in rows {
        terrain[*row] = text.to_string();
    }
    let terrain = format!("20\n20\n{}\n", terrain.join("\n"));
    let units = "unit levy\nname Levy\nstrength 1\nmoves 12\nupkeep 1\nprice 3\nenter grass 2, city 1\n\
                 unit hawk\nname Hawk\nstrength 1\nmoves 12\nupkeep 1\nprice 3\ntravels air\nenter grass 2, water 2, city 1\n";
    let cities = "city 5 5\nname Alder\nside red\nincome 10\nproduce levy 1\nproduce hawk 1\ngarrison levy 8\n\
                  city 15 15\nname Bluemoor\nside blue\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let mut game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut s = Setup::solo(&game.map, Some(Side::Red), "passive");
    s.gold = 50;
    setup(&mut s);
    game = game.with_setup(s);
    let alder = game.map.city_at(ALDER).unwrap().id;
    game.apply(Order::Build { city: alder, what: Some(0) }).unwrap();
    game
}

fn to_point(game: &mut Game) {
    let alder = game.map.city_at(ALDER).unwrap().id;
    game.apply(Order::RallyPoint { n: 0, point: Some(RallyPoint { at: POINT, defend: false }) }).unwrap();
    game.apply(Order::Rally { city: alder, to: Sends::Point { n: 0, bless: false } }).unwrap();
}

/// True if `hex` is next to Alder, a column nearer the point.
fn beside_toward_the_point(game: &Game, hex: Hex) -> bool {
    game.map.neighbours(ALDER).any(|h| h == hex) && hex.col == ALDER.col + 1
}

/// Red's next pay day: the units its cities built.
fn next_round(game: &mut Game) -> Vec<UnitId> {
    let mut events = game.end_turn();
    events.extend(game.end_turn());
    events
        .iter()
        .filter_map(|e| match e {
            Event::Built { unit, .. } => Some(*unit),
            _ => None,
        })
        .filter(|id| game.map.units().get(*id).is_some_and(|u| u.side() == Side::Red))
        .collect()
}

#[test]
fn a_full_city_lets_out_what_it_sends_to_a_rally_point() {
    let mut game = game(&[], |_| {});
    to_point(&mut game);
    let forecast = game.forecast(Side::Red, 1);
    assert_eq!(forecast.days[0].built.len(), 1, "the forecast says so: {forecast:?}");
    let built = next_round(&mut game);
    assert_eq!(built.len(), 1, "the levy is out");
    let levy = game.map.units().get(built[0]).unwrap();
    // On the first hex of its way, toward the point, its step paid for.
    assert!(beside_toward_the_point(&game, levy.pos()), "{:?}", levy.pos());
    assert_eq!(levy.movement, 12 - 2);
    assert_eq!(levy.route, [POINT]);
    assert_eq!(levy.bound, Some(0));
    assert_eq!(game.map.units_at(ALDER).len(), 8, "the city as full as it was");
    assert_eq!(game.gold(Side::Red), forecast.days[0].gold_after);
    // At the end of the turn it goes on, as any unit sent there does.
    let id = built[0];
    let out = game.map.units().get(id).unwrap().pos();
    game.end_turn();
    assert!(game.map.units().get(id).unwrap().pos().col > out.col, "it marched on");
}

#[test]
fn in_rounds_too() {
    let mut game = game(&[], |s| s.mode = Mode::Rounds);
    to_point(&mut game);
    let built = next_round(&mut game);
    assert_eq!(built.len(), 1);
    assert!(beside_toward_the_point(&game, game.map.units().get(built[0]).unwrap().pos()));
}

#[test]
fn a_full_city_whose_units_stay_waits_as_before() {
    let mut game = game(&[], |_| {});
    assert_eq!(game.forecast(Side::Red, 1).days[0].waiting.len(), 1);
    assert!(next_round(&mut game).is_empty());
    assert_eq!(game.map.units_at(ALDER).len(), 8);
}

/// Ringed by another side's units: no way out, no unit (it does not fight
/// its way out).
#[test]
fn a_city_ringed_by_another_sides_units_lets_nothing_out() {
    let mut game = game(&[], |_| {});
    to_point(&mut game);
    let levy = game.map.units().kind_named("levy").unwrap();
    let around: Vec<Hex> = game.map.neighbours(ALDER).collect();
    for hex in around {
        let id = game.map.new_unit_of(Side::Blue, levy).unwrap();
        game.map.place_unit(id, hex).unwrap();
    }
    let forecast = game.forecast(Side::Red, 1);
    assert_eq!(forecast.days[0].waiting.first().map(|w| w.1), Some(Wait::Room), "{forecast:?}");
    assert!(next_round(&mut game).is_empty());
    assert!(game.map.neighbours(ALDER).all(|h| game.map.units_at(h).len() == 1), "nobody joined blue's");
}

/// A full stack of the side's own on the best hex out: the unit steps out
/// onto the best of the others that is free (it once waited, though
/// another hex beside the city had room).
#[test]
fn a_full_stack_on_the_best_hex_out_is_gone_round() {
    let mut open = game(&[], |_| {});
    to_point(&mut open);
    let first = next_round(&mut open)[0];
    let best = open.map.units().get(first).unwrap().pos();
    let mut game = game(&[], |_| {});
    to_point(&mut game);
    let levy = game.map.units().kind_named("levy").unwrap();
    for _ in 0..8 {
        let id = game.map.new_unit_of(Side::Red, levy).unwrap();
        game.map.place_unit(id, best).unwrap();
    }
    let forecast = game.forecast(Side::Red, 1);
    assert_eq!(forecast.days[0].built.len(), 1, "{forecast:?}");
    let built = next_round(&mut game);
    assert_eq!(built.len(), 1, "out by another hex");
    let at = game.map.units().get(built[0]).unwrap().pos();
    assert!(at != best && game.map.neighbours(ALDER).any(|h| h == at), "{at:?} beside Alder, not {best:?}");
    assert_eq!(game.map.units_at(best).len(), 8, "the full stack as it was");
    assert_eq!(game.gold(Side::Red), forecast.days[0].gold_after);
}

/// Every hex round the city full of the side's own: nowhere to step out
/// to, and no unit.
#[test]
fn no_room_on_the_way_out_no_unit() {
    let mut game = game(&[], |s| s.gold = 1000);
    to_point(&mut game);
    let levy = game.map.units().kind_named("levy").unwrap();
    let around: Vec<Hex> = game.map.neighbours(ALDER).collect();
    for hex in around {
        for _ in 0..8 {
            let id = game.map.new_unit_of(Side::Red, levy).unwrap();
            game.map.place_unit(id, hex).unwrap();
        }
    }
    assert_eq!(game.forecast(Side::Red, 1).days[0].waiting.len(), 1);
    assert!(next_round(&mut game).is_empty());
}

/// A walker built where only flyers go out (its city alone on an island):
/// it cannot leave, so it is not built; a flyer is.
#[test]
fn a_walker_on_an_island_waits_and_a_flyer_goes() {
    let water = "w".repeat(20);
    let row5 = "wwwwwcwwwwwwgggggggg";
    let rows = [(3, water.as_str()), (4, water.as_str()), (5, row5), (6, water.as_str()), (7, water.as_str())];
    let mut island = game(&rows, |_| {});
    to_point(&mut island);
    let forecast = island.forecast(Side::Red, 1);
    assert_eq!(forecast.days[0].waiting.first().map(|w| w.1), Some(Wait::Room), "{forecast:?}");
    assert!(next_round(&mut island).is_empty(), "the levy cannot leave: it is not built");
    let alder = island.map.city_at(ALDER).unwrap().id;
    island.apply(Order::Build { city: alder, what: Some(1) }).unwrap();
    let built = next_round(&mut island);
    assert_eq!(built.len(), 1, "the hawk is out");
    let hawk = island.map.units().get(built[0]).unwrap();
    assert!(beside_toward_the_point(&island, hawk.pos()), "over the water, toward the point: {:?}", hawk.pos());
}
