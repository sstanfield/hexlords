//! The panels around the map: the line about the hex under the cursor, the
//! selected hex with a button for each of its units and all about the unit
//! that is selected, and the selected hex's city. No window and no GPU: a
//! `Session` on the real assets, fed what the window would send.

mod common;

use common::{assets, click, hover, pixel_of, widget};
use hexgame::game::Game;
use hexgame::session::{CITY_LINES, DETAIL_LINES, Session, unit_button_id};
use hexgame::state::scenario::{self, Scenario};
use hexgame::state::{Hex, MAX_STACK};
use turbo_ferret::{Modifiers, Point, Rect, Size, SpecialKey, Visibility};

const CITY: Hex = Hex::new(10, 8);

/// A city with three units in it and a road east over a bridge, and a
/// second city that is nobody's.
fn session_at(w: i32, h: i32, dpi: i32) -> Session {
    let mut rows = vec!["g".repeat(20); 20];
    rows[10] = "ggggggggcGGWGgggcggg".to_string();
    let terrain = format!("20\n20\n{}\n", rows.join("\n"));
    let units = "unit riders\nname Riders\nimage units/riders.png\nstrength 4\nmoves 10\nupkeep 3\nprice 9\n\
                 enter grass 2, forest 5, city 1, road 1, bridge 1\n\
                 unit militia\nname Militia\nimage units/militia.png\nstrength 2\nmoves 8\nenter grass 2, city 1, road 1\n";
    let cities = "city 10 8\nname Wrenfold\nside green\nwalls 1\nproduce riders 2\ngarrison militia 2\ngarrison riders\n\
                  city 10 16\nname Low Tarn\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    Session::with_game(&assets(), Size::new(w, h), dpi, game).unwrap()
}

fn session() -> Session {
    session_at(1024, 768, 96)
}

fn rect(s: &Session, name: &str) -> Rect {
    s.screen.area(widget(s, name))
}

fn push(s: &mut Session, name: &str) {
    let r = rect(s, name);
    click(s, Point::new(r.x + r.w / 2, r.y + r.h / 2));
}

fn click_hex(s: &mut Session, hex: Hex) {
    let p = pixel_of(s, hex);
    click(s, p);
}

fn hover_hex(s: &mut Session, hex: Hex) {
    let p = pixel_of(s, hex);
    hover(s, p);
}

fn shown(s: &Session, name: &str) -> bool {
    s.screen.ui.node(widget(s, name)).is_some_and(|n| n.visibility() == Visibility::Visible)
}

fn pressed(s: &Session, name: &str) -> bool {
    s.screen.ui.value(widget(s, name)).as_int() == 1
}

#[test]
fn at_the_start_the_panels_are_about_the_hex_the_cursor_starts_on() {
    let s = session();
    assert_eq!(s.shown(), CITY);
    assert_eq!(s.status(), "Turn 1  |  row 10, column 8: city  |  Wrenfold  |  3 units");
    assert_eq!(s.hex_title(), "Row 10, column 8: city (Wrenfold), 3 units (green)");
    assert_eq!(s.stack(), ["Militia   str 2   mv 8/8", "Militia   str 2   mv 8/8", "Riders   str 4   mv 10/10"]);
    // Nothing is selected yet: a word on what to do.
    assert_eq!(s.unit_details(), ["Click a unit to select it, then a hex to send it there."]);
    assert_eq!(
        s.city(),
        ["Wrenfold", "Held by green", "Walls 1", "Garrison: 3 units", "Builds:", "  Riders: 2 turns, price 9"]
    );
    // The widgets say the same.
    assert_eq!(s.screen.ui.text(widget(&s, "status")), Some(s.status()));
    assert_eq!(s.screen.ui.text(widget(&s, "hex_title")), Some(s.hex_title()));
    assert_eq!(s.screen.ui.text(widget(&s, "unit_2")), Some("Riders   str 4   mv 10/10"));
    assert_eq!(s.screen.ui.text(widget(&s, "city_0")), Some("Wrenfold"));
    // A button for each unit that is there, and no more.
    for i in 0..MAX_STACK {
        assert_eq!(shown(&s, &unit_button_id(i)), i < 3, "unit_{i}");
        assert!(!pressed(&s, &unit_button_id(i)));
    }
}

#[test]
fn the_panels_are_laid_out_under_the_map_and_under_the_minimap() {
    for (w, h, dpi) in [(1024, 768, 96), (2048, 1536, 192), (1400, 900, 96)] {
        let mut s = session_at(w, h, dpi);
        s.frame(16);
        let (map, mini) = (s.screen.map_rect(), s.screen.minimap_rect());
        let window = Rect::new(0, 0, w, h);
        let inside = |r: Rect| r.x >= 0 && r.y >= 0 && r.right() <= window.w && r.bottom() <= window.h;
        // Under the map, one below the other: the cursor's line, the heading,
        // the units' buttons with the selected unit's lines beside them.
        let (status, title, first) = (rect(&s, "status"), rect(&s, "hex_title"), rect(&s, "unit_0"));
        assert!(status.y >= map.bottom() && title.y >= status.bottom() && first.y >= title.bottom(), "{w}x{h}");
        assert_eq!((status.x, status.w, title.w), (0, map.w, map.w), "{w}x{h}");
        for i in 0..3 {
            let button = rect(&s, &unit_button_id(i));
            assert!(inside(button) && button.w > 0 && button.right() <= map.right(), "{w}x{h}: unit_{i} {button:?}");
        }
        let detail = rect(&s, "detail_0");
        assert!(
            detail.x >= first.right() && detail.right() <= map.right() + 1 && inside(detail),
            "{w}x{h}: {detail:?}"
        );
        // Under the minimap: the button, then the city's lines, in the sidebar.
        let end_turn = rect(&s, "end_turn");
        assert!(end_turn.y >= mini.bottom());
        let mut below = end_turn.bottom();
        for i in 0..CITY_LINES {
            let line = rect(&s, &format!("city_{i}"));
            assert!(line.y >= below && line.x >= mini.x && line.right() <= mini.right(), "{w}x{h}: city_{i} {line:?}");
            assert!(line.bottom() <= map.bottom(), "{w}x{h}: city_{i} is beside the map, not below it");
            below = line.bottom();
        }
        // With a full stack the second column of buttons is there too, and
        // all eight fit under the map.
        let riders = s.screen.game.map.units().kind_named("riders").unwrap();
        for _ in 0..5 {
            let unit = s.screen.game.map.new_unit_of(hexgame::state::Side::Green, riders).unwrap();
            s.screen.game.map.place_unit(unit, CITY).unwrap();
        }
        s.frame(32);
        s.frame(48);
        assert_eq!(s.stack().len(), 8);
        for i in 0..MAX_STACK {
            let button = rect(&s, &unit_button_id(i));
            assert!(shown(&s, &unit_button_id(i)) && inside(button), "{w}x{h}: unit_{i} {button:?}");
        }
        assert!(rect(&s, "unit_4").x >= rect(&s, "unit_0").right(), "{w}x{h}: the second column");
        assert!(rect(&s, "detail_0").x >= rect(&s, "unit_4").right(), "{w}x{h}");
        for i in 0..DETAIL_LINES {
            assert!(inside(rect(&s, &format!("detail_{i}"))), "{w}x{h}: detail_{i}");
        }
    }
}

#[test]
fn a_units_button_selects_it_and_says_all_about_it() {
    let mut s = session();
    let ids = s.screen.game.map.units_at(CITY).to_vec();
    // The third unit of the stack: the rider.
    push(&mut s, "unit_2");
    assert_eq!(s.screen.game.selected(), Some(ids[2]));
    assert!(pressed(&s, "unit_2") && !pressed(&s, "unit_0") && !pressed(&s, "unit_1"));
    assert_eq!(
        s.unit_details(),
        [
            "Riders (green)",
            "Strength 4   Moves 10/10   Upkeep 3   Price 9",
            "Enters: grass 2, forest 5, city 1, road 1, bridge 1",
        ]
    );
    // Its range is the one that shows, and it is the unit that shows on the hex.
    use hexgame::screen::Scene;
    assert!(s.marked(Hex::new(10, 12)), "the rider gets over the bridge");
    assert_eq!(s.front_unit(), Some(ids[2]));
    hover_hex(&mut s, Hex::new(10, 12));
    assert_eq!(s.status(), "Turn 1  |  row 10, column 12: grass, road  |  4 of 10 points to move here");

    // Another button: another unit, with its own reach.
    push(&mut s, "unit_0");
    assert_eq!(s.screen.game.selected(), Some(ids[0]));
    assert!(pressed(&s, "unit_0") && !pressed(&s, "unit_2"));
    assert_eq!(s.unit_details()[0], "Militia (green)");
    let bridge = Hex::new(10, 11);
    assert!(!s.marked(bridge), "the militia does not cross bridges");
    hover_hex(&mut s, bridge);
    assert_eq!(s.status(), "Turn 1  |  row 10, column 11: water, bridge");
    // The keys still go to the map: the buttons do not take them.
    assert_eq!(s.screen.ui.focused(), Some(s.screen.map_widget()));
}

#[test]
fn the_selected_unit_goes_where_it_is_sent_and_the_panels_follow() {
    let mut s = session();
    let ids = s.screen.game.map.units_at(CITY).to_vec();
    push(&mut s, "unit_2");
    let far = Hex::new(10, 12);
    click_hex(&mut s, far);
    // The rider is there, and so are the panels.
    assert_eq!(s.screen.game.map.units_at(far), [ids[2]]);
    assert_eq!(s.screen.game.selected(), None);
    assert_eq!(s.shown(), far);
    assert_eq!(s.hex_title(), "Row 10, column 12: grass, road, 1 unit (green)");
    assert_eq!(s.stack(), ["Riders   str 4   mv 6/10"]);
    assert!(shown(&s, "unit_0") && !shown(&s, "unit_1") && !pressed(&s, "unit_0"));
    assert_eq!(s.city(), Vec::<&str>::new(), "no city here");
    assert_eq!(s.unit_details(), ["Click a unit to select it, then a hex to send it there."]);

    // A click on a hex with units selects the first, as before, and shows it.
    click_hex(&mut s, CITY);
    assert_eq!(s.screen.game.selected(), Some(ids[0]));
    assert!(pressed(&s, "unit_0"));
    assert_eq!(s.stack().len(), 2);
    assert_eq!(s.city()[3], "Garrison: 2 units");
    assert_eq!(s.unit_details()[0], "Militia (green)");
    // A click on its own hex deselects it.
    click_hex(&mut s, CITY);
    assert_eq!(s.screen.game.selected(), None);
    assert!(!pressed(&s, "unit_0"));

    // An empty hex: its terrain, nobody, nothing to say about a unit.
    click_hex(&mut s, Hex::new(8, 8));
    assert_eq!(s.hex_title(), "Row 8, column 8: grass, no units");
    assert!(s.stack().is_empty() && s.unit_details().is_empty() && s.city().is_empty());
    assert!((0..MAX_STACK).all(|i| !shown(&s, &unit_button_id(i))));
    // The other city, which is nobody's.
    click_hex(&mut s, Hex::new(10, 16));
    assert_eq!(s.city(), ["Low Tarn", "Nobody's", "Walls 0", "No garrison", "Builds nothing"]);
}

#[test]
fn a_hex_out_of_reach_is_looked_at_while_the_unit_stays_selected() {
    let mut s = session();
    let ids = s.screen.game.map.units_at(CITY).to_vec();
    push(&mut s, "unit_0");
    // Row 2 is too far for a militia: the click shows the hex, no more.
    click_hex(&mut s, Hex::new(5, 8));
    click_hex(&mut s, Hex::new(5, 8));
    assert_eq!(s.screen.game.selected(), Some(ids[0]), "still selected");
    assert_eq!(s.screen.game.map.units_at(CITY).len(), 3, "and where it was");
    assert_eq!(s.shown(), Hex::new(5, 8));
    assert_eq!(s.hex_title(), "Row 5, column 8: grass, no units");
    // The lines are about the selected unit, wherever the panels look.
    assert_eq!(s.unit_details()[0], "Militia (green)");
    assert!(s.stack().is_empty());
}

#[test]
fn the_turn_ends_by_the_button_and_by_the_key() {
    let mut s = session();
    push(&mut s, "unit_2");
    click_hex(&mut s, Hex::new(10, 12));
    assert_eq!(s.stack(), ["Riders   str 4   mv 6/10"]);
    // Ctrl+T ends the turn: the rider has its points back, and two to spare.
    s.screen.ui.key(Some('T'), SpecialKey::None, Modifiers::CTRL, true);
    s.pump();
    assert_eq!(s.screen.game.turn(), 2);
    assert_eq!(s.stack(), ["Riders   str 4   mv 12/10"]);
    assert!(s.status().starts_with("Turn 2  |  "), "{}", s.status());
    push(&mut s, "end_turn");
    assert_eq!(s.screen.game.turn(), 3);
    // A plain t is nobody's key.
    s.screen.ui.key(Some('t'), SpecialKey::None, Modifiers::NONE, true);
    s.pump();
    assert_eq!(s.screen.game.turn(), 3);
}

#[test]
fn the_demo_has_its_one_unit_in_the_panels() {
    let mut s = common::session();
    assert_eq!(s.hex_title(), "Row 50, column 50: mountain, 1 unit (black)");
    assert_eq!(s.stack(), ["Test Cav   str 5   mv 20/20"]);
    assert!(s.city().is_empty());
    push(&mut s, "unit_0");
    assert_eq!(s.unit_details()[..2], ["Test Cav (black)", "Strength 5   Moves 20/20   Upkeep 3"]);
    assert!(s.screen.game.selected().is_some());
}
