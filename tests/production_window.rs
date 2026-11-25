//! The production panel (E, the Cities menu's Production..., O on one's own
//! city): over the map, a side's cities, what they build and where it goes,
//! what the next rounds bring, and its stacks, in one place; the city
//! chosen in full beside them.

mod common;

use common::{assets, choose, push_widget, type_char, widget};
use hexlords::city_windows::{self, BUILD_NOTHING_ID, CITY_DEFEND_ID, CITY_NO_RALLY_ID};
use hexlords::disband_window::DISBAND_YES_ID;
use hexlords::game::{CityState, Game};
use hexlords::info;
use hexlords::orders::{Order, RallyPoint};
use hexlords::production_window::{
    Area, Filter, PROD_DISBAND_ID, PROD_IDLE_ID, PROD_SELECT_ID, PROD_STACKS_ID, PROD_STOP_ALL_ID, PROD_STOP_ALL_NO_ID,
    PROD_STOP_ALL_YES_ID, PRODUCTION_CLOSE_ID, PRODUCTION_WINDOW_ID, city_row_id, stack_row_id,
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

/// Every text in the panel.
fn texts(s: &Session) -> Vec<String> {
    common::descendants(s, PRODUCTION_WINDOW_ID)
        .iter()
        .filter_map(|id| s.screen.ui.text(*id))
        .map(str::to_string)
        .collect()
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
fn every_city_is_a_row_the_idle_and_waiting_first_with_what_comes() {
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
    // No gold block (the owner's word, 2026-11-24: "Remove the Gold info,
    // the econ stuff is busy"): the header counts the cities, and warns
    // only when the upkeep will not be paid (it will be, here).
    let says = |s: &Session, name: &str| s.screen.ui.text(widget(s, name)).map(str::to_string);
    assert!(texts(&s).iter().any(|t| t == "3 cities: 1 idle, 1 waiting, 1 building"), "{:?}", texts(&s));
    assert_eq!(window.warning(), None);
    assert_eq!(says(&s, PROD_IDLE_ID).as_deref(), Some("1 idle"));
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

/// The gold will not pay the upkeep: one line under the header says from
/// when (the owner's word, 2026-11-24: one warning line, nothing else).
#[test]
fn a_line_warns_when_the_gold_will_not_pay_the_upkeep() {
    let mut s = session();
    let levy = s.screen.game.map.units().kind_named("knights").unwrap();
    for _ in 0..4 {
        let id = s.screen.game.map.new_unit_of(Side::Red, levy).unwrap();
        s.screen.game.map.place_unit(id, Hex::new(8, 8)).unwrap();
    }
    open(&mut s);
    let warning = s.production().unwrap().warning().map(str::to_string);
    let round = s.screen.game.forecast(Side::Red, 20).short_at.unwrap();
    assert_eq!(warning, Some(format!("From round {round} the gold will not pay the upkeep: units will leave.")));
    assert!(texts(&s).contains(&warning.unwrap()));
}

/// The owner's word, 2026-11-24: "Merge the production and city
/// dialogs ... When a city is selected the city section changes to it". A
/// row, or Up and Down, chooses a city, and the city is in full in the
/// panel's right column.
#[test]
fn a_city_row_or_up_and_down_choose_it_and_it_is_in_full_beside() {
    let mut s = session();
    // The city the map shows (Alder, red's first) is chosen as it opens.
    open(&mut s);
    let (alder, birch, cobb) = (city(&s.screen.game, ALDER), city(&s.screen.game, BIRCH), city(&s.screen.game, COBB));
    assert_eq!(s.production().unwrap().chosen_city(), Some(alder));
    assert!(s.city_window().is_some_and(|w| w.embedded() && w.city() == alder));
    push_row(&mut s, BIRCH);
    assert_eq!(s.production().unwrap().chosen_city(), Some(birch));
    assert_eq!(s.city_window().map(|w| w.city()), Some(birch), "its view, in the panel");
    // Down: the next row (Cobb); Up: back. The cursor does not move.
    let cursor = s.screen.game.map.cursor;
    common::press(&mut s, SpecialKey::Down);
    assert_eq!(s.city_window().map(|w| w.city()), Some(cobb));
    common::press(&mut s, SpecialKey::Up);
    assert_eq!(s.city_window().map(|w| w.city()), Some(birch));
    assert_eq!(s.screen.game.map.cursor, cursor, "the arrows are the panel's while it is open");
    // Closed, the city's view goes with it.
    type_char(&mut s, 'e');
    assert!(s.production().is_none() && s.city_window().is_none());
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
    assert!(s.production().unwrap().coming().iter().all(|(_, l)| !l.contains("Soldiers")), "nothing comes");
}

#[test]
fn out_of_its_turn_it_shows_and_changes_nothing() {
    let mut s = session();
    s.end_turn();
    assert_eq!(s.screen.game.player(), Side::Blue, "blue plays (it is the computer's turn)");
    open(&mut s);
    push_row(&mut s, ALDER);
    for button in [BUILD_NOTHING_ID, CITY_DEFEND_ID, PROD_STOP_ALL_ID] {
        assert!(disabled(&s, button), "{button}");
    }
    // A click on a greyed button does nothing.
    let before = s.screen.game.map.city_at(ALDER).unwrap().clone();
    push_widget(&mut s, BUILD_NOTHING_ID);
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

/// It covers the map's rectangle, at 1x and at 2x, and after the window
/// changes size; its parts are inside it.
#[test]
fn it_covers_the_map_on_a_small_screen_and_a_large_one() {
    for (w, h, dpi) in [(1024, 768, 96), (2048, 1536, 192)] {
        let mut game = game(|_| {});
        game.apply(Order::Build { city: city(&game, ALDER), what: Some(0) }).unwrap();
        let mut s = session_at(game, w, h, dpi);
        open(&mut s);
        push_row(&mut s, ALDER);
        push_widget(&mut s, PROD_STOP_ALL_ID);
        s.frame(0);
        let id = widget(&s, PRODUCTION_WINDOW_ID);
        let r = Rect::from_origin_size(s.screen.ui.to_root(id, Point::ZERO), s.screen.ui.rect(id).size());
        assert_eq!(r, s.screen.map_rect(), "at {w}");
        for name in [PROD_STACKS_ID, PROD_STOP_ALL_YES_ID, PRODUCTION_CLOSE_ID, &city_row_id(2), BUILD_NOTHING_ID] {
            let b = s.screen.area(widget(&s, name));
            assert!(
                b.w > 0 && r.x <= b.x && b.right() <= r.right() && b.bottom() <= r.bottom(),
                "{name} at {w}: {b:?} in {r:?}"
            );
        }
        // The window changes size: it follows the map.
        s.resize(Size::new(w * 5 / 4, h));
        s.frame(0);
        let r = Rect::from_origin_size(s.screen.ui.to_root(id, Point::ZERO), s.screen.ui.rect(id).size());
        assert_eq!(r, s.screen.map_rect(), "after a resize, at {w}");
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

/// The owner's word, 2026-11-24: "Use the game minimap ... (when the prod
/// box is open the things that went on that map go on the game map now)".
/// While it is open the minimap rings the cities at no work in orange and
/// the chosen one in gold, with a line to where it sends its units; closed,
/// nothing.
#[test]
fn the_game_minimap_marks_what_production_marked() {
    use hexlords::render::{BATTLE_BORDER, CHOSEN_COLOR, MiniMarks};
    use hexlords::screen::Scene;
    let mut s = session();
    assert_eq!(s.minimap_marks(), MiniMarks::default());
    let point = RallyPoint { at: Hex::new(10, 10), defend: false };
    s.screen.game.apply(Order::RallyPoint { n: 2, point: Some(point) }).unwrap();
    let alder = city(&s.screen.game, ALDER);
    s.screen.game.apply(Order::Build { city: alder, what: Some(1) }).unwrap();
    s.screen.game.apply(Order::Rally { city: alder, to: Sends::Point { n: 2, bless: false } }).unwrap();
    open(&mut s);
    push_row(&mut s, ALDER);
    let marks = s.minimap_marks();
    for idle in [BIRCH, COBB] {
        assert!(marks.rings.contains(&(idle, BATTLE_BORDER)), "{idle}: {marks:?}");
    }
    assert!(marks.rings.contains(&(ALDER, CHOSEN_COLOR)) && !marks.rings.contains(&(ALDER, BATTLE_BORDER)));
    assert_eq!(marks.line, Some((ALDER, Hex::new(10, 10))));

    common::press(&mut s, SpecialKey::Esc);
    assert_eq!(s.minimap_marks(), MiniMarks::default(), "closed: nothing");
    // The window's scene is the shell, which hands them on (it did not at
    // first: nothing showed).
    open(&mut s);
    push_row(&mut s, ALDER);
    let shell = hexlords::shell::Shell::playing(&assets(), s);
    assert_eq!(shell.minimap_marks(), marks);
}

/// The cities' filters (the owner's word, 2026-11-24, "The filter row is a
/// good idea"): All, idle, waiting, building, by a click or a key.
#[test]
fn the_filters_show_the_cities_idle_waiting_or_building() {
    let mut s = session();
    open(&mut s);
    let ids = |hexes: &[Hex], s: &Session| hexes.iter().map(|h| city(&s.screen.game, *h)).collect::<Vec<_>>();
    push_widget(&mut s, PROD_IDLE_ID);
    assert_eq!(s.production().unwrap().cities(), ids(&[BIRCH], &s));
    assert_eq!(s.production().unwrap().chosen_city(), Some(city(&s.screen.game, BIRCH)), "the first, chosen");
    type_char(&mut s, 'w');
    assert_eq!(s.production().unwrap().cities(), ids(&[COBB], &s));
    type_char(&mut s, 'b');
    assert_eq!(s.production().unwrap().cities(), ids(&[ALDER], &s));
    type_char(&mut s, 'a');
    assert_eq!(s.production().unwrap().cities(), ids(&[BIRCH, COBB, ALDER], &s));
    assert_eq!(s.production().unwrap().filter(), Filter::All);
    assert!(s.city_list().is_none(), "A is the filter's while it is open, not All cities");
}

/// O (or the lists' Open) on one's own city opens the panel on it; on
/// another side's it does nothing (the sidebar says what is seen of it).
#[test]
fn o_opens_the_panel_on_ones_own_city_and_not_on_anothers() {
    let mut s = session();
    let cobb = city(&s.screen.game, COBB);
    let look = |s: &mut Session, hex: Hex| {
        s.screen.view.center_on(hex);
        let p = common::pixel_of(s, hex);
        common::click(s, p);
        assert_eq!(s.shown(), hex);
    };
    look(&mut s, COBB);
    type_char(&mut s, 'o');
    assert_eq!(s.production().and_then(|w| w.chosen_city()), Some(cobb));
    assert_eq!(s.city_window().map(|w| (w.city(), w.embedded())), Some((cobb, true)));
    type_char(&mut s, 'e');
    look(&mut s, Hex::new(15, 15));
    type_char(&mut s, 'o');
    assert!(s.production().is_none() && s.city_window().is_none(), "Bluemoor is blue's");
}

/// What the city builds and where its units go are set in the city beside
/// the list (its window's own buttons: the owner's word, 2026-11-24, the
/// panel's Stop, Keep, Defend, To a city and second row of rally points
/// went as "redundant").
#[test]
fn nothing_keep_defend_and_a_rally_point_in_the_city_beside() {
    let mut s = session();
    let point = RallyPoint { at: Hex::new(10, 10), defend: false };
    s.screen.game.apply(Order::RallyPoint { n: 2, point: Some(point) }).unwrap();
    open(&mut s);
    push_row(&mut s, ALDER);
    push_widget(&mut s, CITY_DEFEND_ID);
    assert_eq!(sends(&s, ALDER), Sends::Defend);
    push_widget(&mut s, CITY_NO_RALLY_ID);
    assert_eq!(sends(&s, ALDER), Sends::Stay);
    // A button for each rally point that is set, and none for the others.
    assert!(shown(&s, &city_windows::point_id(2)) && !shown(&s, &city_windows::point_id(0)));
    push_widget(&mut s, &city_windows::point_id(2));
    assert_eq!(sends(&s, ALDER), Sends::Point { n: 2, bless: false });
    let alder = city(&s.screen.game, ALDER);
    let window = s.production().unwrap();
    let line = &window.city_lines()[window.cities().iter().position(|c| *c == alder).unwrap()];
    assert!(line.ends_with("; to rally point 2"), "{line}");
    // Nothing: it builds nothing, on purpose; its row says so, and goes last.
    push_widget(&mut s, BUILD_NOTHING_ID);
    assert_eq!(state(&s, ALDER), CityState::Resting);
    let window = s.production().unwrap();
    assert_eq!(window.city_lines().last().map(String::as_str), Some("Alder: stopped"));
    assert_eq!(window.chosen_city(), Some(alder), "still chosen");
}

fn queue(s: &Session, hex: Hex) -> hexlords::state::Queue {
    s.screen.game.map.city_at(hex).unwrap().queue.clone()
}

fn item(what: usize, count: u32, sends: Sends) -> hexlords::state::QueueItem {
    hexlords::state::QueueItem { what, count, sends }
}

/// The owner's word, 2026-11-24: "In the city section the queue will handle
/// production. Pressing a unit button puts it in the queue. If it was empty
/// the defaults are to repeat and keep here simulating the old no queue
/// behaviour." And "The unit buttons should be like the stack buttons".
#[test]
fn a_unit_box_puts_one_in_the_queue_which_repeats_and_keeps_them_at_first() {
    use hexlords::state::QueueEnd;
    let mut s = session();
    open(&mut s);
    // Birch builds nothing: its box makes a queue of one that repeats for
    // ever, its units kept in the city.
    push_row(&mut s, BIRCH);
    let levy = city_windows::build_id(0);
    assert_eq!(s.screen.ui.skin(widget(&s, &levy)), turbo_ferret::TbId::new("UnitSlot"));
    assert_eq!(s.screen.ui.text(widget(&s, &format!("{levy}_text"))), Some("3g 1t"), "its price and turns");
    push_widget(&mut s, &levy);
    assert_eq!(queue(&s, BIRCH).items, [item(0, 1, Sends::Stay)]);
    assert_eq!(queue(&s, BIRCH).end, QueueEnd::Repeat { from: 0, times: None });
    assert_eq!(state(&s, BIRCH), CityState::Building);
    // Alder builds soldiers with no queue: they are its first item, and
    // levies come after them, going where the soldiers go.
    push_row(&mut s, ALDER);
    push_widget(&mut s, CITY_DEFEND_ID);
    push_widget(&mut s, &city_windows::build_id(1));
    assert_eq!(queue(&s, ALDER).items, [item(0, 1, Sends::Defend), item(1, 1, Sends::Defend)]);
    assert_eq!(queue(&s, ALDER).end, QueueEnd::Repeat { from: 0, times: None });
    // The last item out: it builds nothing, and rests.
    push_row(&mut s, BIRCH);
    push_widget(&mut s, &city_windows::queue_remove_id(0));
    assert!(queue(&s, BIRCH).is_empty());
    assert_eq!(state(&s, BIRCH), CityState::Resting);
}

/// The owner's word, 2026-11-24: "the user should be able to use the
/// keyboard to move between units and update the discription area". Tab
/// goes from the cities to the queue to the units; the arrows move there;
/// Enter adds; - and + change the chosen item, Delete takes it out; a
/// digit sends it to that rally point, K keeps it, F defends.
#[test]
fn the_keyboard_works_the_panel() {
    let mut s = session();
    let point = RallyPoint { at: Hex::new(10, 10), defend: false };
    s.screen.game.apply(Order::RallyPoint { n: 3, point: Some(point) }).unwrap();
    open(&mut s);
    push_row(&mut s, BIRCH);
    let focus = |s: &Session| s.production().unwrap().focus();
    assert_eq!(focus(&s), Area::Cities);
    // Tab twice: the units, the first in focus (it looks so, and all about
    // it shows).
    common::press(&mut s, SpecialKey::Tab);
    assert_eq!(focus(&s), Area::Queue);
    common::press(&mut s, SpecialKey::Tab);
    assert_eq!(focus(&s), Area::Units);
    assert_eq!(s.city_window().unwrap().focused_unit(), Some(0));
    s.frame(0);
    let levy = widget(&s, &city_windows::build_id(0));
    assert_eq!(s.screen.ui.skin(levy), turbo_ferret::TbId::new("UnitComing"));
    assert!(
        s.city_window().unwrap().details()[0].starts_with("Levy: strength 1"),
        "{:?}",
        s.city_window().unwrap().details()
    );
    // Enter twice: two levies.
    common::press(&mut s, SpecialKey::Enter);
    common::press(&mut s, SpecialKey::Enter);
    assert_eq!(queue(&s, BIRCH).items, [item(0, 1, Sends::Stay), item(0, 1, Sends::Stay)]);
    // Shift+Tab: the queue; Up to the first item; + twice, - once.
    s.screen.ui.key(None, SpecialKey::Tab, turbo_ferret::Modifiers::SHIFT, true);
    s.screen.ui.key(None, SpecialKey::Tab, turbo_ferret::Modifiers::SHIFT, false);
    s.pump();
    assert_eq!(focus(&s), Area::Queue);
    common::press(&mut s, SpecialKey::Up);
    type_char(&mut s, '+');
    type_char(&mut s, '+');
    type_char(&mut s, '-');
    assert_eq!(queue(&s, BIRCH).items[0].count, 2);
    // 3: to rally point 3; F: defend; K: keep; 5, not set: says so.
    type_char(&mut s, '3');
    assert_eq!(queue(&s, BIRCH).items[0].sends, Sends::Point { n: 3, bless: false });
    type_char(&mut s, 'f');
    assert_eq!(queue(&s, BIRCH).items[0].sends, Sends::Defend);
    type_char(&mut s, 'k');
    assert_eq!(queue(&s, BIRCH).items[0].sends, Sends::Stay);
    type_char(&mut s, '5');
    assert_eq!(queue(&s, BIRCH).items[0].sends, Sends::Stay);
    // Delete: out.
    common::press(&mut s, SpecialKey::Delete);
    assert_eq!(queue(&s, BIRCH).items, [item(0, 1, Sends::Stay)]);
    // None of it reached the map: the cursor and the zoom are as they were.
    assert_eq!(s.screen.game.map.cursor, ALDER);
}
