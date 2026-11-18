//! Shared by the headless tests: a session on the repository's assets, and input
//! the way the window sends it.
#![allow(dead_code)]

use std::path::PathBuf;

use hexgame::session::Session;
use hexgame::state::Hex;
use turbo_ferret::{Modifiers, Point, Size, SpecialKey, TbId, WidgetId};

pub fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets")
}

/// The demo in a 1024 x 768 window at 96 DPI.
pub fn session() -> Session {
    session_at(1024, 768, 96)
}

pub fn session_at(w: i32, h: i32, dpi: i32) -> Session {
    Session::new(&assets(), Size::new(w, h), dpi).expect("the session starts")
}

/// The widget with this id in the layout.
pub fn widget(s: &Session, name: &str) -> WidgetId {
    s.ui.find(s.ui.root(), TbId::new(name)).unwrap_or_else(|| panic!("no widget {name}"))
}

/// The window pixel in the middle of a hex of the main map.
pub fn pixel_of(s: &Session, hex: Hex) -> Point {
    let [x, y] = s.view.to_ndc(s.view.hex_center(hex));
    let r = s.map_rect();
    Point::new(r.x + ((x + 1.0) / 2.0 * r.w as f32) as i32, r.y + ((1.0 - y) / 2.0 * r.h as f32) as i32)
}

/// Move the pointer to a window pixel and apply what that did.
pub fn hover(s: &mut Session, p: Point) {
    s.ui.pointer_move(p, Modifiers::NONE);
    s.pump();
}

/// Click (move, press, release) on a window pixel and apply what that did.
pub fn click(s: &mut Session, p: Point) {
    s.ui.pointer_move(p, Modifiers::NONE);
    s.ui.pointer_down(p, 1, Modifiers::NONE);
    s.ui.pointer_up(p, Modifiers::NONE);
    s.pump();
}

/// Press and release a special key. Returns whether the press was handled.
pub fn press(s: &mut Session, key: SpecialKey) -> bool {
    let handled = s.ui.key(None, key, Modifiers::NONE, true);
    s.ui.key(None, key, Modifiers::NONE, false);
    s.pump();
    handled
}

/// Type a character (a press and a release, as the window's char events arrive).
/// Returns whether the press was handled.
pub fn type_char(s: &mut Session, ch: char) -> bool {
    let handled = s.ui.key(Some(ch), SpecialKey::None, Modifiers::NONE, true);
    s.ui.key(Some(ch), SpecialKey::None, Modifiers::NONE, false);
    s.pump();
    handled
}
