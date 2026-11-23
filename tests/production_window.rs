//! The production window (E, or the Cities menu's Production...): a side's
//! cities, what they build and where it goes, what the next rounds bring,
//! the gold, and its stacks, in one place, with what can be done from there.

mod common;

use common::{assets, choose, push_widget, type_char, widget};
use hexlords::disband_window::DISBAND_YES_ID;
use hexlords::game::{CityState, Game};
use hexlords::info;
use hexlords::orders::{Order, RallyPoint};
use hexlords::production_window::{
    PROD_DEFEND_ID, PROD_DISBAND_ID, PROD_OPEN_ID, PROD_SELECT_ID, PROD_STACKS_ID, PROD_STAY_ID, PROD_STOP_ALL_ID,
    PROD_STOP_ALL_NO_ID, PROD_STOP_ALL_YES_ID, PROD_STOP_ID, PROD_TO_CITY_ID, PRODUCTION_WINDOW_ID, city_row_id,
    point_id, stack_row_id,
};
use hexlords::session::{MENU_CITIES_ID, PRODUCTION_ID, Session};
use hexlords::setup::Setup;
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{CityId, Hex, Sends, Side, UnitId};
use turbo_ferret::{Point, Rect, Size, SpecialKey, Visibility};

const ALDER: Hex = Hex::new(5, 5);
const BIRCH: Hex = Hex::new(5, 12);
const COBB: Hex = Hex::new(15, 5);

/// Red's Alder (soldiers, levies; a garrison of three levies and a captain),
/// Birch (levies) and Cobb (knights at 99 gold); blue's Bluemoor, far off.
/// Red a person with 20 gold, blue passive unless `setup` says otherwise.
fn game(setup: impl FnOnce(&mut Setup)) -> Game {
    let mut rows = vec!["g".repeat(20); 20];
    rows[5] = "gggggcggggggcggggggg".to_string();
    rows[15] = "gggggcgggggggggcgggg".to_string();
    let terrain = format!("20\n20\n{}\n", rows.join("\n"));
    let units = "unit soldiers\nname Soldiers\nstrength 3\nmoves 12\nupkeep 2\nprice 10\nenter grass 2, city 1\n\
                 unit levy\nname Levy\nstrength 1\nmoves 12\nupkeep 1\nprice 3\nenter grass 2, city 1\n\
                 unit knights\nname Knights\nstrength 6\nmoves 12\nupkeep 5\nprice 30\nenter grass 2, city 1\n\
                 unit captain\nname Captain\nstrength 2\nmoves 12\nupkeep 3\ncommander 1\nenter grass 2, city 1\n";
    let cities = "city 5 5\nname Alder\nside red\nincome 10\nproduce soldiers 1\nproduce levy 1\n\
                  garrison levy 3\ngarrison captain\n\
                  city 5 12\nname Birch\nside red\nincome 5\nproduce levy 1\n\
                  city 15 5\nname Cobb\nside red\nincome 5\nproduce knights 1 99\n\
                  city 15 15\nname Bluemoor\nside blue\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    let mut s = Setup::solo(&game.map, Some(Side::Red), "passive");
    s.gold = 20;
    setup(&mut s);
    game.with_setup(s)
}

fn city(game: &Game, hex: Hex) -> CityId {
    game.map.city_at(hex).unwrap().id
}

fn session_at(game: Game, w: i32, h: i32, dpi: i32) -> Session {
    common::with_game(&assets(), Size::new(w, h), dpi, game).unwrap()
}

/// Alder building soldiers, Cobb knights (done after a round, and waiting
/// for the gold), Birch nothing; the session at red's next turn.
fn session() -> Session {
    let mut game = game(|_| {});
    game.apply(Order::Build { city: city(&game, ALDER), what: Some(0) }).unwrap();
    game.apply(Order::Build { city: city(&game, COBB), what: Some(0) }).unwrap();
    game.end_turn();
    game.end_turn();
    session_at(game, 1024, 768, 96)
}

fn open(s: &mut Session) {
    if s.production().is_none() {
        type_char(s, 'e');
    }
    assert!(s.production().is_some());
}

/// The row of `hex`'s city, by the window's order.
fn row_of(s: &Session, hex: Hex) -> String {
    let id = city(&s.screen.game, hex);
    let i = s.production().unwrap().cities().iter().position(|c| *c == id).expect("a row");
    city_row_id(i)
}

/// Click the row of `hex`'s city.
fn push_row(s: &mut Session, hex: Hex) {
    let row = row_of(s, hex);
    push_widget(s, &row);
}

fn shown(s: &Session, name: &str) -> bool {
    let Some(start) = s.screen.ui.find(s.screen.ui.root(), turbo_ferret::TbId::new(name)) else { return false };
    let mut id = Some(start);
    while let Some(w) = id {
        if s.screen.ui.node(w).is_none_or(|n| n.visibility() != Visibility::Visible) {
            return false;
        }
        id = s.screen.ui.parent(w);
    }
    true
}

fn disabled(s: &Session, name: &str) -> bool {
    s.screen.ui.node(widget(s, name)).is_some_and(|n| n.is_disabled())
}

fn sends(s: &Session, hex: Hex) -> Sends {
    s.screen.game.map.city_at(hex).unwrap().sends.clone()
}

fn state(s: &Session, hex: Hex) -> CityState {
    CityState::of(s.screen.game.map.city_at(hex).unwrap())
}

#[test]
fn e_or_the_cities_menu_opens_it_and_esc_closes_it() {
    let mut s = session();
    assert!(s.production().is_none());
    type_char(&mut s, 'e');
    assert!(s.production().is_some_and(|w| w.side() == Side::Red));
    type_char(&mut s, 'E');
    assert!(s.production().is_none(), "the key again closes it");
    choose(&mut s, MENU_CITIES_ID, PRODUCTION_ID);
    assert!(s.production().is_some());
    common::press(&mut s, SpecialKey::Esc);
    assert!(s.production().is_none());
    // The keyboard stays with the map.
    type_char(&mut s, 'e');
    assert_eq!(s.screen.ui.focused(), Some(s.screen.map_widget()));
}

#[test]
fn every_city_is_a_row_the_idle_and_waiting_first_with_the_gold_and_what_comes() {
    let mut s = session();
    open(&mut s);
    let window = s.production().unwrap();
    let map = &s.screen.game.map;
    let order: Vec<CityId> = [BIRCH, COBB, ALDER].iter().map(|h| city(&s.screen.game, *h)).collect();
    assert_eq!(window.cities(), order, "idle, then waiting, then at work");
    let rows: Vec<String> = order.iter().map(|id| info::production_row(map, map.cities().get(*id).unwrap())).collect();
    assert_eq!(window.city_lines(), rows);
    assert_eq!(rows[0], "Birch: IDLE, building nothing");
    assert!(rows[1].starts_with("Cobb: Knights done, they wait for 99 gold"), "{}", rows[1]);
    assert!(rows[2].starts_with("Alder: Soldiers 0/1; stay"), "{}", rows[2]);
    let gold = s.screen.game.gold(Side::Red);
    let money = window.economy_lines();
    assert_eq!(money[0], format!("Gold {gold}"));
    assert!(money.iter().any(|l| l == "Cities: 1 idle, 1 waiting"), "{money:?}");
    // What comes: a round's heading, then Alder's soldiers and Cobb waiting.
    let forecast = s.screen.game.forecast(Side::Red, 20);
    assert_eq!(window.coming(), info::coming_lines(map, &forecast, 5));
    let coming = window.coming();
    assert!(coming[0].0 && coming[0].1.starts_with(&format!("Round {}", forecast.days[0].round)));
    assert!(coming.iter().any(|(_, l)| l == "Alder: Soldiers, 10 gold; stay"), "{coming:?}");
    assert!(coming.iter().any(|(_, l)| l == "Cobb: waits for gold"), "{coming:?}");
    // The rows are on the screen, a button each.
    for i in 0..3 {
        assert!(shown(&s, &city_row_id(i)), "{i}");
    }
}

#[test]
fn a_city_row_chooses_it_and_open_shows_its_window() {
    let mut s = session();
    // The city the map shows (Alder, red's first) is chosen as it opens.
    open(&mut s);
    assert_eq!(s.production().unwrap().chosen_city(), Some(city(&s.screen.game, ALDER)));
    let birch = city(&s.screen.game, BIRCH);
    push_row(&mut s, BIRCH);
    assert_eq!(s.production().unwrap().chosen_city(), Some(birch));
    assert!(shown(&s, PROD_OPEN_ID) && shown(&s, PROD_STOP_ID));
    push_widget(&mut s, PROD_OPEN_ID);
    assert_eq!(s.city_window().map(|w| w.city()), Some(birch), "its window, for what it builds and its queue");
    assert!(s.production().is_some(), "under it, still");
}

#[test]
fn stop_keep_defend_and_a_rally_point_from_the_chosen_city() {
    let mut s = session();
    let point = RallyPoint { at: Hex::new(10, 10), defend: false };
    s.screen.game.apply(Order::RallyPoint { n: 2, point: Some(point) }).unwrap();
    open(&mut s);
    push_row(&mut s, ALDER);
    push_widget(&mut s, PROD_DEFEND_ID);
    assert_eq!(sends(&s, ALDER), Sends::Defend);
    push_widget(&mut s, PROD_STAY_ID);
    assert_eq!(sends(&s, ALDER), Sends::Stay);
    // A button for each rally point that is set, and none for the others.
    assert!(shown(&s, &point_id(2)) && !shown(&s, &point_id(0)));
    push_widget(&mut s, &point_id(2));
    assert_eq!(sends(&s, ALDER), Sends::Point { n: 2, bless: false });
    let line = &s.production().unwrap().city_lines()[2];
    assert!(line.ends_with("; to rally point 2"), "{line}");
    // Stop: it builds nothing, on purpose; its row says so, and goes last.
    push_widget(&mut s, PROD_STOP_ID);
    assert_eq!(state(&s, ALDER), CityState::Resting);
    let window = s.production().unwrap();
    assert_eq!(window.city_lines().last().map(String::as_str), Some("Alder: stopped"));
    assert_eq!(window.chosen_city(), Some(city(&s.screen.game, ALDER)), "still chosen");
}

#[test]
fn to_a_city_then_its_row_sends_the_new_units_there() {
    let mut s = session();
    open(&mut s);
    let alder = city(&s.screen.game, ALDER);
    push_row(&mut s, ALDER);
    push_widget(&mut s, PROD_TO_CITY_ID);
    assert_eq!(s.production().unwrap().sending(), Some(alder));
    assert!(shown(&s, PROD_TO_CITY_ID));
    // The button again: never mind.
    push_widget(&mut s, PROD_TO_CITY_ID);
    assert_eq!(s.production().unwrap().sending(), None);
    assert_eq!(sends(&s, ALDER), Sends::Stay);
    // Again, and Cobb's row: there.
    push_widget(&mut s, PROD_TO_CITY_ID);
    push_row(&mut s, COBB);
    assert_eq!(sends(&s, ALDER), Sends::Route(vec![COBB]));
    let window = s.production().unwrap();
    assert_eq!((window.sending(), window.chosen_city()), (None, Some(alder)), "Alder stays chosen");
    let line = &window.city_lines()[window.cities().iter().position(|c| *c == alder).unwrap()];
    assert!(line.ends_with("; to Cobb"), "{line}");
}

#[test]
fn stop_all_asks_first_then_stops_every_city_at_work() {
    let mut s = session();
    open(&mut s);
    assert!(!shown(&s, PROD_STOP_ALL_YES_ID));
    push_widget(&mut s, PROD_STOP_ALL_ID);
    assert!(s.production().unwrap().asking() && shown(&s, PROD_STOP_ALL_YES_ID));
    push_widget(&mut s, PROD_STOP_ALL_NO_ID);
    assert!(!s.production().unwrap().asking());
    assert_eq!((state(&s, ALDER), state(&s, COBB)), (CityState::Building, CityState::Waiting), "nothing changed");
    push_widget(&mut s, PROD_STOP_ALL_ID);
    push_widget(&mut s, PROD_STOP_ALL_YES_ID);
    assert_eq!((state(&s, ALDER), state(&s, COBB)), (CityState::Resting, CityState::Resting));
    assert_eq!(state(&s, BIRCH), CityState::Idle, "it had nothing to stop");
    assert!(s.production().unwrap().economy_lines().iter().any(|l| l.contains("nothing built")));
}

#[test]
fn out_of_its_turn_it_shows_and_changes_nothing() {
    let mut s = session();
    s.end_turn();
    assert_eq!(s.screen.game.player(), Side::Blue, "blue plays (it is the computer's turn)");
    open(&mut s);
    push_row(&mut s, ALDER);
    for button in [PROD_STOP_ID, PROD_DEFEND_ID, PROD_TO_CITY_ID, PROD_STOP_ALL_ID] {
        assert!(disabled(&s, button), "{button}");
    }
    assert!(!disabled(&s, PROD_OPEN_ID), "looking is all right");
    // A click on a greyed button does nothing.
    let before = s.screen.game.map.city_at(ALDER).unwrap().clone();
    push_widget(&mut s, PROD_STOP_ID);
    assert_eq!(s.screen.game.map.city_at(ALDER).unwrap(), &before);
}

#[test]
fn the_stacks_dearest_first_defenders_apart_to_select_or_disband() {
    let mut s = session();
    let alder: Vec<UnitId> = s.screen.game.map.units_at(ALDER).to_vec();
    // Two of Alder's levies defend it; the other levy, the captain and the
    // soldiers it built in round 2 not.
    s.screen.game.apply(Order::Defend { units: alder[..2].to_vec(), on: true }).unwrap();
    open(&mut s);
    assert!(!s.production().unwrap().stacks_shown() && !shown(&s, &stack_row_id(0)), "hidden at first");
    push_widget(&mut s, PROD_STACKS_ID);
    let window = s.production().unwrap();
    assert!(window.stacks_shown());
    let rows = window.stacks();
    assert_eq!(rows.len(), 2, "one hex, two stacks: {rows:?}");
    assert_eq!((rows[0].defending, rows[0].upkeep), (false, 1 + 3 + 2), "a levy, the captain, soldiers: dearest");
    assert_eq!((rows[1].defending, rows[1].upkeep), (true, 2));
    assert_eq!(window.stack_lines()[1], "Alder: 2 units, upkeep 2, defending");
    // Select: those units, under the map, and no others.
    push_widget(&mut s, &stack_row_id(0));
    push_widget(&mut s, PROD_SELECT_ID);
    let mut chosen = s.screen.game.selection().to_vec();
    chosen.sort_by_key(|id| (id.index, id.generation));
    let mut others = alder[2..].to_vec();
    others.sort_by_key(|id| (id.index, id.generation));
    assert_eq!(chosen, others);
    // Disband...: the window that asks, about them; Yes, and they are gone.
    push_widget(&mut s, &stack_row_id(1));
    push_widget(&mut s, PROD_DISBAND_ID);
    assert_eq!(s.disband().map(|w| w.units.clone()), Some(alder[..2].to_vec()));
    push_widget(&mut s, DISBAND_YES_ID);
    assert!(alder[..2].iter().all(|id| s.screen.game.map.units().get(*id).is_none()));
    assert_eq!(s.production().unwrap().stacks().len(), 1, "its rows follow");
}

#[test]
fn it_fits_the_map_on_a_small_screen_and_a_large_one() {
    for (w, h, dpi) in [(1024, 768, 96), (2048, 1536, 192)] {
        let mut game = game(|_| {});
        game.apply(Order::Build { city: city(&game, ALDER), what: Some(0) }).unwrap();
        let mut s = session_at(game, w, h, dpi);
        open(&mut s);
        push_widget(&mut s, PROD_STACKS_ID);
        push_row(&mut s, ALDER);
        // (The tallest it gets: the stacks shown, Stop all asking.)
        push_widget(&mut s, PROD_STOP_ALL_ID);
        s.frame(0);
        let id = widget(&s, PRODUCTION_WINDOW_ID);
        let r = Rect::from_origin_size(s.screen.ui.to_root(id, Point::ZERO), s.screen.ui.rect(id).size());
        let map = s.screen.map_rect();
        assert!(
            r.x >= map.x && r.y >= map.y && r.right() <= map.right() && r.bottom() <= map.bottom(),
            "{r:?} {map:?}"
        );
        for name in [PROD_OPEN_ID, PROD_TO_CITY_ID, PROD_STOP_ALL_ID, PROD_STOP_ALL_YES_ID] {
            let b = s.screen.area(widget(&s, name));
            assert!(b.w > 0 && r.x <= b.x && b.right() <= r.right(), "{name} at {w}: {b:?} in {r:?}");
        }
    }
}

#[test]
fn in_hotseat_the_next_persons_cover_closes_it() {
    let mut s = session_at(game(|s| s.set_players("blue=person").unwrap()), 1024, 768, 96);
    if s.covered() {
        push_widget(&mut s, hexlords::session::BEGIN_TURN_ID);
    }
    open(&mut s);
    s.end_turn();
    assert!(s.covered(), "the map is covered for blue");
    assert!(s.production().is_none(), "red's gold is not blue's to see");
}
