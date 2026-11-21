//! Shared by the headless tests: a session on the repository's assets, and input
//! the way the window sends it.
#![allow(dead_code)]

use std::path::PathBuf;

use hexgame::game::Game;
use hexgame::session::Session;
use hexgame::state::{Hex, Map, MoveCost, Side, TileType};
use turbo_ferret::{Modifiers, Point, Size, SpecialKey, TbId, WidgetId};

pub fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets")
}

/// The grassland (`grassland`) in a 1024 x 768 window at 96 DPI.
pub fn session() -> Session {
    session_at(1024, 768, 96)
}

pub fn session_at(w: i32, h: i32, dpi: i32) -> Session {
    Session::with_game(&assets(), Size::new(w, h), dpi, grassland()).expect("the session starts")
}

/// A game on `map` with one black rider in its middle (grass 2, 20 moves),
/// and the cursor on it.
pub fn rider_on(mut map: Map) -> Game {
    let middle = Hex::new(map.rows() / 2, map.cols() / 2);
    let cost = map.acquire_move_cost(MoveCost { grass: 2, ..MoveCost::default() });
    let rider = map.new_unit(Side::Black, "Rider", 3, 5, 20, cost);
    map.place_unit(rider, middle).unwrap();
    map.cursor = middle;
    Game::new(map)
}

/// The tests' map (it was the C++ demo's): 100 x 100 grass with a mountain
/// diagonal, and one black rider ("Test Cav", 5 strong, 20 moves) in the
/// middle, on the diagonal (a mountain it could not enter), with the cursor.
pub fn grassland() -> Game {
    let mut map = Map::new(100, 100, TileType::Grass).unwrap();
    for i in 0..100 {
        map.tile_mut(Hex::new(i, i)).unwrap().kind = TileType::Mountain;
    }
    let costs = MoveCost {
        grass: 2,
        forest: 4,
        swamp: 5,
        desert: 3,
        hill: 4,
        bridge: 1,
        road: 2,
        city: 1,
        ..MoveCost::default()
    };
    let cost = map.acquire_move_cost(costs);
    let rider = map.new_unit(Side::Black, "Test Cav", 3, 5, 20, cost);
    map.place_unit(rider, Hex::new(50, 50)).unwrap();
    map.cursor = Hex::new(50, 50);
    Game::new(map)
}

/// The widget with this id in the layout.
pub fn widget(s: &Session, name: &str) -> WidgetId {
    s.screen.ui.find(s.screen.ui.root(), TbId::new(name)).unwrap_or_else(|| panic!("no widget {name}"))
}

/// Click the middle of the widget with this id (a button).
pub fn push_widget(s: &mut Session, name: &str) {
    let r = s.screen.area(widget(s, name));
    click(s, Point::new(r.x + r.w / 2, r.y + r.h / 2));
}

/// Choose an item of a menu as a player does: a click on its title, then on
/// the item.
pub fn choose(s: &mut Session, menu: &str, item: &str) {
    push_widget(s, menu);
    assert!(s.open_menu_items().is_some(), "{menu} opens");
    push_widget(s, item);
    assert!(s.open_menu_items().is_none(), "choosing {item} closes {menu}");
}

/// The widget with this id and all that is in it, the widget first.
pub fn descendants(s: &Session, name: &str) -> Vec<WidgetId> {
    let mut out = Vec::new();
    let mut stack = vec![widget(s, name)];
    while let Some(id) = stack.pop() {
        out.push(id);
        stack.extend(s.screen.ui.children(id).iter().copied());
    }
    out
}

/// The window pixel in the middle of a hex of the main map.
pub fn pixel_of(s: &Session, hex: Hex) -> Point {
    let [x, y] = s.screen.view.to_ndc(s.screen.view.hex_center(hex));
    let r = s.screen.map_rect();
    Point::new(r.x + ((x + 1.0) / 2.0 * r.w as f32) as i32, r.y + ((1.0 - y) / 2.0 * r.h as f32) as i32)
}

/// Move the pointer to a window pixel and apply what that did.
pub fn hover(s: &mut Session, p: Point) {
    s.screen.ui.pointer_move(p, Modifiers::NONE);
    s.pump();
}

/// Click (move, press, release) on a window pixel and apply what that did.
pub fn click(s: &mut Session, p: Point) {
    s.screen.ui.pointer_move(p, Modifiers::NONE);
    s.screen.ui.pointer_down(p, 1, Modifiers::NONE);
    s.screen.ui.pointer_up(p, Modifiers::NONE);
    s.pump();
}

/// Click on a window pixel with Shift held, and apply what that did.
pub fn shift_click(s: &mut Session, p: Point) {
    s.screen.ui.pointer_move(p, Modifiers::SHIFT);
    s.screen.ui.pointer_down(p, 1, Modifiers::SHIFT);
    s.screen.ui.pointer_up(p, Modifiers::SHIFT);
    s.pump();
}

/// Press and release a special key with Shift held.
pub fn shift_press(s: &mut Session, key: SpecialKey) -> bool {
    let handled = s.screen.ui.key(None, key, Modifiers::SHIFT, true);
    s.screen.ui.key(None, key, Modifiers::SHIFT, false);
    s.pump();
    handled
}

/// Press and release a special key. Returns whether the press was handled.
pub fn press(s: &mut Session, key: SpecialKey) -> bool {
    let handled = s.screen.ui.key(None, key, Modifiers::NONE, true);
    s.screen.ui.key(None, key, Modifiers::NONE, false);
    s.pump();
    handled
}

/// Type a character (a press and a release, as the window's char events arrive).
/// Returns whether the press was handled.
pub fn type_char(s: &mut Session, ch: char) -> bool {
    let handled = s.screen.ui.key(Some(ch), SpecialKey::None, Modifiers::NONE, true);
    s.screen.ui.key(Some(ch), SpecialKey::None, Modifiers::NONE, false);
    s.pump();
    handled
}
