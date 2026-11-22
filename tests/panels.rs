//! The panel under the map (the line about the hex under the cursor, the
//! selected hex, a slot for each of its units, the selected unit in a line,
//! and the box the pointer opens over a unit), and the selected hex's city
//! beside the map. No window and no GPU: a `Session` on the real assets, fed
//! what the window would send.

mod common;

use common::{assets, click, hover, pixel_of, widget};
use hexlords::game::Game;
use hexlords::screen::Scene;
use hexlords::session::{LEADER_ID, STACK_SLOTS, Session, TIP_LINES, unit_button_id};
use hexlords::state::scenario::{self, Scenario};
use hexlords::state::{DEFAULT_UNIT_IMAGE, Hex, MAX_STACK, Side};
use turbo_ferret::{Modifiers, Point, Rect, Size, SpecialKey, Visibility};

const CITY: Hex = Hex::new(10, 8);

/// A city with three units in it and a road east over a bridge, and a
/// second city that is nobody's.
fn session_at(w: i32, h: i32, dpi: i32) -> Session {
    let mut rows = vec!["g".repeat(20); 20];
    rows[10] = "ggggggggcGGWGgggcggg".to_string();
    let terrain = format!("20\n20\n{}\n", rows.join("\n"));
    let units = "unit riders\nname Riders\nimage units/lightcavalry.png\nstrength 4\nmoves 10\nupkeep 3\nprice 9\n\
                 enter grass 2, forest 5, city 1, road 1, bridge 1\n\
                 unit militia\nname Militia\nimage units/militia.png\nstrength 2\nmoves 8\nenter grass 2, city 1, road 1\n";
    let cities = "city 10 8\nname Wrenfold\nside green\nwalls 1\nproduce riders 2\ngarrison militia 2\ngarrison riders\n\
                  city 10 16\nname Low Tarn\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    common::with_game(&assets(), Size::new(w, h), dpi, game).unwrap()
}

fn session() -> Session {
    session_at(1024, 768, 96)
}

fn rect(s: &Session, name: &str) -> Rect {
    s.screen.area(widget(s, name))
}

/// The whole widget, its skin's padding too, in window pixels.
fn frame(s: &Session, name: &str) -> Rect {
    let id = widget(s, name);
    Rect::from_origin_size(s.screen.ui.to_root(id, Point::ZERO), s.screen.ui.rect(id).size())
}

fn middle(r: Rect) -> Point {
    Point::new(r.x + r.w / 2, r.y + r.h / 2)
}

fn push(s: &mut Session, name: &str) {
    let p = middle(rect(s, name));
    click(s, p);
}

fn point_at(s: &mut Session, name: &str) {
    let p = middle(rect(s, name));
    hover(s, p);
}

fn click_hex(s: &mut Session, hex: Hex) {
    let p = pixel_of(s, hex);
    click(s, p);
}

fn hover_hex(s: &mut Session, hex: Hex) {
    let p = pixel_of(s, hex);
    hover(s, p);
}

fn visibility(s: &Session, name: &str) -> Visibility {
    s.screen.ui.node(widget(s, name)).map(|n| n.visibility()).unwrap()
}

fn shown(s: &Session, name: &str) -> bool {
    visibility(s, name) == Visibility::Visible
}

/// A box of the stack with a unit in it (an empty one is there, dimmed).
fn filled(s: &Session, name: &str) -> bool {
    shown(s, name) && s.screen.ui.node(widget(s, name)).is_some_and(|n| !n.is_disabled())
}

/// What the widgets in this one say.
fn texts_in(s: &Session, name: &str) -> Vec<String> {
    common::descendants(s, name).into_iter().filter_map(|id| s.screen.ui.text(id).map(str::to_string)).collect()
}

fn pressed(s: &Session, name: &str) -> bool {
    s.screen.ui.value(widget(s, name)).as_int() == 1
}

const HINT: &str = "Click your units to select them, then a hex to send them there.";

#[test]
fn at_the_start_the_panels_are_about_the_hex_the_cursor_starts_on() {
    let s = session();
    assert_eq!(s.shown(), CITY);
    assert_eq!(s.status(), "Turn 1  |  row 10, column 8: city  |  Wrenfold  |  3 units");
    assert_eq!(s.hex_title(), "Row 10, column 8: city (Wrenfold), 3 units (green)");
    // The units in the stack's order, each its strength and its moves.
    assert_eq!(s.stack(), ["2 \u{b7} 8/8", "2 \u{b7} 8/8", "4 \u{b7} 10/10"]);
    assert_eq!(s.slot_icon(0), Some(("units/militia.png", Side::Green)));
    assert_eq!(s.slot_icon(2), Some(("units/lightcavalry.png", Side::Green)));
    assert_eq!(s.slot_icon(3), None);
    // Nothing is selected yet: a word on what to do. No box is open.
    assert_eq!(s.unit_line(), HINT);
    assert!(s.tip().is_empty());
    // Green's own city: in full in the sidebar (no lines of another's).
    assert_eq!(s.city_panel().map(|p| p.city()), s.screen.game.map.city_at(CITY).map(|c| c.id));
    assert!(s.city().is_empty());
    let panel = texts_in(&s, "city_box");
    for line in ["Wrenfold", "Walls 1, taxes 10", "Garrison: 3 units", "Building nothing", "New units stay here"] {
        assert!(panel.iter().any(|t| t == line), "{line} is not in {panel:?}");
    }
    // The widgets say the same.
    assert_eq!(s.screen.ui.text(widget(&s, "status")), Some(s.status()));
    assert_eq!(s.screen.ui.text(widget(&s, "hex_title")), Some(s.hex_title()));
    assert_eq!(s.screen.ui.text(widget(&s, "unit_line")), Some(HINT));
    assert_eq!(s.screen.ui.text(widget(&s, "unit_2_text")), Some("4 \u{b7} 10/10"));
    // A box for each unit a hex holds, and its leader's on their left: the
    // empty ones stay, dimmed; the leader's says whose it is.
    assert_eq!(STACK_SLOTS, MAX_STACK);
    for i in 0..STACK_SLOTS {
        let id = unit_button_id(i);
        assert!(shown(&s, &id), "{id}");
        assert_eq!(filled(&s, &id), i < 3, "{id}");
        assert!(!pressed(&s, &id));
    }
    assert!(shown(&s, LEADER_ID) && !filled(&s, LEADER_ID));
    assert_eq!(s.screen.ui.text(widget(&s, &format!("{LEADER_ID}_text"))), Some("Leader"));
    assert_eq!(s.leader_icon(), None);
    assert_eq!(visibility(&s, "unit_tip"), Visibility::Gone);
}

#[test]
fn the_map_takes_all_the_room_the_panels_leave() {
    for (w, h, dpi) in [(1024, 768, 96), (2048, 1536, 192), (1400, 900, 96), (1024, 600, 96), (2880, 1700, 192)] {
        let mut s = session_at(w, h, dpi);
        s.frame(16);
        let dp = |n: i32| n * dpi / 96;
        let (map, mini) = (s.screen.map_rect(), s.screen.minimap_rect());
        let inside = |r: Rect| r.x >= 0 && r.y >= 0 && r.right() <= w && r.bottom() <= h;
        // The map at the left under the menu bar, 80% of the width.
        let top = frame(&s, "menu_bar").bottom() + dp(4);
        assert_eq!((map.x, map.y, map.w), (0, top, (w as f32 * 0.8) as i32), "{w}x{h}");
        // Under it, one below the other and as wide as it: the cursor's line,
        // the heading, the slots, the selected unit's line, down to the
        // window's bottom edge.
        let (status, title, line) = (rect(&s, "status"), rect(&s, "hex_title"), rect(&s, "unit_line"));
        let ids: Vec<String> =
            std::iter::once(LEADER_ID.to_string()).chain((0..STACK_SLOTS).map(unit_button_id)).collect();
        let slots: Vec<Rect> = ids.iter().map(|id| rect(&s, id)).collect();
        assert_eq!(status.y, map.bottom() + dp(5), "{w}x{h}");
        assert!(title.y >= status.bottom() && slots[0].y >= title.bottom() && line.y >= slots[0].bottom());
        assert_eq!((status.x, status.w, title.w, line.w), (0, map.w, map.w, map.w), "{w}x{h}");
        assert_eq!(line.bottom(), h, "{w}x{h}: no room is left under the panel");
        // Nine boxes in a row (the leader's and eight), the empty ones too,
        // and the buttons after them, inside the map's width.
        for (i, slot) in slots.iter().enumerate() {
            assert!(inside(*slot) && slot.w > 0 && slot.h > 0 && slot.right() <= map.right(), "{w}x{h}: {i} {slot:?}");
            assert_eq!(slot.y, slots[0].y);
            if i > 0 {
                assert!(slot.x >= slots[i - 1].right(), "{w}x{h}: slot {i}");
            }
        }
        // The icon fills its 40 dp in the slot.
        let icon = rect(&s, "unit_0_icon");
        assert_eq!((icon.w, icon.h), (dp(40), dp(40)), "{w}x{h}");
        assert!(icon.x >= slots[1].x && icon.right() <= slots[1].right(), "{w}x{h}");
        let next = rect(&s, "next_stack");
        assert!(next.x >= slots[8].right() && next.right() <= map.right() && inside(next), "{w}x{h}: {next:?}");
        // Beside it all, the sidebar: the minimap, the button, the city (the
        // player's own, in a box that scrolls if it needs more room).
        assert_eq!((mini.x, mini.y), (map.right() + dp(5), map.y), "{w}x{h}");
        assert_eq!(mini.right(), w);
        let end_turn = rect(&s, "end_turn");
        assert!(end_turn.y >= mini.bottom());
        let city = rect(&s, "city_scroll");
        assert!(
            city.y >= end_turn.bottom() && city.x >= mini.x && city.right() <= w && inside(city),
            "{w}x{h}: {city:?}"
        );

        // A full stack fills the eight boxes; the leader's stays empty.
        let riders = s.screen.game.map.units().kind_named("riders").unwrap();
        for _ in 0..5 {
            let unit = s.screen.game.map.new_unit_of(Side::Green, riders).unwrap();
            s.screen.game.map.place_unit(unit, CITY).unwrap();
        }
        s.frame(32);
        assert_eq!(s.stack().len(), MAX_STACK);
        for i in 0..STACK_SLOTS {
            assert!(filled(&s, &unit_button_id(i)), "{w}x{h}: box {i}");
        }
        assert!(!filled(&s, LEADER_ID));
        // The map is the size it was: the panel does not grow.
        assert_eq!(s.screen.map_rect(), map, "{w}x{h}");
    }
}

#[test]
fn a_narrow_window_has_narrower_slots() {
    for (w, h, dpi) in [(640, 700, 96), (640, 480, 96), (500, 400, 96), (640, 700, 192)] {
        let s = session_at(w, h, dpi);
        let map = s.screen.map_rect();
        let last = rect(&s, &unit_button_id(STACK_SLOTS - 1));
        assert!(last.right() <= map.right(), "{w}x{h}: {last:?} {map:?}");
        assert!(rect(&s, "mini_map").right() <= w);
        // Each icon fits its box.
        let (slot, icon) = (rect(&s, "unit_0"), rect(&s, "unit_0_icon"));
        assert!(icon.w <= slot.w && icon.x >= slot.x && icon.right() <= slot.right(), "{w}x{h}: {icon:?} {slot:?}");
        // The treasury is in the window, or not there at all.
        let treasury = frame(&s, "treasury");
        assert!(treasury.right() <= w || visibility(&s, "treasury") == Visibility::Gone, "{w}x{h}: {treasury:?}");
    }
}

#[test]
fn a_units_slot_selects_it_and_its_line_says_who_it_is() {
    let mut s = session();
    let ids = s.screen.game.map.units_at(CITY).to_vec();
    // The third unit of the stack: the rider.
    push(&mut s, "unit_2");
    assert_eq!(s.screen.game.selected(), Some(ids[2]));
    assert!(pressed(&s, "unit_2") && !pressed(&s, "unit_0") && !pressed(&s, "unit_1"));
    assert_eq!(s.unit_line(), "Riders (green): strength 4, moves 10/10, upkeep 3, price 9");
    // Its range is the one that shows, and it is the unit the map marks.
    assert!(s.marked(Hex::new(10, 12)), "the rider gets over the bridge");
    assert_eq!(s.front_unit(), Some(ids[2]));
    hover_hex(&mut s, Hex::new(10, 12));
    assert_eq!(s.status(), "Turn 1  |  row 10, column 12: grass, road  |  4 of 10 points to move here");

    // Another slot: that unit comes along too, and the two go where both can.
    push(&mut s, "unit_0");
    assert_eq!(s.screen.game.selection(), [ids[2], ids[0]]);
    assert!(pressed(&s, "unit_0") && pressed(&s, "unit_2") && !pressed(&s, "unit_1"));
    assert!(s.unit_line().starts_with("2 units selected"), "{}", s.unit_line());
    let bridge = Hex::new(10, 11);
    assert!(!s.marked(bridge), "the militia does not cross bridges");
    // The rider's slot again: it stays behind, and the militia goes alone.
    push(&mut s, "unit_2");
    assert_eq!(s.screen.game.selection(), [ids[0]]);
    assert!(pressed(&s, "unit_0") && !pressed(&s, "unit_2"));
    assert!(s.unit_line().starts_with("Militia (green): strength 2, moves 8/8"), "{}", s.unit_line());
    assert!(!s.marked(bridge));
    hover_hex(&mut s, bridge);
    assert_eq!(s.status(), "Turn 1  |  row 10, column 11: water, bridge");
    // The keys still go to the map: the slots do not take them.
    assert_eq!(s.screen.ui.focused(), Some(s.screen.map_widget()));
}

#[test]
fn the_pointer_on_a_unit_opens_a_box_with_all_about_it() {
    let mut s = session();
    point_at(&mut s, "unit_2");
    assert_eq!(
        s.tip(),
        [
            "Riders (green)",
            "Strength 4   Moves 10/10   Upkeep 3   Price 9",
            "Enters: grass 2, forest 5, city 1, road 1, bridge 1"
        ]
    );
    // Pointing selects nothing.
    assert_eq!(s.screen.game.selected(), None);
    // The box is over the slot, inside the window, and only as tall as its
    // lines.
    let (tip, slot) = (frame(&s, "unit_tip"), frame(&s, "unit_2"));
    assert!(tip.bottom() <= slot.y && tip.x == slot.x && tip.right() <= 1024 && tip.y >= 0, "{tip:?} {slot:?}");
    assert!(shown(&s, "tip_2") && !shown(&s, "tip_3") && !shown(&s, "tip_6"));
    assert_eq!(TIP_LINES, 7);
    // It takes no input: the pointer on it is on what is under it, and that
    // closes it.
    let under = Point::new(tip.x + 5, tip.y + 5);
    hover(&mut s, under);
    assert_ne!(s.screen.ui.hovered(), Some(widget(&s, "unit_tip")));
    assert!(s.tip().is_empty());
    assert_eq!(visibility(&s, "unit_tip"), Visibility::Gone);

    // Another unit: its own box.
    point_at(&mut s, "unit_0");
    assert_eq!(s.tip()[0], "Militia (green)");
    // An empty slot opens nothing, and nor does the map.
    point_at(&mut s, "unit_5");
    assert!(s.tip().is_empty());
    point_at(&mut s, "unit_1");
    hover_hex(&mut s, Hex::new(5, 5));
    assert!(s.tip().is_empty());
    // A click on the slot selects the unit, and the box stays open.
    push(&mut s, "unit_1");
    assert_eq!(s.screen.game.selected(), Some(s.screen.game.map.units_at(CITY)[1]));
    assert_eq!(s.tip()[0], "Militia (green)");
}

#[test]
fn a_box_near_the_windows_edge_stays_inside_it() {
    // A window so short that the menu bar and the panel are all there is:
    // the box stays inside the window, as far as it fits in it.
    let mut s = session_at(300, 170, 96);
    point_at(&mut s, "unit_2");
    let tip = frame(&s, "unit_tip");
    assert!(!s.tip().is_empty());
    assert!(tip.x >= 0 && tip.right() <= 300.max(tip.w), "{tip:?}");
    assert!(tip.y >= 0 && tip.bottom() <= 170, "{tip:?}");
    // In a window with room under the slot but none over it, it goes under.
    let mut s = session_at(1024, 768, 96);
    s.screen.ui.set_rect(widget(&s, "hex_panel"), Rect::new(0, -40, 819, 148));
    point_at(&mut s, "unit_2");
    let (tip, slot) = (frame(&s, "unit_tip"), frame(&s, "unit_2"));
    assert!(tip.y >= slot.bottom(), "{tip:?} {slot:?}");
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
    assert_eq!(s.stack(), ["4 \u{b7} 6/10"]);
    assert_eq!(s.slot_icon(0), Some(("units/lightcavalry.png", Side::Green)));
    assert!(filled(&s, "unit_0") && !filled(&s, "unit_1") && !pressed(&s, "unit_0"));
    assert_eq!(s.city(), Vec::<&str>::new(), "no city here");
    assert_eq!(s.unit_line(), HINT);

    // A click on a hex with units selects the stack, to move together, and
    // shows it.
    click_hex(&mut s, CITY);
    assert_eq!(s.screen.game.selection(), [ids[0], ids[1]]);
    assert!(pressed(&s, "unit_0") && pressed(&s, "unit_1"));
    assert_eq!(s.stack().len(), 2);
    assert_eq!(s.slot_icon(0), Some(("units/militia.png", Side::Green)));
    assert!(texts_in(&s, "city_box").iter().any(|t| t == "Garrison: 2 units"));
    assert!(s.unit_line().starts_with("2 units selected"), "{}", s.unit_line());
    // A click on its own hex keeps it selected (the review, 2026-11-21: the
    // screen selects stacks by itself, and a click on one is to take it);
    // Esc drops it.
    click_hex(&mut s, CITY);
    assert_eq!(s.screen.game.selection(), [ids[0], ids[1]]);
    common::press(&mut s, turbo_ferret::SpecialKey::Esc);
    assert_eq!(s.screen.game.selected(), None);
    assert!(!pressed(&s, "unit_0") && !pressed(&s, "unit_1"));

    // An empty hex: its terrain, nobody, nothing to say about a unit.
    click_hex(&mut s, Hex::new(8, 8));
    assert_eq!(s.hex_title(), "Row 8, column 8: grass, no units");
    assert!(s.stack().is_empty() && s.unit_line().is_empty() && s.city().is_empty());
    assert!((0..STACK_SLOTS).all(|i| !filled(&s, &unit_button_id(i))));
    assert!((0..STACK_SLOTS).all(|i| s.slot_icon(i).is_none()));
    // The other city, which is nobody's: what shows from outside (what it
    // builds is not told).
    click_hex(&mut s, Hex::new(10, 16));
    assert_eq!(s.city(), ["Low Tarn", "Nobody's", "Walls 0, taxes 10", "No garrison"]);
}

#[test]
fn a_hex_out_of_reach_sends_the_unit_there_and_the_panels_follow_it() {
    let mut s = session();
    let ids = s.screen.game.map.units_at(CITY).to_vec();
    push(&mut s, "unit_0");
    // Row 5 is too far for a militia this turn: it is sent there (Go to),
    // the panels are about where it got to, and it is no longer selected.
    click_hex(&mut s, Hex::new(5, 8));
    assert_eq!(s.screen.game.selected(), None);
    let militia = s.screen.game.map.units().get(ids[0]).unwrap();
    assert_eq!(militia.goto(), Some(Hex::new(5, 8)));
    assert_ne!(militia.pos(), CITY, "it went as far as it could");
    assert_eq!(s.shown(), militia.pos());
    assert!(!s.marked(Hex::new(10, 9)), "no range shows");
}

#[test]
fn the_turn_ends_by_the_button_and_by_the_key() {
    let mut s = session();
    push(&mut s, "unit_2");
    click_hex(&mut s, Hex::new(10, 12));
    assert_eq!(s.stack(), ["4 \u{b7} 6/10"]);
    // Ctrl+T ends the turn: the rider has its points back, and two to spare.
    s.screen.ui.key(Some('T'), SpecialKey::None, Modifiers::CTRL, true);
    s.pump();
    assert_eq!(s.screen.game.turn(), 2);
    assert_eq!(s.stack(), ["4 \u{b7} 12/10"]);
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
    assert_eq!(s.stack(), ["5 \u{b7} 20/20"]);
    // Its kind has no picture: the default unit's.
    assert_eq!(s.slot_icon(0), Some((DEFAULT_UNIT_IMAGE, Side::Black)));
    assert!(s.city().is_empty());
    push(&mut s, "unit_0");
    assert_eq!(s.unit_line(), "Test Cav (black): strength 5, moves 20/20, upkeep 3");
    assert!(s.screen.game.selected().is_some());
}

#[test]
fn a_map_whose_picture_is_missing_still_shows_its_units() {
    let mut rows = vec!["g".repeat(20); 20];
    rows[3] = "gggcgggggggggggggggg".to_string();
    let terrain = format!("20\n20\n{}\n", rows.join("\n"));
    let units = "unit ghost\nname Ghost\nimage units/nowhere.png\nmoves 4\nenter grass 1\n";
    let cities = "city 3 3\nname Hollow\nside red\ngarrison ghost\n";
    let (map, warnings) = scenario::parse(&terrain, Some(units), Some(cities)).unwrap();
    let game = Game::from_scenario(Scenario { map, assets: None, warnings });
    // (The icon is the default unit's picture on a red disc.)
    let s = common::with_game(&assets(), Size::new(1024, 768), 96, game).unwrap();
    assert_eq!(s.stack().len(), 1);
    assert_eq!(s.slot_icon(0), Some(("units/nowhere.png", Side::Red)));
}
