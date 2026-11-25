//! Gold: each side's treasury, the taxes its cities bring in at the start of
//! its turns, the upkeep its units cost (those it cannot pay leave), and the
//! price of what its cities build (paid when it is done; it waits for the
//! gold). The agent that spends within its means; what the screen shows.

mod common;

use common::{assets, click, widget};
use hexlords::agents::{Agent, Simple};
use hexlords::game::Game;
use hexlords::info;
use hexlords::orders::{Event, Order};
use hexlords::session::Session;
use hexlords::setup::Setup;
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{Hex, Side, UnitId};
use turbo_ferret::{Point, Size};

const REDHOLD: Hex = Hex::new(5, 5);
const BLUEMOOR: Hex = Hex::new(5, 15);

/// Red's Redhold (taxes 6) with `garrison`, building soldiers (price 10, a
/// turn each, upkeep 2); blue's Bluemoor with nobody. Red starts with
/// `gold`; both sides are passive, red a person unless `setup` says not.
fn game(garrison: &str, gold: u32, setup: impl FnOnce(&mut Setup)) -> Game {
    let mut rows = vec!["g".repeat(20); 20];
    rows[5] = "gggggcgggggggggcgggg".to_string();
    let terrain = format!("20\n20\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nupkeep 2\nprice 10\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 12\nupkeep 1\nprice 3\nenter grass 2, city 1\n\
                 unit captain\nname Captain\nstrength 2\nmoves 12\nupkeep 3\ncommander 1\nenter grass 2, city 1\n";
    let cities = format!(
        "city 5 5\nname Redhold\nside red\nincome 6\nproduce soldiers 1\nproduce levy 1\n{garrison}\
         city 5 15\nname Bluemoor\nside blue\n"
    );
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(&cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut s = Setup::solo(&game.map, Some(Side::Red), "passive");
    s.gold = gold;
    setup(&mut s);
    game.with_setup(s)
}

/// Red's turn again: blue's (passive) in between.
fn next_round(game: &mut Game) {
    game.end_turn();
    game.end_turn();
}

#[test]
fn taxes_come_in_and_upkeep_goes_out_at_the_start_of_each_turn() {
    let mut game = game("garrison soldiers 2\n", 30, |_| {});
    assert_eq!((game.gold(Side::Red), game.gold(Side::Blue)), (30, 30), "the setup's gold, each");
    assert_eq!((game.income(Side::Red), game.upkeep(Side::Red)), (6, 4));
    assert_eq!(game.income(Side::Blue), 10, "a city whose map says nothing brings 10");
    // Round 1 is the start: nothing comes or goes. Then each of red's turns:
    // +6 taxes, -4 upkeep.
    next_round(&mut game);
    assert_eq!(game.gold(Side::Red), 32);
    next_round(&mut game);
    assert_eq!(game.gold(Side::Red), 34);
}

#[test]
fn what_a_city_finishes_is_paid_for_or_waits_for_the_gold() {
    // 9 gold: the soldiers (10) are done, but they wait.
    let mut game = game("", 9, |_| {});
    let redhold = game.map.city_at(REDHOLD).unwrap().id;
    game.apply(Order::Build { city: redhold, what: Some(0) }).unwrap();
    game.end_turn();
    game.end_turn();
    // Round 2: +6 taxes (15), the soldiers paid (5); round 3: +6 -2 (9),
    // the next ones wait: 9 is not 10.
    assert_eq!(game.map.units_at(REDHOLD).len(), 1);
    assert_eq!(game.gold(Side::Red), 5);
    next_round(&mut game);
    assert_eq!((game.map.units_at(REDHOLD).len(), game.gold(Side::Red)), (1, 9));
    let city = game.map.city_at(REDHOLD).unwrap();
    assert_eq!(info::progress_short(&game.map, city), "Soldiers: they wait for 10 gold");
    assert_eq!(info::progress_line(&game.map, city), "Soldiers done: they wait for 10 gold");
    // Round 4: 9 + 6 - 2 = 13: paid, 3 left.
    next_round(&mut game);
    assert_eq!((game.map.units_at(REDHOLD).len(), game.gold(Side::Red)), (2, 3));
}

#[test]
fn units_that_cannot_be_paid_leave_as_few_as_make_it_up() {
    // Upkeep 2 + 2 + 1 + 3 (the captain) = 8 against 6 taxes and no gold:
    // 2 short. A soldier (2) makes it up alone; the levy (1) would not.
    let mut game = game("garrison soldiers 2\ngarrison levy\ngarrison captain\n", 0, |_| {});
    next_round(&mut game);
    let names: Vec<String> =
        game.map.units_at(REDHOLD).iter().map(|id| game.map.units().get(*id).unwrap().name.clone()).collect();
    assert_eq!(names, ["Soldiers", "Levy", "Captain"]);
    assert_eq!(game.gold(Side::Red), 0);
    let deserted: Vec<&Event> =
        game.history().iter().map(|r| &r.event).filter(|e| matches!(e, Event::Deserted { .. })).collect();
    assert_eq!(deserted, [&Event::Deserted { side: Side::Red, name: "Soldiers".to_string() }]);
    // The log says so.
    let lines = info::log(&game, Side::Red, 0);
    assert_eq!(lines[0].1[0].lines, ["Unpaid, your Soldiers left you: the treasury could not pay their upkeep."]);
    // A commander goes last: with taxes of 0, everyone else goes first.
    let redhold = game.map.city_at(REDHOLD).unwrap().id;
    game.map.city_mut(redhold).unwrap().income = 0;
    next_round(&mut game);
    let left: Vec<UnitId> = game.map.units_at(REDHOLD).to_vec();
    assert_eq!(left.len(), 0, "nothing paid, nobody stays");
    let order: Vec<String> = game
        .history()
        .iter()
        .filter_map(|r| match &r.event {
            Event::Deserted { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(order, ["Soldiers", "Soldiers", "Levy", "Captain"]);
}

#[test]
fn the_simple_agent_builds_what_it_can_keep() {
    // Taxes 6, upkeep 2 (a soldier): 4 to spare: soldiers (upkeep 2) it can
    // keep; it sets Redhold to build them.
    let game = game("garrison soldiers\n", 50, |s| s.set_players("red=simple").unwrap());
    let redhold = game.map.city_at(REDHOLD).unwrap().id;
    let mut agent = Simple::default();
    assert_eq!(agent.next_order(&game, Side::Red), Order::Build { city: redhold, what: Some(0) });
    // Upkeep 6 of taxes 6: nothing to spare; a city building soldiers stops.
    let mut game = self::game("garrison soldiers 3\n", 50, |s| s.set_players("red=simple").unwrap());
    game.map.set_production(redhold, Some(0));
    let mut agent = Simple::default();
    assert_eq!(agent.next_order(&game, Side::Red), Order::Build { city: redhold, what: None });
    // And then it does not start again.
    game.map.set_production(redhold, None);
    assert_ne!(agent.next_order(&game, Side::Red), Order::Build { city: redhold, what: Some(0) });
}

#[test]
fn a_map_gives_its_cities_taxes() {
    let game = game("", 0, |_| {});
    assert_eq!(game.map.city_at(REDHOLD).unwrap().income, 6);
    assert_eq!(game.map.city_at(BLUEMOOR).unwrap().income, 10);
    let terrain = format!("20\n20\n{}\n", vec!["g".repeat(20); 20].join("\n").replacen('g', "c", 1));
    let err = scenario::parse(&terrain, None, Some("city 0 0\nname A\nincome lots\n")).unwrap_err();
    assert_eq!(err.to_string(), "cities.txt line 3: income must be a number, found \"lots\"");
}

// The screen.

#[test]
fn the_menu_bar_shows_the_treasury_and_a_city_its_taxes() {
    let mut s =
        common::with_game(&assets(), Size::new(1024, 768), 96, game("garrison soldiers\n", 30, |_| {})).unwrap();
    let says = |s: &Session, name: &str| s.screen.ui.text(widget(s, name)).unwrap_or("").to_string();
    // At a glance, at the right end of the menu bar: the gold, what comes in
    // a turn, what goes out.
    let treasury = |s: &Session| [says(s, "gold"), says(s, "gold_in"), says(s, "gold_out")];
    assert_eq!(treasury(&s), ["30", "+6", "-2"]);
    assert!(s.next_turn());
    assert_eq!(treasury(&s)[0], "34");
    let (bar, gold) = (s.screen.area(widget(&s, "menu_bar")), s.screen.area(widget(&s, "gold_out")));
    assert!(gold.y >= bar.y && gold.bottom() <= bar.bottom() && gold.right() > bar.right() - 120, "{gold:?} {bar:?}");
    // And last, by a city's picture, the side's cities of all the map's
    // (the owner's word, 2026-11-24).
    let cities = s.screen.area(widget(&s, "cities_held"));
    assert!(cities.x > gold.right() && cities.right() > bar.right() - 40, "{cities:?} {bar:?}");
    let icon = s.screen.area(widget(&s, "city_icon"));
    assert!(icon.x > gold.right() && icon.right() <= cities.x && icon.h > 8, "{icon:?}");
    assert_eq!(says(&s, "cities_held"), "1/2");
    let bluemoor = s.screen.game.map.city_at(Hex::new(5, 15)).unwrap().id;
    s.screen.game.map.city_mut(bluemoor).unwrap().side = Side::Red;
    s.frame(0);
    assert_eq!(says(&s, "cities_held"), "2/2", "it follows the cities' holders");
    // The city's window says what it brings in.
    let r = s.screen.area(widget(&s, hexlords::city_windows::PANEL_OPEN_ID));
    click(&mut s, Point::new(r.x + r.w / 2, r.y + r.h / 2));
    let window = widget(&s, hexlords::city_windows::CITY_WINDOW_ID);
    let mut texts = Vec::new();
    let mut stack = vec![window];
    while let Some(id) = stack.pop() {
        texts.extend(s.screen.ui.text(id).map(str::to_string));
        stack.extend(s.screen.ui.children(id).iter().copied());
    }
    assert!(texts.iter().any(|t| t == "Taxes: 6 gold a turn"), "{texts:?}");
}

#[test]
fn in_rounds_too_units_that_cannot_be_paid_leave() {
    // As in turns: upkeep 8 against taxes of 6 and no gold, 2 short.
    let mut game = game("garrison soldiers 2\ngarrison levy\ngarrison captain\n", 0, |s| {
        s.mode = hexlords::setup::Mode::Rounds;
    });
    assert!(game.rounds());
    next_round(&mut game);
    assert_eq!(game.turn(), 2, "the round played out");
    let names: Vec<String> =
        game.map.units_at(REDHOLD).iter().map(|id| game.map.units().get(*id).unwrap().name.clone()).collect();
    assert_eq!(names, ["Soldiers", "Levy", "Captain"]);
    assert_eq!(game.gold(Side::Red), 0);
    let lines = info::log(&game, Side::Red, 0);
    let all: Vec<&String> = lines.iter().flat_map(|(_, entries)| entries).flat_map(|e| &e.lines).collect();
    assert!(all.iter().any(|l| l.starts_with("Unpaid, your Soldiers left you")), "{all:?}");
}
