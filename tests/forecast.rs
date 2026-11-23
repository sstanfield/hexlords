//! The forecast of a side's pay days (`Game::forecast`), for the production
//! window: what its cities will finish, and its gold, round by round. It
//! is the rules played forward, so it is held to them: what it says is what
//! real turns then do, while nothing else changes.

use hexlords::game::{Forecast, Game, Wait};
use hexlords::orders::{Event, Order, RallyPoint};
use hexlords::setup::{Mode, Setup};
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{CityId, Hex, QueueEnd, QueueItem, Sends, Side};

const REDHOLD: Hex = Hex::new(5, 5);
const SOUTHWICK: Hex = Hex::new(15, 5);
const BLUEMOOR: Hex = Hex::new(5, 15);

/// Red's Redhold (taxes 6) and Southwick (taxes 4), with `redhold` in the
/// first; blue's Bluemoor, far off. Red starts with `gold`; both passive.
fn game(redhold: &str, gold: u32, setup: impl FnOnce(&mut Setup)) -> Game {
    let mut rows = vec!["g".repeat(20); 20];
    rows[5] = "gggggcgggggggggcgggg".to_string();
    rows[15] = "gggggcgggggggggggggg".to_string();
    let terrain = format!("20\n20\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nupkeep 2\nprice 10\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 12\nupkeep 1\nprice 3\nenter grass 2, city 1\n\
                 unit knights\nname Knights\nstrength 6\nmoves 12\nupkeep 4\nprice 12\nenter grass 2, city 1\n\
                 unit captain\nname Captain\nstrength 2\nmoves 12\nupkeep 3\ncommander 1\nenter grass 2, city 1\n";
    let cities = format!(
        "city 5 5\nname Redhold\nside red\nincome 6\nproduce soldiers 1\nproduce levy 1\nproduce captain 1\n{redhold}\
         city 15 5\nname Southwick\nside red\nincome 4\nproduce knights 2\nproduce levy 1\n\
         city 5 15\nname Bluemoor\nside blue\n"
    );
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(&cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut s = Setup::solo(&game.map, Some(Side::Red), "passive");
    s.gold = gold;
    setup(&mut s);
    game.with_setup(s)
}

fn city(game: &Game, hex: Hex) -> CityId {
    game.map.city_at(hex).unwrap().id
}

/// Red's next pay day, and the events of the turns (or the round) on the
/// way to it.
fn next_round(game: &mut Game) -> Vec<Event> {
    let mut events = game.end_turn();
    events.extend(game.end_turn());
    events
}

/// What red's cities built, by the events: (city, kind), in order.
fn built(game: &Game, events: &[Event]) -> Vec<(CityId, String)> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Built { city, unit } => {
                let unit = game.map.units().get(*unit)?;
                (unit.side() == Side::Red).then(|| (*city, unit.name.clone()))
            }
            _ => None,
        })
        .collect()
}

fn said(forecast: &Forecast, day: usize) -> Vec<(CityId, String)> {
    forecast.days[day].built.iter().map(|b| (b.city, b.kind.clone())).collect()
}

/// Redhold on a queue (two levies that stay, then soldiers sent to
/// Southwick, once again from the start), Southwick building knights (two
/// turns each) to defend it: the forecast says, day by day, the gold and
/// what each city finishes, and the turns then do just that.
fn agrees_with_real_turns(mode: Mode) {
    let mut game = game("garrison levy\n", 200, |s| s.mode = mode);
    let (redhold, southwick) = (city(&game, REDHOLD), city(&game, SOUTHWICK));
    let items = vec![
        QueueItem { what: 1, count: 2, sends: Sends::Stay },
        QueueItem { what: 0, count: 1, sends: Sends::Route(vec![SOUTHWICK]) },
    ];
    game.apply(Order::Queue { city: redhold, items, end: QueueEnd::Repeat { from: 0, times: Some(1) } }).unwrap();
    game.apply(Order::Build { city: southwick, what: Some(0) }).unwrap();
    game.apply(Order::Rally { city: southwick, to: Sends::Defend }).unwrap();
    let forecast = game.forecast(Side::Red, 8);
    assert_eq!(forecast.days.len(), 8);
    assert_eq!(forecast.short_at, None);
    assert_eq!(forecast.days[0].round, 2, "round 1 has no pay day");
    let mut seen_built = 0;
    for (day, said_day) in forecast.days.iter().enumerate() {
        let events = next_round(&mut game);
        assert_eq!(game.turn(), said_day.round);
        assert_eq!(built(&game, &events), said(&forecast, day), "day {day} ({mode:?})");
        assert_eq!(game.gold(Side::Red), said_day.gold_after, "day {day} ({mode:?})");
        seen_built += said_day.built.len();
    }
    assert!(seen_built >= 6, "it built something to compare: {seen_built}");
    // The queue ran out (two passes of it): Redhold builds nothing now.
    assert!(game.map.city_at(REDHOLD).unwrap().production().is_none());
    let last = &forecast.days.last().unwrap();
    assert!(last.built.iter().all(|b| b.city == southwick), "{last:?}");
}

#[test]
fn the_forecast_is_what_the_turns_then_do() {
    agrees_with_real_turns(Mode::Turns);
}

#[test]
fn the_forecast_is_what_the_rounds_then_do() {
    agrees_with_real_turns(Mode::Rounds);
}

/// A unit that is done waits for the gold: the next day, with the taxes in,
/// it is out.
#[test]
fn a_city_waits_for_the_gold_then_builds() {
    let mut game = game("", 0, |_| {});
    game.apply(Order::Build { city: city(&game, REDHOLD), what: Some(0) }).unwrap();
    let forecast = game.forecast(Side::Red, 2);
    let redhold = city(&game, REDHOLD);
    // Day 1: 6 + 4 in, soldiers at 10: paid. (Taxes 10.)
    assert_eq!(said(&forecast, 0), [(redhold, "Soldiers".to_string())]);
    assert_eq!(forecast.days[0].gold_after, 0);
    // Day 2: 10 in, 2 upkeep: 8, short of 10.
    assert!(forecast.days[1].built.is_empty());
    assert_eq!(forecast.days[1].waiting, [(redhold, Wait::Gold)]);
    for day in &forecast.days {
        next_round(&mut game);
        assert_eq!(game.gold(Side::Red), day.gold_after);
    }
}

/// A full city waits for room (a new unit is placed in its city before it
/// is sent anywhere); one whose units go to a rally point does not fill up.
#[test]
fn a_full_city_waits_for_room_unless_its_units_go_elsewhere() {
    let mut game = game("garrison levy 8\n", 100, |_| {});
    let redhold = city(&game, REDHOLD);
    game.apply(Order::Build { city: redhold, what: Some(1) }).unwrap();
    let forecast = game.forecast(Side::Red, 1);
    assert_eq!(forecast.days[0].waiting, [(redhold, Wait::Room)]);
    // A captain still has room: a hex holds a commander besides.
    game.apply(Order::Build { city: redhold, what: Some(2) }).unwrap();
    let forecast = game.forecast(Side::Red, 2);
    assert_eq!(said(&forecast, 0), [(redhold, "Captain".to_string())]);
    assert_eq!(forecast.days[1].waiting, [(redhold, Wait::Room)], "and one only");
    // With room for one: levies that stay fill it, and the next waits;
    // levies sent to a rally point leave, and there is room each day.
    let levy = game.map.units_at(REDHOLD)[0];
    game.apply(Order::Disband { units: vec![levy] }).unwrap();
    game.apply(Order::Build { city: redhold, what: Some(1) }).unwrap();
    let forecast = game.forecast(Side::Red, 2);
    assert_eq!(said(&forecast, 0), [(redhold, "Levy".to_string())]);
    assert_eq!(forecast.days[1].waiting, [(redhold, Wait::Room)]);
    let point = RallyPoint { at: Hex::new(10, 10), defend: false };
    game.apply(Order::RallyPoint { n: 0, point: Some(point) }).unwrap();
    game.apply(Order::Build { city: redhold, what: Some(1) }).unwrap();
    game.apply(Order::Rally { city: redhold, to: Sends::Point { n: 0, bless: false } }).unwrap();
    let forecast = game.forecast(Side::Red, 3);
    assert!(forecast.days.iter().all(|d| d.waiting.is_empty() && d.built.len() == 1), "{forecast:?}");
    // And the turns agree.
    for day in &forecast.days {
        let events = next_round(&mut game);
        assert_eq!(built(&game, &events).len(), day.built.len());
        assert_eq!(game.gold(Side::Red), day.gold_after);
    }
}

/// The first pay day the gold will not pay the upkeep is found, and the
/// forecast stops there: the rules then send units away.
#[test]
fn the_first_round_the_upkeep_goes_unpaid_is_found() {
    // Upkeep 2 x 4 = 8 against taxes of 10, and soldiers built each turn
    // (10, upkeep 2): the gold runs down.
    let mut game = game("garrison soldiers 4\n", 30, |_| {});
    game.apply(Order::Build { city: city(&game, REDHOLD), what: Some(0) }).unwrap();
    let forecast = game.forecast(Side::Red, 20);
    let short = forecast.short_at.expect("the gold runs out");
    let day = forecast.days.last().unwrap();
    assert!(day.short && day.round == short && day.built.is_empty());
    assert!(forecast.days[..forecast.days.len() - 1].iter().all(|d| !d.short));
    // The turns: no unit leaves before that round; one does in it.
    let deserted = |events: &[Event]| events.iter().any(|e| matches!(e, Event::Deserted { side: Side::Red, .. }));
    while game.turn() < short {
        let events = next_round(&mut game);
        assert!(!deserted(&events) || game.turn() == short, "round {}", game.turn());
        if game.turn() == short {
            assert!(deserted(&events), "units leave in round {short}");
        }
    }
}

/// In turns, a side whose turn is still to come this round has its pay day
/// in it; the side playing now has had its own.
#[test]
fn a_side_still_to_play_has_its_pay_day_this_round() {
    let mut game = game("", 30, |_| {});
    next_round(&mut game);
    assert_eq!((game.turn(), game.player()), (2, Side::Red));
    assert_eq!(game.forecast(Side::Red, 1).days[0].round, 3);
    assert_eq!(game.forecast(Side::Blue, 1).days[0].round, 2);
    // And what blue's comes to is what it gets.
    let said = game.forecast(Side::Blue, 1).days[0].gold_after;
    game.end_turn();
    assert_eq!((game.player(), game.gold(Side::Blue)), (Side::Blue, said));
    let _ = BLUEMOOR;
}
