//! The windows about cities: the list of every city and of the player's, with
//! a minimap that shows where the chosen one is; a city in full, where its
//! holder sets what it builds; and the turns that build it. No window and no
//! GPU: a `Session` on the real assets, fed what the window would send.

mod common;

use common::{assets, choose, click, descendants, pixel_of, press, widget};
use hexgame::city_windows::CityList;
use hexgame::city_windows::{
    BUILD_NOTHING_ID, CITY_CLOSE_ID, CITY_RALLY_ID, CITY_WINDOW_ID, LIST_CLOSE_ID, LIST_DETAILS_ID, LIST_GOTO_ID,
    LIST_WINDOW_ID, PANEL_OPEN_ID, PANEL_PREFIX, build_id, row_id,
};
use hexgame::game::{Game, Sight};
use hexgame::session::{ALL_CITIES_ID, CITY_DETAILS_ID, MENU_CITIES_ID, MY_CITIES_ID, Session};
use hexgame::setup::Setup;
use hexgame::state::scenario::{self, Scenario};
use hexgame::state::{Hex, Side};
use turbo_ferret::widgets::{ImageWidget, Window};
use turbo_ferret::{Point, Rect, Size, SpecialKey, TbId, Visibility};

const WRENFOLD: Hex = Hex::new(10, 8);
const LOW_TARN: Hex = Hex::new(10, 16);
const HIGHCLIFF: Hex = Hex::new(3, 3);

/// Wrenfold, green's, with three units (the game starts there, so the player
/// is green); Low Tarn, nobody's; Highcliff, blue's.
fn session_at(w: i32, h: i32, dpi: i32) -> Session {
    let mut rows = vec!["g".repeat(20); 20];
    rows[3] = "gggcgggggggggggggggg".to_string();
    rows[10] = "ggggggggcGGWGgggcggg".to_string();
    let terrain = format!("20\n20\n{}\n", rows.join("\n"));
    let units = "unit riders\nname Riders\nimage units/lightcavalry.png\nstrength 4\nmoves 10\nprice 9\n\
                 enter grass 2, city 1, road 1, bridge 1\n\
                 unit militia\nname Militia\nimage units/militia.png\nstrength 2\nmoves 8\nenter grass 2, city 1, road 1\n";
    let cities = "city 10 8\nname Wrenfold\nside green\nwalls 1\nproduce riders 2\nproduce militia 1\n\
                  garrison militia 2\ngarrison riders\n\
                  city 10 16\nname Low Tarn\n\
                  city 3 3\nname Highcliff\nside blue\nproduce militia 1\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    Session::with_game(&assets(), Size::new(w, h), dpi, game).unwrap()
}

fn session() -> Session {
    session_at(1024, 768, 96)
}

/// The same game, with a person playing `side` and the computer passing the
/// others' turns.
fn session_as(side: Side) -> Session {
    let s = session();
    let game = std::mem::replace(&mut { s }.screen.game, common::grassland());
    let setup = Setup::solo(&game.map, Some(side), "passive");
    let mut s = Session::with_game(&assets(), Size::new(1024, 768), 96, game.with_setup(setup)).unwrap();
    // The computer's turns before the person's (the frames would play them,
    // the log open), and the person's started.
    s.wait_for_turn();
    s.start_turn();
    s
}

/// The whole widget in window pixels.
fn frame(s: &Session, name: &str) -> Rect {
    let id = widget(s, name);
    Rect::from_origin_size(s.screen.ui.to_root(id, Point::ZERO), s.screen.ui.rect(id).size())
}

fn exists(s: &Session, name: &str) -> bool {
    s.screen.ui.find(s.screen.ui.root(), TbId::new(name)).is_some()
}

fn push(s: &mut Session, name: &str) {
    let r = s.screen.area(widget(s, name));
    click(s, Point::new(r.x + r.w / 2, r.y + r.h / 2));
}

fn pressed(s: &Session, name: &str) -> bool {
    s.screen.ui.value(widget(s, name)).as_int() == 1
}

fn disabled(s: &Session, name: &str) -> bool {
    s.screen.ui.node(widget(s, name)).is_some_and(|n| n.is_disabled())
}

fn city_id(s: &Session, hex: Hex) -> hexgame::state::CityId {
    s.screen.game.map.city_at(hex).unwrap().id
}

#[test]
fn all_cities_lists_every_city_and_rings_the_chosen_one() {
    let mut s = session();
    assert_eq!(s.screen.game.player(), Side::Green);
    assert!(!exists(&s, LIST_WINDOW_ID));
    choose(&mut s, MENU_CITIES_ID, ALL_CITIES_ID);
    let list = s.city_list().expect("the list is open");
    assert!(!list.own());
    assert_eq!(list.rows(), ["Wrenfold (green)", "Low Tarn (nobody's)", "Highcliff (blue)"]);
    // The selected hex's city is the chosen one; its row is marked.
    assert_eq!(list.chosen(), Some(city_id(&s, WRENFOLD)));
    assert!(pressed(&s, &row_id(0)) && !pressed(&s, &row_id(1)));
    // Another row: another city.
    push(&mut s, &row_id(2));
    assert_eq!(s.city_list().unwrap().chosen(), Some(city_id(&s, HIGHCLIFF)));
    assert!(pressed(&s, &row_id(2)) && !pressed(&s, &row_id(0)));
    // The minimap beside the list has a picture its own size.
    let images: Vec<_> = descendants(&s, LIST_WINDOW_ID)
        .into_iter()
        .filter_map(|id| s.screen.ui.get::<ImageWidget>(id).map(|w| (id, w.image().size())))
        .collect();
    assert_eq!(images.len(), 1);
    let (id, size) = images[0];
    assert_eq!(size, s.screen.ui.rect(id).size());
    assert!(size.w > 100 && size.h > 80, "{size:?}");
    // The button again closes it.
    choose(&mut s, MENU_CITIES_ID, ALL_CITIES_ID);
    assert!(s.city_list().is_none() && !exists(&s, LIST_WINDOW_ID));
}

#[test]
fn my_cities_lists_the_players_with_what_they_build() {
    let mut s = session();
    choose(&mut s, MENU_CITIES_ID, MY_CITIES_ID);
    let list = s.city_list().unwrap();
    assert!(list.own());
    assert_eq!(list.rows(), ["Wrenfold: building nothing"]);
    let title = s.screen.ui.text(widget(&s, LIST_WINDOW_ID)).unwrap().to_string();
    assert_eq!(title, "Green's cities");
    // From one list to the other.
    choose(&mut s, MENU_CITIES_ID, ALL_CITIES_ID);
    assert_eq!(s.city_list().unwrap().rows().len(), 3);
    // A side with no city has a word for it.
    choose(&mut s, MENU_CITIES_ID, ALL_CITIES_ID);
    let root = s.screen.root_window();
    let area = s.screen.map_rect();
    let list = CityList::open(
        &mut s.screen.ui,
        root,
        &s.screen.game.map,
        true,
        Side::Purple,
        (area, (0.0, 0.0)),
        &Sight::all(),
    );
    assert!(list.rows().is_empty());
    let says: Vec<String> =
        descendants(&s, LIST_WINDOW_ID).into_iter().filter_map(|id| s.screen.ui.text(id).map(str::to_string)).collect();
    assert!(says.iter().any(|t| t == "You hold no city."), "{says:?}");
}

#[test]
fn details_open_the_city_and_go_to_shows_it_on_the_map() {
    let mut s = session();
    choose(&mut s, MENU_CITIES_ID, ALL_CITIES_ID);
    push(&mut s, &row_id(1));
    push(&mut s, LIST_DETAILS_ID);
    let window = s.city_window().expect("Low Tarn's window");
    assert_eq!(window.city(), city_id(&s, LOW_TARN));
    assert_eq!(s.screen.ui.text(widget(&s, CITY_WINDOW_ID)), Some("Low Tarn"));
    // Both windows are open, over the map, side by side.
    let (list, city, map) = (frame(&s, LIST_WINDOW_ID), frame(&s, CITY_WINDOW_ID), s.screen.map_rect());
    assert!(list.right() <= city.x, "{list:?} {city:?}");
    for r in [list, city] {
        assert!(r.x >= map.x && r.right() <= map.right() && r.y >= map.y && r.bottom() <= map.bottom(), "{r:?}");
    }
    // Go to: the cursor and the view go there, and the panels are about it.
    push(&mut s, LIST_GOTO_ID);
    assert_eq!(s.screen.game.map.cursor, LOW_TARN);
    assert!(s.screen.view.shows(LOW_TARN));
    assert_eq!(s.shown(), LOW_TARN);
    assert_eq!(s.city()[0], "Low Tarn");
    // Without a chosen city the buttons do nothing.
    choose(&mut s, MENU_CITIES_ID, MY_CITIES_ID);
    choose(&mut s, MENU_CITIES_ID, MY_CITIES_ID);
    assert!(s.city_list().is_none());
}

#[test]
fn the_player_sets_what_a_city_builds_and_the_turns_build_it() {
    let mut s = session();
    push(&mut s, PANEL_OPEN_ID);
    assert_eq!(s.city_window().unwrap().city(), city_id(&s, WRENFOLD));
    let says = |s: &Session| -> Vec<String> {
        descendants(s, CITY_WINDOW_ID).into_iter().filter_map(|id| s.screen.ui.text(id).map(str::to_string)).collect()
    };
    let lines = says(&s);
    for line in ["Held by green, walls 1, at row 10, column 8", "Garrison: 2 Militia, 1 Riders", "Building nothing"] {
        assert!(lines.iter().any(|t| t == line), "{line} is not in {lines:?}");
    }
    assert!(pressed(&s, BUILD_NOTHING_ID) && !disabled(&s, &build_id(0)));
    // Riders, two turns.
    push(&mut s, &build_id(0));
    assert!(pressed(&s, &build_id(0)) && !pressed(&s, BUILD_NOTHING_ID));
    assert_eq!(s.screen.game.map.city_at(WRENFOLD).unwrap().building, Some(0));
    // A round: the turn ends, the others play theirs, the next starts.
    assert!(s.next_turn());
    assert!(says(&s).iter().any(|t| t == "Building Riders, 1 of 2 turns done"), "{:?}", says(&s));
    assert!(s.next_turn());
    assert_eq!(s.screen.game.map.units_at(WRENFOLD).len(), 4, "the rider is done");
    assert!(says(&s).iter().any(|t| t == "Garrison: 2 Militia, 2 Riders"), "{:?}", says(&s));
    // The panels under the map show it too: a fourth slot.
    assert_eq!(s.stack().len(), 4);
    // Nothing: the work stops.
    push(&mut s, BUILD_NOTHING_ID);
    assert_eq!(s.screen.game.map.city_at(WRENFOLD).unwrap().building, None);
}

#[test]
fn only_the_holder_sets_what_a_city_builds() {
    let mut s = session();
    choose(&mut s, MENU_CITIES_ID, ALL_CITIES_ID);
    push(&mut s, &row_id(2));
    push(&mut s, LIST_DETAILS_ID);
    assert_eq!(s.city_window().unwrap().city(), city_id(&s, HIGHCLIFF));
    assert!(disabled(&s, &build_id(0)) && disabled(&s, BUILD_NOTHING_ID));
    let says: Vec<String> =
        descendants(&s, CITY_WINDOW_ID).into_iter().filter_map(|id| s.screen.ui.text(id).map(str::to_string)).collect();
    // Another side's city: what it builds is its holder's to know; its
    // buttons are not shown.
    assert!(says.iter().any(|t| t == "Only blue knows what it builds."), "{says:?}");
    // Shown: it and everything it is in.
    let shown = |s: &Session, id: &str| {
        let ui = &s.screen.ui;
        std::iter::successors(Some(widget(s, id)), |w| ui.parent(*w))
            .all(|w| ui.node(w).is_some_and(|n| n.visibility() == Visibility::Visible))
    };
    assert!(!shown(&s, BUILD_NOTHING_ID) && !shown(&s, CITY_RALLY_ID));
    push(&mut s, &build_id(0));
    assert_eq!(s.screen.game.map.city_at(HIGHCLIFF).unwrap().building, None);
    // When a person plays blue, on blue's turn, they can.
    let mut s = session_as(Side::Blue);
    assert_eq!(s.screen.game.player(), Side::Blue, "green, the computer, has passed");
    choose(&mut s, MENU_CITIES_ID, ALL_CITIES_ID);
    push(&mut s, &row_id(2));
    push(&mut s, LIST_DETAILS_ID);
    assert!(!disabled(&s, &build_id(0)));
    push(&mut s, &build_id(0));
    assert_eq!(s.screen.game.map.city_at(HIGHCLIFF).unwrap().building, Some(0));
}

#[test]
fn esc_and_the_close_buttons_shut_the_windows_one_at_a_time() {
    let mut s = session();
    choose(&mut s, MENU_CITIES_ID, ALL_CITIES_ID);
    push(&mut s, LIST_DETAILS_ID);
    assert!(s.city_window().is_some() && s.city_list().is_some());
    // Esc: the city's, on top, then the list.
    press(&mut s, SpecialKey::Esc);
    assert!(s.city_window().is_none() && s.city_list().is_some());
    press(&mut s, SpecialKey::Esc);
    assert!(s.city_list().is_none());
    // The help, if it is open, goes first.
    choose(&mut s, MENU_CITIES_ID, ALL_CITIES_ID);
    press(&mut s, SpecialKey::F1);
    press(&mut s, SpecialKey::Esc);
    assert!(!s.screen.help_open() && s.city_list().is_some());
    // The close buttons, and the title bars' ones.
    push(&mut s, LIST_CLOSE_ID);
    assert!(s.city_list().is_none());
    push(&mut s, PANEL_OPEN_ID);
    push(&mut s, CITY_CLOSE_ID);
    assert!(s.city_window().is_none());
    push(&mut s, PANEL_OPEN_ID);
    let x = s.screen.ui.get::<Window>(widget(&s, CITY_WINDOW_ID)).and_then(|w| w.close_button()).unwrap();
    let r = s.screen.area(x);
    click(&mut s, Point::new(r.x + r.w / 2, r.y + r.h / 2));
    s.frame(16);
    assert!(s.city_window().is_none() && !exists(&s, CITY_WINDOW_ID));
}

#[test]
fn the_sidebar_has_ones_own_city_in_full_and_anothers_from_outside() {
    let mut s = session();
    let visible = |s: &Session| s.screen.ui.node(widget(s, CITY_DETAILS_ID)).map(|n| n.visibility());
    // Wrenfold, the player's: in full, and what it builds is set there.
    assert_eq!(s.city_panel().map(|p| p.city()), Some(city_id(&s, WRENFOLD)));
    assert_eq!(visible(&s), Some(Visibility::Gone));
    push(&mut s, &format!("{PANEL_PREFIX}{}", build_id(0)));
    assert_eq!(s.screen.game.map.city_at(WRENFOLD).unwrap().building, Some(0));
    assert!(pressed(&s, &format!("{PANEL_PREFIX}{}", build_id(0))));
    // Low Tarn, nobody's: its lines from outside, and a button for its window.
    s.screen.view.center_on(LOW_TARN);
    let p = pixel_of(&s, LOW_TARN);
    click(&mut s, p);
    assert!(s.city_panel().is_none());
    assert_eq!(s.city()[0], "Low Tarn");
    assert_eq!(visible(&s), Some(Visibility::Visible));
    // No city: neither.
    let p = pixel_of(&s, Hex::new(15, 5));
    click(&mut s, p);
    assert!(s.city_panel().is_none() && s.city().is_empty());
    assert_eq!(visible(&s), Some(Visibility::Gone));
}

#[test]
fn the_windows_fit_the_map_and_take_the_wheel_from_it() {
    for (w, h, dpi) in [(1024, 768, 96), (2048, 1536, 192)] {
        let mut s = session_at(w, h, dpi);
        choose(&mut s, MENU_CITIES_ID, ALL_CITIES_ID);
        push(&mut s, LIST_DETAILS_ID);
        let (list, city, map) = (frame(&s, LIST_WINDOW_ID), frame(&s, CITY_WINDOW_ID), s.screen.map_rect());
        assert!(list.right() <= city.x, "{w}x{h}: {list:?} {city:?}");
        assert!(list.x >= map.x && city.right() <= map.right(), "{w}x{h}: {list:?} {city:?} {map:?}");
        // The wheel over a window is the window's, not the map's.
        let over = Point::new(list.x + list.w / 2, list.y + list.h / 2);
        let middle = s.screen.view.middle();
        assert!(!s.screen.scroll(over, 0.0, 300.0), "{w}x{h}");
        assert_eq!(s.screen.view.middle(), middle);
        // And the keyboard is still the map's.
        assert_eq!(s.screen.ui.focused(), Some(s.screen.map_widget()));
    }
}

#[test]
fn what_a_city_can_build_shows_its_icon_and_numbers_and_all_about_it_under_the_pointer() {
    let mut s = session();
    push(&mut s, PANEL_OPEN_ID);
    // A button each: the unit's icon, its name, strength and moves.
    let text = |s: &Session, name: &str| s.screen.ui.text(widget(s, name)).unwrap_or("").to_string();
    assert_eq!(text(&s, &format!("{}_text", build_id(0))), "Riders  4 \u{b7} 10");
    assert_eq!(text(&s, &format!("{}_text", build_id(1))), "Militia  2 \u{b7} 8");
    let icon = widget(&s, &format!("{}_icon", build_id(0)));
    assert!(s.screen.area(icon).w > 0, "the icon has room");
    // Nothing under the pointer, nothing built: a hint.
    assert_eq!(s.city_window().unwrap().details(), ["Point at one: all about it shows here."]);
    // The pointer on Riders: all about them; the window keeps its size.
    let before = frame(&s, CITY_WINDOW_ID);
    let r = s.screen.area(widget(&s, &build_id(0)));
    common::hover(&mut s, Point::new(r.x + r.w / 2, r.y + r.h / 2));
    assert_eq!(frame(&s, CITY_WINDOW_ID), before);
    let details = s.city_window().unwrap().details();
    assert_eq!(details[..2], ["Riders: strength 4, moves 10", "Builds in 2 turns, price 9; upkeep 0"]);
    assert!(details[2].starts_with("Enters: grass 2"), "{details:?}");
    // Away again: what it builds, once it builds something.
    push(&mut s, &build_id(1));
    common::hover(&mut s, Point::new(5, 5));
    assert_eq!(s.city_window().unwrap().details()[0], "Militia: strength 2, moves 8");
}
